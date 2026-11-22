use ccxt::{Value, exchanges::binance::BinanceCore, runtime::call_typed};
use oracle_spot_amount::Amount;
use serde::Serialize;
use serde_json::{Value as Json, json};
use sha2::{Digest, Sha256};
use std::{
    env,
    process::ExitCode,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const PUBLIC_API: &str = "https://data-api.binance.vision/api/v3";
const MINUTE: u64 = 60_000;

fn utc_ms() -> Result<u64, &'static str> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| u64::try_from(elapsed.as_millis()).ok())
        .ok_or("system clock unavailable")
}

fn last_ms(open_ms: u64, now: u64) -> Result<u64, &'static str> {
    let end = open_ms.checked_add(MINUTE).ok_or("invalid minute")?;
    if !open_ms.is_multiple_of(MINUTE) || end > now || end > i64::MAX as u64 {
        return Err("expected the start timestamp of a fully closed UTC minute");
    }
    Ok(end - 1)
}

#[derive(Serialize)]
struct Observation {
    schema_version: u8,
    venue: &'static str,
    market_id: &'static str,
    quote: &'static str,
    base: &'static str,
    method: &'static str,
    open_ms: u64,
    close_ms_inclusive: u64,
    received_ms: u64,
    open: Amount,
    high: Amount,
    low: Amount,
    close: Amount,
    base_volume: Amount,
    response_json_sha256: String,
    source_response: Json,
    rights_status: &'static str,
}

fn normalize(raw: Json, open_ms: u64, received_ms: u64) -> Result<Observation, &'static str> {
    let close_ms = last_ms(open_ms, received_ms)?;
    let rows = raw.as_array().ok_or("invalid kline response")?;
    if rows.is_empty() {
        return Err("missing requested minute");
    }
    let [row] = rows.as_slice() else {
        return Err("ambiguous result for one minute");
    };
    let fields = row.as_array().ok_or("invalid kline row")?;
    if fields.len() != 12
        || fields[0].as_u64() != Some(open_ms)
        || fields[6].as_u64() != Some(close_ms)
    {
        return Err("venue interval differs from the requested minute");
    }
    let amount = |i: usize| {
        fields[i]
            .as_str()
            .and_then(Amount::parse)
            .ok_or("invalid spot decimal")
    };
    let (open, high, low, close, base_volume) =
        (amount(1)?, amount(2)?, amount(3)?, amount(4)?, amount(5)?);
    if low == Amount::ZERO || low > open.min(close) || high < open.max(close) {
        return Err("inconsistent OHLC range");
    }
    // The upstream engine returns parsed JSON. Hash its compact array representation.
    let digest = Sha256::digest(raw.to_string().as_bytes());
    Ok(Observation {
        schema_version: 1,
        venue: "binance",
        market_id: "BTCUSDT",
        quote: "USDT",
        base: "BTC",
        method: "public_get_klines",
        open_ms,
        close_ms_inclusive: close_ms,
        received_ms,
        open,
        high,
        low,
        close,
        base_volume,
        response_json_sha256: format!("{digest:x}"),
        source_response: raw,
        rights_status: "unreviewed",
    })
}

async fn read_minute(base_url: &str, open_ms: u64) -> Result<Observation, &'static str> {
    let requested_ms = utc_ms()?;
    let end_ms = last_ms(open_ms, requested_ms)?;
    let mut venue = BinanceCore::new(None);
    venue.urls = Value::from_json(&json!({"api": {"public": base_url}}));
    venue.internals.http_client = Some(
        reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| "HTTP client unavailable")?,
    );
    let params = Value::from_json(&json!({
        "symbol": "BTCUSDT", "interval": "1m", "startTime": open_ms, "endTime": end_ms, "limit": 1,
    }));
    let result = call_typed(venue.public_get_klines(&[params])).await;
    let received_ms = utc_ms()?;
    if received_ms < requested_ms {
        return Err("clock moved backwards during venue read");
    }
    let raw = result.map_err(|error| {
        if error.is("RateLimitExceeded") || error.is("DDoSProtection") {
            "venue throttled the read"
        } else {
            "venue read failed"
        }
    })?;
    normalize(raw.to_json(), open_ms, received_ms)
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    // CCXT catches exchange exceptions after Rust invokes the panic hook. The
    // exchange message can contain the response body. Keep it off the CLI log.
    std::panic::set_hook(Box::new(|_| eprintln!("market data runtime exception")));
    let args: Vec<_> = env::args().skip(1).collect();
    let [arg] = args.as_slice() else {
        eprintln!("usage: oracle-market-data OPEN_MS");
        return ExitCode::FAILURE;
    };
    let Ok(open_ms) = arg.parse() else {
        eprintln!("invalid UTC timestamp");
        return ExitCode::FAILURE;
    };
    match read_minute(PUBLIC_API, open_ms).await {
        Ok(observation) => {
            println!("{}", json!(observation));
            ExitCode::SUCCESS
        }
        Err(reason) => {
            eprintln!(
                "{}",
                json!({"market_id":"BTCUSDT","open_ms":open_ms,"completeness":"incomplete","reason":reason})
            );
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests;
