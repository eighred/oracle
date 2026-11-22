use num_rational::BigRational as Number;
use oracle_market_data::spot::{Venue, history};
use oracle_spot_amount::spot_decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio_postgres::Client;

#[derive(Deserialize)]
struct Job {
    id: String,
    token: String,
    revision: i32,
    cutoff: u64,
    #[serde(rename = "markOnly")]
    mark_only: bool,
    cash: String,
    holdings: Vec<Holding>,
    targets: Vec<Target>,
}

#[derive(Deserialize, Serialize, Clone, Ord, PartialOrd, Eq, PartialEq)]
struct Instrument {
    venue: String,
    pair: String,
}

#[derive(Deserialize)]
struct Holding {
    #[serde(flatten)]
    instrument: Instrument,
    quantity: String,
}

#[derive(Deserialize)]
struct Target {
    #[serde(flatten)]
    instrument: Instrument,
    bps: i64,
}

fn ratio(n: i64, d: i64) -> Number {
    Number::new(n.into(), d.into())
}

fn amount(s: &str) -> Result<Number, &'static str> {
    spot_decimal(s).ok_or("Unsupported ledger precision")
}

// Posted amounts are finite decimals (price, lot, fee and their products).
fn decimal(n: &Number) -> String {
    let mut whole = (n.numer() / n.denom()).to_string();
    let mut remainder = n.numer() % n.denom();
    if remainder != 0.into() {
        whole.push('.');
        while remainder != 0.into() {
            remainder *= 10;
            whole.push_str(&(&remainder / n.denom()).to_string());
            remainder %= n.denom();
        }
    }
    whole
}

struct Leg {
    instrument: Instrument,
    before: Number,
    weight: i64,
    price: Number,
    step: Number,
    buy_price: Number,
    sell_price: Number,
    evidence: Value,
}

async fn leg(
    instrument: Instrument,
    before: Number,
    weight: i64,
    point: u64,
    fixture: bool,
) -> Result<Leg, &'static str> {
    let (price, step, tick, evidence) = if fixture {
        // Only the explicit offline worker command uses these generated observations.
        let price = if instrument.pair.starts_with("SOL/") {
            "100"
        } else {
            "50"
        };
        (
            price.to_owned(),
            "0.01".to_owned(),
            "0.01".to_owned(),
            json!({"source":"synthetic_test"}),
        )
    } else {
        let venue = match instrument.venue.as_str() {
            "binance" => Venue::Binance,
            "okx" => Venue::Okx,
            _ => return Err("Unsupported venue"),
        };
        let h = history(venue, &instrument.pair, point - 60000, 1).await?;
        let evidence = json!({"source":"ccxt_closed_minute","market":h.market_id,"receivedMs":h.received_ms,
            "metadataDigest":h.metadata_sha256,"candleDigest":h.candles_sha256});
        (h.closes[0].clone(), h.quantity_step, h.tick, evidence)
    };
    let price = amount(&price)?;
    let step = amount(&step)?;
    let tick = amount(&tick)?;
    let buy_price = (&price * ratio(10005, 10000) / &tick).ceil() * &tick;
    let sell_price = (&price * ratio(9995, 10000) / &tick).floor() * &tick;
    Ok(Leg {
        instrument,
        before,
        weight,
        price,
        step,
        buy_price,
        sell_price,
        evidence,
    })
}

fn now() -> Result<u64, &'static str> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "Clock unavailable")?
        .as_millis() as u64)
}

async fn receipt(job: &Job, fixture: bool) -> Result<Value, &'static str> {
    let mut instruments = BTreeMap::new();
    for h in &job.holdings {
        instruments.insert(h.instrument.clone(), (amount(&h.quantity)?, 0));
    }
    for t in &job.targets {
        instruments
            .entry(t.instrument.clone())
            .or_insert((ratio(0, 1), 0))
            .1 = t.bps;
    }
    let current = now()?;
    let point = if fixture || instruments.is_empty() {
        current
    } else {
        current / 60000 * 60000
    };
    if !instruments.is_empty() && point.saturating_sub(1) < job.cutoff {
        return Err("Waiting for a price after the instruction");
    }
    let mut legs = Vec::new();
    for (i, (q, w)) in instruments {
        legs.push(leg(i, q, w, point, fixture).await?);
    }
    if !fixture && now()?.saturating_sub(point) >= 60000 {
        return Err("Price window expired before all legs were observed");
    }
    if job.mark_only {
        let mut nav = amount(&job.cash)?;
        let mut marks = Vec::new();
        for leg in legs {
            nav += &leg.before * &leg.price;
            marks.push(json!({"venue":leg.instrument.venue,"pair":leg.instrument.pair,"after":decimal(&leg.before),
                "mark":decimal(&leg.price),"evidence":leg.evidence}));
        }
        return Ok(json!({"nav":decimal(&nav),"asOfMs":point,"marks":marks}));
    }
    calculate(&job.cash, point, &legs)
}

fn calculate(cash: &str, point: u64, legs: &[Leg]) -> Result<Value, &'static str> {
    let mut cash = amount(cash)?;
    let zero = ratio(0, 1);
    let fee = ratio(10, 10000);
    let mut nav = cash.clone();
    for leg in legs {
        nav += &leg.before * &leg.price;
    }
    let mut orders = Vec::new();
    let mut needed = zero.clone();
    for leg in legs {
        if leg.sell_price <= zero || !(&leg.before / &leg.step).is_integer() {
            return Err("Price or held quantity is outside current venue increments");
        }
        let desired =
            (&nav * ratio(leg.weight, 10000) / (&leg.price * &leg.step)).floor() * &leg.step;
        let delta = desired - &leg.before;
        if delta < zero {
            cash += -&delta * &leg.sell_price * (ratio(1, 1) - &fee);
        } else {
            needed += &delta * &leg.buy_price * (ratio(1, 1) + &fee);
        }
        orders.push(delta);
    }
    let scale = if needed > cash {
        &cash / needed
    } else {
        ratio(1, 1)
    };
    let mut holdings = Vec::new();
    let mut fills = Vec::new();
    let mut total_fees = zero.clone();
    let mut position_value = zero.clone();
    for (leg, delta) in legs.iter().zip(orders) {
        let (side, quantity, price, after) = if delta < zero {
            ("sell", -&delta, &leg.sell_price, &leg.before + &delta)
        } else {
            let buy = (delta * &scale / &leg.step).floor() * &leg.step;
            cash -= &buy * &leg.buy_price * (ratio(1, 1) + &fee);
            ("buy", buy.clone(), &leg.buy_price, &leg.before + buy)
        };
        let charge = &quantity * price * &fee;
        total_fees += &charge;
        position_value += &after * &leg.price;
        if after > zero {
            holdings.push(json!({"venue":leg.instrument.venue,"pair":leg.instrument.pair,"quantity":decimal(&after)}));
        }
        fills.push(json!({"venue":leg.instrument.venue,"pair":leg.instrument.pair,"targetBps":leg.weight,
            "before":decimal(&leg.before),"after":decimal(&after),"mark":decimal(&leg.price),"step":decimal(&leg.step),
            "side":side,"quantity":decimal(&quantity),"price":decimal(price),"fee":decimal(&charge),"evidence":leg.evidence}));
    }
    if cash < zero {
        return Err("Insufficient cash after modeled costs");
    }
    // Any amount written here must remain readable on the next rebalance.
    let cash_text = decimal(&cash);
    amount(&cash_text)?;
    Ok(
        json!({"model":"basket_paper_v1","asOfMs":point,"navBefore":decimal(&nav),
        "nav":decimal(&(cash+position_value)),"cash":cash_text,"fees":decimal(&total_fees),"holdings":holdings,"fills":fills}),
    )
}

pub(super) async fn run(client: Client, fixture: bool) -> Result<(), &'static str> {
    let allowed:bool = client.query_one("SELECT NOT rolsuper AND NOT rolbypassrls AND
        pg_has_role(current_user,'oracle_basket_worker','member') AND
        NOT pg_has_role(current_user,'oracle_report_reader','member') AND
        NOT pg_has_role(current_user,'oracle_report_writer','member') AND
        NOT pg_has_role(current_user,'oracle_paper_operator','member') FROM pg_roles WHERE rolname=current_user",
        &[]).await.map_err(|_|"Paper worker role check failed")?.get(0);
    if !allowed {
        return Err("A restricted basket worker login is required");
    }
    loop {
        let raw: Option<String> = client
            .query_one("SELECT oracle_basket_paper.claim()::text", &[])
            .await
            .map_err(|_| "Paper worker claim failed")?
            .get(0);
        let Some(raw) = raw else {
            tokio::time::sleep(Duration::from_secs(1)).await;
            continue;
        };
        let job: Job = serde_json::from_str(&raw).map_err(|_| "Invalid paper instruction")?;
        let (output, reason) = match receipt(&job, fixture).await {
            Ok(value) => (Some(value.to_string()), None),
            Err(reason) => (None, Some(reason)),
        };
        client.execute("SELECT oracle_basket_paper.finish($1::text::uuid,$2::text::uuid,$3,$4,$5::text::jsonb,$6)",
            &[&job.id,&job.token,&job.revision,&job.mark_only,&output,&reason]).await.map_err(|_|"Paper worker commit failed")?;
    }
}
