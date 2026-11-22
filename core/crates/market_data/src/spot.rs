//! Discover active native spot instruments and fetch a bounded panel of closed minute prices.
use super::{MINUTE, PUBLIC_API, last_ms, utc_ms};
use ccxt::{
    Value,
    exchanges::{binance::BinanceCore, okx::OkxCore},
    runtime::call_typed,
};
use oracle_spot_amount::spot_decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};
use sha2::{Digest, Sha256};
use std::time::Duration;

const OKX: &str = "https://www.okx.com";
pub mod book;
pub const MAX_CLOSES: usize = 100;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Venue {
    Binance,
    Okx,
}

#[derive(Debug, Serialize)]
pub struct History {
    pub venue: Venue,
    pub base: String,
    pub quote: String,
    pub market_id: String,
    pub tick: String,
    pub quantity_step: String,
    pub first_open_ms: u64,
    pub received_ms: u64,
    pub metadata_sha256: String,
    pub candles_sha256: String,
    pub closes: Vec<String>,
}

fn digest(value: &Json) -> String {
    format!("{:x}", Sha256::digest(value.to_string().as_bytes()))
}

pub fn pair(symbol: &str) -> Result<(&str, &str), &'static str> {
    let (base, quote) = symbol.split_once('/').ok_or("expected native BASE/QUOTE")?;
    // Asset names can contain Unicode. Only venue metadata determines eligibility.
    let valid = |s: &str| {
        !s.is_empty()
            && s.len() <= 64
            && s.chars()
                .all(|c| !c.is_whitespace() && !c.is_control() && c != '/')
    };
    if !valid(base) || !valid(quote) || base == quote {
        return Err("invalid spot pair");
    }
    Ok((base, quote))
}

fn positive(value: &Json) -> Option<&str> {
    value
        .as_str()
        .filter(|s| spot_decimal(s).is_some_and(|n| n.numer() > &0.into()))
}

fn rules<'a>(
    venue: Venue,
    meta: &'a Json,
    base: &str,
    quote: &str,
    id: &str,
) -> Result<(&'a str, &'a str), &'static str> {
    let missing = "instrument is unavailable for spot trading";
    let rows = match venue {
        Venue::Binance => &meta["symbols"],
        Venue::Okx => &meta["data"],
    };
    let [r] = rows.as_array().ok_or(missing)?.as_slice() else {
        return Err(missing);
    };
    let (tick, step) = match venue {
        Venue::Binance => {
            if r["symbol"] != id
                || r["baseAsset"] != base
                || r["quoteAsset"] != quote
                || r["status"] != "TRADING"
                || r["isSpotTradingAllowed"] != true
            {
                return Err(missing);
            }
            let filters = r["filters"].as_array().ok_or(missing)?;
            let field = |kind, key: &str| {
                filters
                    .iter()
                    .find(|f| f["filterType"] == kind)
                    .and_then(|f| positive(&f[key]))
            };
            (
                field("PRICE_FILTER", "tickSize"),
                field("LOT_SIZE", "stepSize"),
            )
        }
        Venue::Okx => {
            if meta["code"] != "0"
                || r["instId"] != id
                || r["baseCcy"] != base
                || r["quoteCcy"] != quote
                || r["instType"] != "SPOT"
                || r["state"] != "live"
            {
                return Err(missing);
            }
            (positive(&r["tickSz"]), positive(&r["lotSz"]))
        }
    };
    Ok((
        tick.ok_or("missing positive tick")?,
        step.ok_or("missing positive lot step")?,
    ))
}

fn closes(
    venue: Venue,
    response: &Json,
    first: u64,
    count: usize,
) -> Result<Vec<String>, &'static str> {
    let data = match venue {
        Venue::Binance => response,
        Venue::Okx if response["code"] == "0" => &response["data"],
        Venue::Okx => return Err("OKX rejected the candle request"),
    };
    let rows = data.as_array().ok_or("invalid candle response")?;
    if rows.len() != count {
        return Err("missing or extra candles");
    }
    let mut result = Vec::with_capacity(count);
    for i in 0..count {
        let open = first + i as u64 * MINUTE;
        let row = match venue {
            Venue::Binance => &rows[i],
            Venue::Okx => &rows[count - 1 - i],
        };
        let valid = match venue {
            Venue::Binance => {
                row.as_array().is_some_and(|r| r.len() == 12)
                    && row[0].as_u64() == Some(open)
                    && row[6].as_u64() == Some(open + MINUTE - 1)
            }
            Venue::Okx => {
                row.as_array().is_some_and(|r| r.len() == 9)
                    && row[0].as_str().and_then(|s| s.parse().ok()) == Some(open)
                    && row[8] == "1"
            }
        };
        if !valid {
            return Err("unconfirmed, missing or misaligned minute");
        }
        let price = |j: usize| {
            positive(&row[j])
                .and_then(spot_decimal)
                .ok_or("invalid candle price")
        };
        let (o, h, l, c) = (price(1)?, price(2)?, price(3)?, price(4)?);
        if o < l || c < l || o > h || c > h {
            return Err("inconsistent candle range");
        }
        result.push(row[4].as_str().unwrap().to_owned());
    }
    Ok(result)
}

pub async fn history(
    venue: Venue,
    symbol: &str,
    final_open_ms: u64,
    count: usize,
) -> Result<History, &'static str> {
    let endpoint = match venue {
        Venue::Binance => PUBLIC_API,
        Venue::Okx => OKX,
    };
    read(venue, symbol, final_open_ms, count, endpoint).await
}

async fn read(
    venue: Venue,
    symbol: &str,
    last: u64,
    count: usize,
    endpoint: &str,
) -> Result<History, &'static str> {
    let (base, quote) = pair(symbol)?;
    if !(1..=MAX_CLOSES).contains(&count) {
        return Err("unsupported observation count");
    }
    let requested = utc_ms()?;
    let end = last_ms(last, requested)?;
    let first = last
        .checked_sub((count as u64 - 1) * MINUTE)
        .ok_or("invalid history window")?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none());
    let client = if endpoint == PUBLIC_API || endpoint == OKX {
        client
    } else {
        client.no_proxy()
    };
    let client = client.build().map_err(|_| "HTTP client unavailable")?;
    let id = match venue {
        Venue::Binance => format!("{base}{quote}"),
        Venue::Okx => format!("{base}-{quote}"),
    };
    let metadata_error = "venue instrument lookup failed";
    let candles_error = "venue candle request failed";
    let metadata_rules = |meta: &Json| {
        let (tick, step) = rules(venue, meta, base, quote, &id)?;
        Ok::<_, &'static str>((digest(meta), tick.to_owned(), step.to_owned()))
    };
    let (metadata_sha256, tick, quantity_step, data) = match venue {
        Venue::Binance => {
            let mut exchange = BinanceCore::new(None);
            exchange.urls = Value::from_json(&json!({"api":{"public":endpoint}}));
            exchange.internals.http_client = Some(client);
            let params = Value::from_json(&json!({"symbol":id}));
            let meta = call_typed(exchange.public_get_exchange_info(&[params]))
                .await
                .map_err(|_| metadata_error)?
                .to_json();
            let (hash, tick, step) = metadata_rules(&meta)?;
            let params = Value::from_json(
                &json!({"symbol":id,"interval":"1m","startTime":first,"endTime":end,"limit":count}),
            );
            let data = call_typed(exchange.public_get_klines(&[params]))
                .await
                .map_err(|_| candles_error)?;
            (hash, tick, step, data.to_json())
        }
        Venue::Okx => {
            let mut exchange = OkxCore::new(None);
            exchange.urls = Value::from_json(&json!({"api":{"rest":endpoint}}));
            exchange.internals.http_client = Some(client);
            let params = Value::from_json(&json!({"instType":"SPOT","instId":id}));
            let meta = call_typed(exchange.public_get_public_instruments(&[params]))
                .await
                .map_err(|_| metadata_error)?
                .to_json();
            let (hash, tick, step) = metadata_rules(&meta)?;
            let params =
                Value::from_json(&json!({"instId":id,"bar":"1m","after":last+1,"limit":count}));
            let data = call_typed(exchange.public_get_market_history_candles(&[params]))
                .await
                .map_err(|_| candles_error)?;
            (hash, tick, step, data.to_json())
        }
    };
    let received_ms = utc_ms()?;
    if received_ms < requested {
        return Err("clock moved backwards during venue read");
    }
    Ok(History {
        venue,
        base: base.into(),
        quote: quote.into(),
        market_id: id,
        tick,
        quantity_step,
        first_open_ms: first,
        received_ms,
        metadata_sha256,
        candles_sha256: digest(&data),
        closes: closes(venue, &data, first, count)?,
    })
}
