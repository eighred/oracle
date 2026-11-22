use super::{Database, identity, session, social};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Response,
};
use num_rational::BigRational;
use oracle_market_data::spot::{Venue, history, pair};
use oracle_portfolio_replay::risk::basket::{Leg, SizeUnit, evaluate_sized};
use oracle_spot_amount::spot_decimal;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::BTreeMap, time::Duration};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Parameters {
    currency: String,
    closes: usize,
    confidence_bps: u16,
    shock_bps: i32,
    #[serde(default)]
    commentary: bool,
}

#[derive(Deserialize, PartialEq)]
enum Source {
    #[serde(rename = "Paper spot")]
    Spot,
    #[serde(rename = "binance")]
    First,
    #[serde(rename = "okx")]
    Second,
}
#[derive(Deserialize)]
struct Holding {
    pair: String,
    source: Source,
    quantity: String,
    value: String,
    as_of_ms: u64,
}

#[derive(Deserialize)]
struct Snapshot {
    currency: String,
    available: String,
    basket_cash: String,
    cash: String,
    nav: String,
    captured_ms: u64,
    holdings: Vec<Holding>,
    first: bool,
    second: bool,
}

type Error = &'static str;
const COVERAGE: Error = "A complete price history is unavailable for this wallet.";
fn decimal(raw: &str) -> Result<BigRational, Error> {
    spot_decimal(raw).ok_or("Wallet amounts are outside the supported precision.")
}
impl Snapshot {
    fn reconcile(&self) -> Result<(), Error> {
        let cash = decimal(&self.cash)?;
        if decimal(&self.available)? + decimal(&self.basket_cash)? != cash {
            return Err("Wallet cash does not reconcile.");
        }
        let mut total = cash;
        for h in &self.holdings {
            if pair(&h.pair)?.1 != self.currency
                || decimal(&h.quantity)?.numer() <= &0.into()
                || decimal(&h.value)?.numer() <= &0.into()
            {
                return Err("Wallet holdings do not reconcile.");
            }
            if h.as_of_ms > self.captured_ms || self.captured_ms - h.as_of_ms > 600_000 {
                return Err(
                    "An open holding needs a current mark. Refresh the wallet and try again.",
                );
            }
            total += decimal(&h.value)?;
        }
        if total != decimal(&self.nav)? {
            return Err("Wallet value does not reconcile.");
        }
        Ok(())
    }
}

struct Series {
    closes: Vec<String>,
    received_ms: u64,
    candle_digest: String,
    metadata_digest: String,
}
async fn series(
    venue: Venue,
    enabled: bool,
    symbol: &str,
    last: u64,
    count: usize,
) -> Option<Series> {
    if !enabled {
        return None;
    }
    let h = history(venue, symbol, last, count).await.ok()?;
    Some(Series {
        closes: h.closes,
        received_ms: h.received_ms,
        candle_digest: h.candles_sha256,
        metadata_digest: h.metadata_sha256,
    })
}

type Histories = BTreeMap<String, (Option<Series>, Option<Series>)>;
async fn load(s: &Snapshot, last: u64, count: usize) -> Result<Histories, Error> {
    let mut wanted = BTreeMap::<String, (bool, bool)>::new();
    for h in &s.holdings {
        let feeds = wanted.entry(h.pair.clone()).or_default();
        feeds.0 |= s.first && h.source != Source::Second;
        feeds.1 |= s.second && h.source != Source::First;
    }
    let mut waiting = wanted.into_iter();
    let mut running = tokio::task::JoinSet::new();
    let mut data = BTreeMap::new();
    loop {
        while running.len() < 3 {
            let Some((symbol, (first, second))) = waiting.next() else {
                break;
            };
            running.spawn(async move {
                let panels = tokio::join!(
                    series(Venue::Binance, first, &symbol, last, count),
                    series(Venue::Okx, second, &symbol, last, count)
                );
                (symbol, panels)
            });
        }
        let Some(done) = running.join_next().await else {
            break;
        };
        let (symbol, panels) = done.map_err(|_| COVERAGE)?;
        data.insert(symbol, panels);
    }
    Ok(data)
}

fn consolidated(a: Option<&[String]>, b: Option<&[String]>) -> Result<Vec<String>, Error> {
    match (a, b) {
        (Some(a), Some(b)) if a.len() == b.len() => a
            .iter()
            .zip(b)
            .map(|(x, y)| {
                Ok(if decimal(x)? >= decimal(y)? {
                    x.clone()
                } else {
                    y.clone()
                })
            })
            .collect(),
        (Some(a), None) | (None, Some(a)) => Ok(a.to_vec()),
        _ => Err(COVERAGE),
    }
}

struct Panel {
    value: String,
    closes: Vec<String>,
}
async fn assess(s: &Snapshot, p: &Parameters) -> Result<Value, Error> {
    s.reconcile()?;
    let last = (s.captured_ms / 60_000 - 1) * 60_000;
    let data = load(s, last, p.closes).await?;
    summarize(s, p, last, &data)
}
fn summarize(s: &Snapshot, p: &Parameters, last: u64, data: &Histories) -> Result<Value, Error> {
    let mut report = json!({"schema_version":1,"currency":s.currency,"captured_ms":s.captured_ms,"available_cash":s.available,
        "basket_cash":s.basket_cash,"cash":s.cash,"nav":s.nav,"positions":[],"state":"cash_only"});
    if s.holdings.is_empty() {
        return Ok(report);
    }

    let mut by_pair = BTreeMap::<&str, Vec<&Holding>>::new();
    for h in &s.holdings {
        by_pair.entry(&h.pair).or_default().push(h);
    }
    let mut panels = Vec::new();
    let mut details = Vec::new();
    let mut group_sizes = Vec::new();
    let mut partial = false;
    for (symbol, rows) in by_pair {
        let (one, two) = data.get(symbol).ok_or(COVERAGE)?;
        let a = one.as_ref().map(|h| h.closes.as_slice());
        let b = two.as_ref().map(|h| h.closes.as_slice());
        let mut quantity = BigRational::from_integer(0.into());
        let mut exposure = quantity.clone();
        group_sizes.push(rows.len());
        for h in rows {
            let closes = match h.source {
                Source::First => a.ok_or(COVERAGE)?.to_vec(),
                Source::Second => b.ok_or(COVERAGE)?.to_vec(),
                Source::Spot => {
                    partial |= (s.first && a.is_none()) || (s.second && b.is_none());
                    consolidated(a, b)?
                }
            };
            quantity += decimal(&h.quantity)?;
            exposure += decimal(&h.value)?;
            panels.push(Panel {
                value: h.value.clone(),
                closes,
            });
        }
        let inputs:Vec<_>=[one,two].into_iter().flatten().map(|h|json!({
            "received_ms":h.received_ms,"candles_sha256":h.candle_digest,"metadata_sha256":h.metadata_digest
        })).collect();
        details.push(json!({"pair":symbol,"quantity":quantity.to_string(),"exposure":exposure.to_string(),"inputs":inputs}));
    }
    let legs = panels
        .iter()
        .map(|pnl| Leg {
            quantity: &pnl.value,
            closes: &pnl.closes,
            shock_bps: p.shock_bps,
        })
        .collect::<Vec<_>>();
    let (risk, contributions) = evaluate_sized(&s.cash, &legs, p.confidence_bps, SizeUnit::Quote)?;
    let mut contributions = contributions.into_iter();
    for (row, count) in details.iter_mut().zip(group_sizes) {
        let mut total = BigRational::from_integer(0.into());
        for c in contributions.by_ref().take(count) {
            total += c
                .expected_shortfall
                .parse::<BigRational>()
                .map_err(|_| "Invalid tail attribution.")?;
        }
        row["tail_contribution"] = json!(total.to_string());
    }
    report["risk"] = json!(risk);
    report["positions"] = json!(details);
    report["state"] = json!("valued");
    report["partial_market_coverage"] = json!(partial);
    report["first_open_ms"] = json!(last - (p.closes as u64 - 1) * 60_000);
    report["market_close_ms"] = json!(last + 60_000);
    report["oldest_mark_ms"] = json!(s.holdings.iter().map(|h| h.as_of_ms).min());
    Ok(report)
}

pub(super) async fn handle(
    State(db): State<Database>,
    headers: HeaderMap,
    Json(p): Json<Parameters>,
) -> Response {
    let result = async {
        let login = identity(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
        let raw: String = {
            let mut client = db.lock().await;
            let tx = client.transaction().await.map_err(social::failure)?;
            session(&tx, login.clone()).await?;
            if p.currency.is_empty()
                || p.currency.len() > 64
                || !(5..=100).contains(&p.closes)
                || !(1..10000).contains(&p.confidence_bps)
                || !(-10000..=100000).contains(&p.shock_bps)
            {
                return Err(StatusCode::UNPROCESSABLE_ENTITY);
            }
            let row = tx
                .query_opt(include_str!("wallet_risk.sql"), &[&p.currency])
                .await
                .map_err(social::failure)?
                .ok_or(StatusCode::NOT_FOUND)?;
            tx.commit().await.map_err(social::failure)?;
            row.get(0)
        };
        let snapshot: Snapshot =
            serde_json::from_str(&raw).map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?;
        let _permit = super::spot::SLOTS
            .try_acquire()
            .map_err(|_| StatusCode::TOO_MANY_REQUESTS)?;
        let mut output = tokio::time::timeout(Duration::from_secs(50), assess(&snapshot, &p))
            .await
            .unwrap_or(Err(COVERAGE));
        if p.commentary
            && let Ok(report) = &mut output
            && report["state"] == "valued"
        {
            verify_session(&db, login.clone()).await?;
            let advisory = super::advisory::explain(report).await;
            report["advisory"] = social::member_advisory(advisory);
        }
        verify_session(&db, login).await?;
        Ok(match output {
            Ok(v) => (StatusCode::OK, v),
            Err(reason) => (StatusCode::UNPROCESSABLE_ENTITY, json!({"reason":reason})),
        })
    }
    .await;
    social::status_response("wallet_risk_v1", &headers, result)
}

async fn verify_session(db: &Database, login: (String, String)) -> Result<(), StatusCode> {
    let mut client = db.lock().await;
    let tx = client.transaction().await.map_err(social::failure)?;
    session(&tx, login).await?;
    tx.commit().await.map_err(social::failure)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spot_and_basket_exposures_use_joint_losses_on_the_booked_wallet() {
        let captured = 60_000_000;
        let mut wallet:Snapshot = serde_json::from_value(json!({
            "captured_ms":captured,"currency":"USD","available":"25","basket_cash":"15","cash":"40","nav":"100",
            "first":true,"second":true,"holdings":[
                {"source":"Paper spot","pair":"BTC/USD","quantity":"1","value":"20","as_of_ms":captured},
                {"source":"binance","pair":"BTC/USD","quantity":"2","value":"40","as_of_ms":captured}
            ]
        })).unwrap();
        let p = Parameters {
            currency: "USD".into(),
            closes: 5,
            confidence_bps: 7500,
            shock_bps: -1000,
            commentary: false,
        };
        let sample = |prices: [&str; 5]| {
            Some(Series {
                closes: prices.map(str::to_owned).to_vec(),
                received_ms: captured,
                candle_digest: "test".into(),
                metadata_digest: "test".into(),
            })
        };
        let mut market = BTreeMap::from([(
            "BTC/USD".into(),
            (
                sample(["10", "10", "8", "8", "10"]),
                sample(["9", "10", "10", "8", "9"]),
            ),
        )]);
        wallet.reconcile().unwrap();
        let report = summarize(&wallet, &p, captured - 60000, &market).unwrap();
        // The four joint losses are 0, 8, 4 and -15 USD. Fixed cash does not react to a spot shock.
        assert_eq!(report["risk"]["nav"], "100");
        assert_eq!(report["risk"]["expected_shortfall"], "8");
        assert_eq!(report["risk"]["stress_pnl"], "-6");
        assert_eq!(report["positions"].as_array().unwrap().len(), 1);
        assert_eq!(report["positions"][0]["tail_contribution"], "8");
        assert_eq!(report["positions"][0]["quantity"], "3");
        market.get_mut("BTC/USD").unwrap().1 = None;
        assert_eq!(
            summarize(&wallet, &p, captured - 60000, &market).unwrap()["partial_market_coverage"],
            true
        );
        market.get_mut("BTC/USD").unwrap().0 = None;
        assert_eq!(
            summarize(&wallet, &p, captured - 60000, &market).unwrap_err(),
            COVERAGE
        );
        wallet.nav = "101".into();
        assert!(wallet.reconcile().is_err());
        wallet.nav = "100".into();
        wallet.holdings[0].as_of_ms -= 600_001;
        assert!(wallet.reconcile().is_err());
    }
}
