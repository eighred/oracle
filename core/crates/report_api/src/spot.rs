use super::{Database, authorize, identity};
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use oracle_market_data::spot::{MAX_CLOSES, Venue, history, pair};
use oracle_portfolio_replay::risk::basket::{Leg, SizeUnit, evaluate_sized};
use oracle_spot_amount::spot_decimal;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    io::Read,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Request {
    #[serde(default)]
    size_unit: SizeUnit,
    #[serde(default)]
    commentary: bool,
    cash: String,
    confidence_bps: u16,
    closes: usize,
    last_open_ms: Option<u64>,
    positions: Vec<Position>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Position {
    venue: Venue,
    pair: String,
    quantity: String,
    shock_bps: i32,
}

impl Request {
    fn validate(&self) -> Result<&str, &'static str> {
        if !(1..=8).contains(&self.positions.len())
            || !(5..=MAX_CLOSES).contains(&self.closes)
            || !(1..10_000).contains(&self.confidence_bps)
            || spot_decimal(&self.cash).is_none()
        {
            return Err(
                "Use 1–8 positions, 5–100 closes, nonnegative cash and confidence in 1–9999 bps.",
            );
        }
        let quote = pair(&self.positions[0].pair)?.1;
        let mut seen = HashSet::new();
        for p in &self.positions {
            if pair(&p.pair)?.1 != quote {
                return Err("A shared quote currency is required. FX conversion is unavailable.");
            }
            if !seen.insert((p.venue, &p.pair)) {
                return Err("Combine duplicate venue positions.");
            }
            if !spot_decimal(&p.quantity).is_some_and(|n| n.numer() > &0.into())
                || !(-10000..=100000).contains(&p.shock_bps)
            {
                return Err("Use a positive quantity and a shock between -10000 and 100000 bps.");
            }
        }
        Ok(quote)
    }
}

async fn calculate(input: &Request) -> Result<Value, &'static str> {
    let quote = input.validate()?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "system clock unavailable")?
        .as_millis() as u64;
    let last = input
        .last_open_ms
        .unwrap_or((now / 60_000).saturating_sub(1) * 60_000);
    let mut histories = Vec::new();
    for p in &input.positions {
        histories.push(history(p.venue, &p.pair, last, input.closes).await?);
    }
    let legs = input
        .positions
        .iter()
        .zip(&histories)
        .map(|(p, h)| Leg {
            quantity: &p.quantity,
            shock_bps: p.shock_bps,
            closes: &h.closes,
        })
        .collect::<Vec<_>>();
    let (risk, contributions) =
        evaluate_sized(&input.cash, &legs, input.confidence_bps, input.size_unit)?;
    let positions=input.positions.iter().zip(&histories).zip(contributions).map(|((p,h),c)|
        json!({"venue":h.venue,"pair":p.pair,"market_id":h.market_id,"quantity":p.quantity,"tick":h.tick,
            "quantity_step":h.quantity_step,"mark":h.closes.last(),"shock_bps":p.shock_bps,"stress_pnl":c.stress_pnl,"exposure":c.exposure,
            "expected_shortfall_contribution":c.expected_shortfall,"received_ms":h.received_ms,"metadata_sha256":h.metadata_sha256,"candles_sha256":h.candles_sha256})).collect::<Vec<_>>();
    Ok(
        json!({"schema_version":1,"source":"public_spot_research","size_unit":input.size_unit,"quote":quote,
        "first_open_ms":histories[0].first_open_ms,"as_of_ms":last+60_000,
        "known_at_ms":histories.iter().map(|h|h.received_ms).max(),"positions":positions,"risk":risk}),
    )
}

pub(super) async fn cli() -> Result<(), &'static str> {
    let mut raw = Vec::new();
    std::io::stdin()
        .take(16385)
        .read_to_end(&mut raw)
        .map_err(|_| "cannot read scenario")?;
    if raw.len() > 16384 {
        return Err("scenario exceeds 16 KiB");
    }
    let input = serde_json::from_slice(&raw).map_err(|_| "invalid scenario JSON")?;
    println!("{}", calculate(&input).await?);
    Ok(())
}

async fn access(db: &Database, portfolio: &str, login: (String, String)) -> Result<(), StatusCode> {
    let mut client = db.lock().await;
    let tx = client
        .build_transaction()
        .read_only(true)
        .start()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if portfolio == "member-scenario" && login.0.starts_with('@') {
        super::session(&tx, login).await?;
    } else {
        authorize(&tx, portfolio, login).await?;
    }
    tx.commit()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)
}

pub(super) async fn handle(
    State(db): State<Database>,
    Path(portfolio): Path<String>,
    headers: HeaderMap,
    Json(input): Json<Request>,
) -> Response {
    static SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);
    let Some(login) = identity(&headers) else {
        return (
            StatusCode::UNAUTHORIZED,
            [(
                header::WWW_AUTHENTICATE,
                "Basic realm=\"Oracle workstation\"",
            )],
        )
            .into_response();
    };
    if let Err(status) = access(&db, &portfolio, login.clone()).await {
        return status.into_response();
    }
    let Ok(_slot) = SLOTS.try_acquire() else {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    };
    let mut result = calculate(&input).await;
    if input.commentary
        && let Ok(report) = &mut result
    {
        if let Err(status) = access(&db, &portfolio, login.clone()).await {
            return status.into_response();
        }
        report["advisory"] = crate::advisory::explain(report).await;
    }
    if let Err(status) = access(&db, &portfolio, login).await {
        return status.into_response();
    }
    match result {
        Ok(value) => ([(header::CACHE_CONTROL, "no-store")], Json(value)).into_response(),
        Err(reason) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            [(header::CACHE_CONTROL, "no-store")],
            Json(json!({"reason":reason})),
        )
            .into_response(),
    }
}
