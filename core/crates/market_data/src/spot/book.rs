use super::*;

/// A paper execution is limited to one displayed level in an active native spot market.
#[derive(Debug, Serialize)]
pub struct BookLevel {
    pub venue: Venue,
    pub price: String,
    pub available: String,
    pub received_ms: u64,
    pub market_digest: String,
    pub book_digest: String,
}

pub async fn level(
    venue: Venue,
    symbol: &str,
    buy: bool,
    quantity: &str,
) -> Result<BookLevel, &'static str> {
    let (base, quote) = pair(symbol)?;
    let qty = spot_decimal(quantity).ok_or("Invalid quantity")?;
    let start = utc_ms()?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "Client unavailable")?;
    let (id, meta, book) = match venue {
        Venue::Binance => {
            let id = format!("{base}{quote}");
            let mut exchange = BinanceCore::new(None);
            exchange.urls = Value::from_json(&json!({"api":{"public":PUBLIC_API}}));
            exchange.internals.http_client = Some(client);
            let params = Value::from_json(&json!({"symbol":id}));
            let meta = call_typed(exchange.public_get_exchange_info(&[params]))
                .await
                .map_err(|_| "No venue rules")?
                .to_json();
            let params = Value::from_json(&json!({"symbol":id,"limit":5}));
            let book = call_typed(exchange.public_get_depth(&[params]))
                .await
                .map_err(|_| "No order book")?
                .to_json();
            (id, meta, book)
        }
        Venue::Okx => {
            let id = format!("{base}-{quote}");
            let mut exchange = OkxCore::new(None);
            exchange.urls = Value::from_json(&json!({"api":{"rest":OKX}}));
            exchange.internals.http_client = Some(client);
            let params = Value::from_json(&json!({"instType":"SPOT","instId":id}));
            let meta = call_typed(exchange.public_get_public_instruments(&[params]))
                .await
                .map_err(|_| "No venue rules")?
                .to_json();
            let params = Value::from_json(&json!({"instId":id,"sz":"1"}));
            let book = call_typed(exchange.public_get_market_books(&[params]))
                .await
                .map_err(|_| "No order book")?
                .to_json();
            (id, meta, book)
        }
    };
    let received_ms = utc_ms()?;
    let (tick, step) = rules(venue, &meta, base, quote, &id)?;
    if received_ms < start
        || received_ms - start > 10000
        || qty.numer() <= &0.into()
        || !(&qty / spot_decimal(step).ok_or("Invalid lot")?).is_integer()
    {
        return Err("Quantity or observation outside paper limits");
    }
    let depth = match venue {
        Venue::Binance if book["lastUpdateId"].as_u64().is_some() => &book,
        Venue::Okx if book["code"] == "0" => {
            let snapshot = &book["data"][0];
            let as_of = snapshot["ts"]
                .as_str()
                .and_then(|s| s.parse::<u64>().ok())
                .ok_or("Missing book time")?;
            if as_of > received_ms + 1000 || received_ms.saturating_sub(as_of) > 10000 {
                return Err("Stale book");
            }
            snapshot
        }
        _ => return Err("Missing book"),
    };
    let best = &depth[if buy { "asks" } else { "bids" }][0];
    let price = positive(&best[0]).ok_or("Missing price")?;
    let available = positive(&best[1]).ok_or("Missing size")?;
    let px = spot_decimal(price).ok_or("Invalid price")?;
    if spot_decimal(available).ok_or("Invalid size")? < qty
        || !(&px / spot_decimal(tick).ok_or("Invalid tick")?).is_integer()
    {
        return Err("Insufficient eligible depth");
    }
    let permitted = |value: &Json, amount: &num_rational::BigRational, minimum: bool| {
        value.as_str().and_then(spot_decimal).is_some_and(|limit| {
            limit.numer() == &0.into()
                || if minimum {
                    amount >= &limit
                } else {
                    amount <= &limit
                }
        })
    };
    if venue == Venue::Okx {
        if !permitted(&meta["data"][0]["minSz"], &qty, true) {
            return Err("Below minimum size");
        }
    } else {
        for filter in meta["symbols"][0]["filters"]
            .as_array()
            .ok_or("Missing limits")?
        {
            match filter["filterType"].as_str() {
                Some("LOT_SIZE" | "MARKET_LOT_SIZE") => {
                    if !permitted(&filter["minQty"], &qty, true)
                        || !permitted(&filter["maxQty"], &qty, false)
                    {
                        return Err("Outside quantity limits");
                    }
                    let step = filter["stepSize"]
                        .as_str()
                        .and_then(spot_decimal)
                        .ok_or("Missing size increment")?;
                    if step.numer() > &0.into() && !(&qty / step).is_integer() {
                        return Err("Outside quantity increment");
                    }
                }
                Some("MIN_NOTIONAL" | "NOTIONAL") => {
                    let value = &qty * &px;
                    if !permitted(&filter["minNotional"], &value, true)
                        || (filter["filterType"] == "NOTIONAL"
                            && !permitted(&filter["maxNotional"], &value, false))
                    {
                        return Err("Outside notional limits");
                    }
                }
                _ => {}
            }
        }
    }
    Ok(BookLevel {
        venue,
        price: price.to_owned(),
        available: available.to_owned(),
        received_ms,
        market_digest: digest(&meta),
        book_digest: digest(&book),
    })
}
