use super::{Database, identity, session, social};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Response,
};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Replacement {
    revision: i32,
    pairs: Vec<String>,
}

pub(super) async fn read(State(db): State<Database>, headers: HeaderMap) -> Response {
    social::status_response("watchlist_v1", &headers, apply(db, &headers, None).await)
}

pub(super) async fn replace(
    State(db): State<Database>,
    headers: HeaderMap,
    Json(input): Json<Replacement>,
) -> Response {
    social::status_response(
        "watchlist_v1",
        &headers,
        apply(db, &headers, Some(input)).await,
    )
}

async fn apply(
    db: Database,
    headers: &HeaderMap,
    input: Option<Replacement>,
) -> Result<(StatusCode, Value), StatusCode> {
    let login = identity(headers).ok_or(StatusCode::UNAUTHORIZED)?;
    let (expected, desired) = if let Some(list) = input {
        if list.revision < 0
            || list.pairs.len() > 100
            || list
                .pairs
                .iter()
                .any(|p| oracle_market_data::spot::pair(p).is_err())
        {
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }
        (Some(list.revision), Some(list.pairs))
    } else {
        (None, None)
    };
    let mut client = db.lock().await;
    let tx = client.transaction().await.map_err(social::failure)?;
    session(&tx, login).await?;
    let row = tx
        .query_one(
            "SELECT revision, pairs, conflict FROM oracle_social.watchlist($1,$2)",
            &[&expected, &desired],
        )
        .await
        .map_err(social::failure)?;
    tx.commit().await.map_err(social::failure)?;
    let status = if row.get("conflict") {
        StatusCode::CONFLICT
    } else {
        StatusCode::OK
    };
    Ok((
        status,
        json!({"revision":row.get::<_, i32>("revision"), "pairs":row.get::<_, Vec<String>>("pairs")}),
    ))
}
