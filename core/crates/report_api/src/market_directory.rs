use axum::{
    body::Bytes,
    extract::Path,
    http::{HeaderName, StatusCode, header},
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

type Snapshot = Result<([(HeaderName, &'static str); 1], Bytes), StatusCode>;
type Cache = Mutex<Option<(Instant, Snapshot)>>;
static FIRST: Cache = Mutex::const_new(None);
static SECOND: Cache = Mutex::const_new(None);
type Symbols = HashMap<String, (String, String)>;
static SYMBOLS: Mutex<Option<(Instant, Symbols)>> = Mutex::const_new(None);

async fn read(client: &reqwest::Client, url: &str) -> Option<Value> {
    let mut response = client.get(url).send().await.ok()?.error_for_status().ok()?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.ok()? {
        if bytes.len() + chunk.len() > 24 * 1024 * 1024 {
            return None;
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).ok()
}

async fn symbols(client: &reqwest::Client) -> Option<Symbols> {
    let mut cache = SYMBOLS.lock().await;
    if let Some((at, names)) = cache.as_ref()
        && at.elapsed() < Duration::from_secs(3600)
    {
        return Some(names.clone());
    }
    let raw = read(
        client,
        "https://api.binance.com/api/v3/exchangeInfo?symbolStatus=TRADING&showPermissionSets=false",
    )
    .await?;
    let names: Symbols = raw["symbols"]
        .as_array()?
        .iter()
        .filter_map(|r| {
            if r["status"] != "TRADING" || r["isSpotTradingAllowed"] != true {
                return None;
            }
            Some((
                r["symbol"].as_str()?.into(),
                (
                    r["baseAsset"].as_str()?.into(),
                    r["quoteAsset"].as_str()?.into(),
                ),
            ))
        })
        .collect();
    *cache = Some((Instant::now(), names.clone()));
    Some(names)
}

async fn snapshot(first: bool) -> Option<Value> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .ok()?;
    let mut rows = Vec::new();
    if first {
        let (names, ticks) = tokio::join!(
            symbols(&client),
            read(
                &client,
                "https://api.binance.com/api/v3/ticker/24hr?symbolStatus=TRADING"
            )
        );
        let names = names?;
        for r in ticks?.as_array()? {
            if let Some((base, quote)) = r["symbol"].as_str().and_then(|s| names.get(s)) {
                rows.push(json!({"pair":format!("{base}/{quote}"),"quote":quote,"price":r["lastPrice"],"open":r["openPrice"],"volume":r["quoteVolume"],"ts":r["closeTime"]}));
            }
        }
    } else {
        let ticks = read(
            &client,
            "https://www.okx.com/api/v5/market/tickers?instType=SPOT",
        )
        .await?;
        if ticks["code"] != "0" {
            return None;
        }
        for r in ticks["data"].as_array()? {
            if r["instType"] == "SPOT"
                && let Some((base, quote)) = r["instId"].as_str().and_then(|s| s.split_once('-'))
                && !quote.contains('-')
            {
                rows.push(json!({"pair":format!("{base}/{quote}"),"quote":quote,"price":r["last"],"open":r["open24h"],"volume":r["volCcy24h"],"ts":r["ts"]}));
            }
        }
    }
    Some(Value::Array(rows))
}

pub(super) async fn feed(Path(route): Path<String>) -> Snapshot {
    let cache = match route.as_str() {
        "s1" => &FIRST,
        "s2" => &SECOND,
        _ => return Err(StatusCode::NOT_FOUND),
    };
    let mut state = cache.lock().await;
    if let Some((until, result)) = state.as_ref()
        && Instant::now() < *until
    {
        return result.clone();
    }
    let result = snapshot(route == "s1")
        .await
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)
        .and_then(|value| serde_json::to_vec(&value).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR))
        .map(|bytes| {
            (
                [(header::CONTENT_TYPE, "application/json")],
                Bytes::from(bytes),
            )
        });
    *state = Some((
        Instant::now() + Duration::from_secs(if result.is_ok() { 45 } else { 10 }),
        result.clone(),
    ));
    result
}
