use super::{Database, identity, session, social};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Response,
};
use oracle_market_data::spot::{Venue, book::level};
use oracle_spot_amount::spot_decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "lowercase", deny_unknown_fields)]
pub(super) enum Instruction {
    Quote {
        pair: String,
        side: String,
        quantity: String,
    },
    Confirm {
        id: String,
    },
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct MemberTicket {
    id: String,
    pair: String,
    side: String,
    quantity: String,
    price: String,
    fee: String,
    spread: String,
    total: String,
    currency: String,
    expires: String,
    as_of: String,
    settled: Option<String>,
}

pub(super) async fn handle(
    State(db): State<Database>,
    headers: HeaderMap,
    Json(input): Json<Instruction>,
) -> Response {
    social::response(
        "paper_spot_ticket_v1",
        &headers,
        apply(db, &headers, input).await,
    )
}

async fn apply(db: Database, headers: &HeaderMap, input: Instruction) -> Result<Value, StatusCode> {
    let login = identity(headers).ok_or(StatusCode::UNAUTHORIZED)?;
    let invalid = StatusCode::UNPROCESSABLE_ENTITY;
    static SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(4);
    let _permit = if matches!(input, Instruction::Quote { .. }) {
        Some(
            SLOTS
                .try_acquire()
                .map_err(|_| StatusCode::TOO_MANY_REQUESTS)?,
        )
    } else {
        None
    };
    let books = if let Instruction::Quote {
        ref pair,
        ref side,
        ref quantity,
    } = input
    {
        let _ = oracle_market_data::spot::pair(pair).map_err(|_| invalid)?;
        let buy = match side.as_str() {
            "buy" => true,
            "sell" => false,
            _ => return Err(invalid),
        };
        if !spot_decimal(quantity).is_some_and(|n| n.numer() > &0.into()) || quantity.len() > 32 {
            return Err(invalid);
        }
        let (binance, okx) = {
            let mut client = db.lock().await;
            let tx = client.transaction().await.map_err(social::failure)?;
            session(&tx, login.clone()).await?;
            let row = tx
                .query_one(
                    "SELECT oracle_spot_paper.enabled('binance'),oracle_spot_paper.enabled('okx')",
                    &[],
                )
                .await
                .map_err(social::failure)?;
            tx.commit().await.map_err(social::failure)?;
            (row.get::<_, bool>(0), row.get::<_, bool>(1))
        };
        let read = |venue, enabled| async move {
            if enabled {
                level(venue, pair, buy, quantity).await.ok()
            } else {
                None
            }
        };
        let (a, b) = tokio::join!(read(Venue::Binance, binance), read(Venue::Okx, okx));
        let valid: Vec<_> = [a, b].into_iter().flatten().collect();
        if valid.is_empty() {
            return Err(StatusCode::SERVICE_UNAVAILABLE);
        }
        json!(valid)
    } else {
        Value::Null
    };
    let mut client = db.lock().await;
    let tx = client.transaction().await.map_err(social::failure)?;
    session(&tx, login).await?;
    let raw:String=match input {
        Instruction::Confirm{id}=>tx.query_one("SELECT oracle_spot_paper.confirm($1::text::uuid)::text",&[&id]).await,
        Instruction::Quote{pair:symbol,side,quantity}=>{
            // Paper fee schedule v1 charges 10 bps on the modeled venue fill, in quote currency.
            tx.query_one("SELECT oracle_spot_paper.quote($1,$2,$3::text::numeric,jsonb_agg(level ||
                jsonb_build_object('fee',round((level->>'price')::numeric*$3::text::numeric/1000,8)::text,
                'feeSchedule','paper_taker_10bps_v1')))::text FROM jsonb_array_elements($4::text::jsonb) level",
                &[&symbol,&side,&quantity,&books.to_string()]).await
        }
    }.map_err(social::failure)?.get(0);
    let receipt: MemberTicket =
        serde_json::from_str(&raw).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tx.commit().await.map_err(social::failure)?;
    Ok(json!(receipt))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn member_ticket_contains_only_the_public_contract() {
        let mut public = json!({"id":"ticket","pair":"BTC/USD","side":"buy","quantity":"1",
            "price":"55000","fee":"5","spread":"1","total":"55005","currency":"USD",
            "expires":"2026-11-22T12:00:12Z","asOf":"2026-11-22T12:00:00Z","settled":null});
        for settlement in [Value::Null, json!("2026-11-22T12:00:05Z")] {
            public["settled"] = settlement;
            let mut stored = public.clone();
            stored["venue"] = json!("future-adapter");
            stored["venuePrice"] = json!("54999");
            stored["execution"] = json!({"counterparty":"future-adapter"});
            let shown: MemberTicket = serde_json::from_value(stored).unwrap();
            assert_eq!(json!(shown), public);
        }
        public.as_object_mut().unwrap().remove("price");
        assert!(serde_json::from_value::<MemberTicket>(public).is_err());
    }
}
