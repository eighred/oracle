use super::{Database, identity, session, social};
use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::Response,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use image::{
    DynamicImage, ImageDecoder,
    codecs::jpeg::{JpegDecoder, JpegEncoder},
};
use serde::Deserialize;
use serde_json::Value;
use std::io::Cursor;

#[derive(Deserialize)]
pub(super) struct Feed {
    view: Option<String>,
    handle: Option<String>,
    before: Option<i64>,
    currency: Option<String>,
}

pub(super) async fn read(
    State(db): State<Database>,
    headers: HeaderMap,
    Query(feed): Query<Feed>,
) -> Response {
    let result = async {
        let login = identity(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
        let owner = feed.handle.unwrap_or_else(|| login.0.clone());
        let mut client = db.lock().await;
        let tx = client.transaction().await.map_err(social::failure)?;
        session(&tx, login).await?;
        let ranking = feed.view.as_deref() == Some("rankings");
        let raw = if ranking {
            let currency = feed.currency.as_deref().unwrap_or("USDT");
            if currency.len() > 16
                || !currency
                    .bytes()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
            {
                return Err(StatusCode::UNPROCESSABLE_ENTITY);
            }
            tx.query_one("SELECT oracle_social.rankings($1)::text", &[&currency])
                .await
        } else {
            tx.query_one(
                "SELECT oracle_social.community($1,$2,$3)::text",
                &[
                    &feed.view.as_deref().unwrap_or("global"),
                    &owner,
                    &feed.before,
                ],
            )
            .await
        };
        let v: String = raw.map_err(social::failure)?.get(0);
        tx.commit().await.map_err(social::failure)?;
        serde_json::from_str(&v).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
    }
    .await;
    social::response("trader_feed_v1", &headers, result)
}

fn avatar(raw: &str) -> Option<String> {
    if raw.is_empty() {
        return Some(String::new());
    }
    let bytes = STANDARD
        .decode(raw.strip_prefix("data:image/jpeg;base64,")?)
        .ok()?;
    if bytes.len() > 24500 {
        return None;
    }
    let decoder = JpegDecoder::new(Cursor::new(bytes)).ok()?;
    let (w, h) = decoder.dimensions();
    if !(1..=128).contains(&w) || !(1..=128).contains(&h) {
        return None;
    }
    let decoded = DynamicImage::from_decoder(decoder).ok()?;
    let mut data = Vec::new();
    JpegEncoder::new_with_quality(&mut data, 75)
        .encode_image(&decoded)
        .ok()?;
    Some(format!("data:image/jpeg;base64,{}", STANDARD.encode(data)))
}

pub(super) async fn change(
    State(db): State<Database>,
    headers: HeaderMap,
    Json(mut payload): Json<Value>,
) -> Response {
    let invalid = StatusCode::UNPROCESSABLE_ENTITY;
    let result = async {
        let login = identity(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
        let op = payload["op"].as_str().ok_or(invalid)?.to_owned();
        let mut client = db.lock().await;
        let tx = client.transaction().await.map_err(social::failure)?;
        session(&tx, login).await?;
        if op == "profile" {
            payload["avatar"] =
                Value::String(avatar(payload["avatar"].as_str().ok_or(invalid)?).ok_or(invalid)?);
        } else if op == "post" {
            oracle_market_data::spot::pair(payload["pair"].as_str().ok_or(invalid)?)
                .map_err(|_| invalid)?;
        }
        tx.execute(
            "SELECT oracle_social.community_change($1,$2::text::jsonb)",
            &[&op, &payload.to_string()],
        )
        .await
        .map_err(social::failure)?;
        tx.commit().await.map_err(social::failure)?;
        Ok(serde_json::json!({"saved":true}))
    }
    .await;
    social::response("trader_change_v1", &headers, result)
}
