use super::{Database, digest, identity, session};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use ring::{
    pbkdf2,
    rand::{SecureRandom, SystemRandom},
};
use serde_json::{Value, json};
use tokio_postgres::error::SqlState;

type Failure = StatusCode;
pub(super) fn failure(e: tokio_postgres::Error) -> Failure {
    match e.code() {
        Some(&SqlState::NO_DATA_FOUND) => StatusCode::NOT_FOUND,
        Some(&SqlState::INSUFFICIENT_PRIVILEGE) => StatusCode::FORBIDDEN,
        Some(&SqlState::T_R_SERIALIZATION_FAILURE) => StatusCode::CONFLICT,
        Some(code) if matches!(&code.code()[..2], "22" | "23") => StatusCode::UNPROCESSABLE_ENTITY,
        _ => StatusCode::SERVICE_UNAVAILABLE,
    }
}
pub(super) fn response(
    policy: &str,
    headers: &HeaderMap,
    result: Result<Value, Failure>,
) -> Response {
    status_response(policy, headers, result.map(|value| (StatusCode::OK, value)))
}
pub(super) fn status_response(
    policy: &str,
    headers: &HeaderMap,
    result: Result<(StatusCode, Value), Failure>,
) -> Response {
    let status = match &result {
        Ok((status, _)) | Err(status) => *status,
    };
    let principal = if status == StatusCode::UNAUTHORIZED {
        None
    } else {
        identity(headers).map(|login| login.0)
    };
    eprintln!(
        "{}",
        json!({"policy":policy,"principal":principal,"status":status.as_u16(),"effect":if status.is_success(){"completed"}else{"refused"}})
    );
    let mut r = match result {
        Ok((status, v)) => (status, Json(v)).into_response(),
        Err(StatusCode::UNAUTHORIZED) => (
            StatusCode::UNAUTHORIZED,
            [(
                header::WWW_AUTHENTICATE,
                "Basic realm=\"Oracle workstation\"",
            )],
        )
            .into_response(),
        Err(status) => status.into_response(),
    };
    r.headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    r
}

pub(super) async fn list(State(db): State<Database>, headers: HeaderMap) -> Response {
    response("basket_read_v1", &headers, apply(&db, &headers, None).await)
}
pub(super) async fn change(
    State(db): State<Database>,
    headers: HeaderMap,
    Json(input): Json<Value>,
) -> Response {
    response(
        "basket_change_v1",
        &headers,
        apply(&db, &headers, Some(input)).await,
    )
}

async fn apply(db: &Database, headers: &HeaderMap, input: Option<Value>) -> Result<Value, Failure> {
    let login = identity(headers).ok_or(StatusCode::UNAUTHORIZED)?;
    let mut client = db.lock().await;
    let tx = client.transaction().await.map_err(failure)?;
    let member = login.0.starts_with('@');
    session(&tx, login).await?;
    let mut result = if let Some(input) = input {
        let invalid = StatusCode::UNPROCESSABLE_ENTITY;
        let op = input["op"].as_str().ok_or(invalid)?;
        let mut p = if op == "unlock" {
            json!({})
        } else {
            json!({"id":input["id"]})
        };
        let mut link = None;
        match op {
            "fund" | "redeem" | "deposit" => {
                p["request"] = json!(input["request"].as_str().ok_or(invalid)?);
                p["currency"] = input["currency"].clone();
                p["amount"] = input["amount"].clone();
                p["revision"] = input["revision"].clone();
            }
            "create" | "edit" => {
                p["items"] = input["items"].clone();
                if let Some(items) = p["items"].as_array_mut() {
                    for item in items {
                        match item["venue"].as_str() {
                            Some("s1") => item["venue"] = json!("binance"),
                            Some("s2") => item["venue"] = json!("okx"),
                            _ => {}
                        }
                    }
                }
                p["name"] = input["name"].clone();
                p["revision"] = input["revision"].clone();
            }
            "publish" | "pin" => p["enabled"] = json!(input["enabled"].as_bool().ok_or(invalid)?),
            "invite" | "uninvite" => p["member"] = json!(input["member"].as_str().ok_or(invalid)?),
            "clone" => p["revision"] = input["revision"].clone(),
            "join" | "leave" | "unlink" => {}
            "link" | "unlock" => {
                let password = input["password"]
                    .as_str()
                    .filter(|s| (12..=256).contains(&s.len()))
                    .ok_or(invalid)?
                    .to_owned();
                let token = if op == "link" {
                    let mut nonce = [0u8; 32];
                    SystemRandom::new()
                        .fill(&mut nonce)
                        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
                    let token = URL_SAFE_NO_PAD.encode(nonce);
                    link = Some(token.clone());
                    token
                } else {
                    input["token"]
                        .as_str()
                        .filter(|s| {
                            s.len() == 43
                                && s.bytes()
                                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
                        })
                        .ok_or(invalid)?
                        .to_owned()
                };
                p["token_hash"] = json!(digest(token.as_bytes()));
                let hash = tokio::task::spawn_blocking(move || {
                    let mut derived = [0u8; 32];
                    pbkdf2::derive(
                        pbkdf2::PBKDF2_HMAC_SHA256,
                        600_000.try_into().unwrap(),
                        token.as_bytes(),
                        password.as_bytes(),
                        &mut derived,
                    );
                    URL_SAFE_NO_PAD.encode(derived)
                })
                .await
                .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
                p["password_hash"] = json!(hash);
            }
            _ => return Err(invalid),
        }
        let command = if matches!(op, "fund" | "redeem" | "deposit") {
            "SELECT oracle_basket_paper.change($1,$2::text::jsonb)::text"
        } else {
            "SELECT oracle_social.change($1,$2::text::jsonb)::text"
        };
        let command = if op == "invite" && p["member"].as_str().is_some_and(|v| v.starts_with('@'))
        {
            "SELECT oracle_social.member_invite(($2::text::jsonb->>'id')::uuid,$2::text::jsonb->>'member')::text WHERE $1::text='invite'"
        } else {
            command
        };
        let id: String = tx
            .query_one(command, &[&op, &p.to_string()])
            .await
            .map_err(failure)?
            .get(0);
        json!({"id":id,"token":link})
    } else {
        let raw: String = tx
            .query_one("SELECT (oracle_social.library() || jsonb_build_object('paper',oracle_basket_paper.statement()))::text", &[])
            .await
            .map_err(failure)?
            .get(0);
        serde_json::from_str(&raw).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    };
    tx.commit().await.map_err(failure)?;
    if member {
        member_market_view(&mut result);
    }
    Ok(result)
}

pub(super) async fn admin(
    State(db): State<Database>,
    headers: HeaderMap,
    Json(input): Json<Value>,
) -> Response {
    response(
        "platform_settings_v1",
        &headers,
        settings(&db, &headers, input).await,
    )
}

async fn settings(db: &Database, headers: &HeaderMap, input: Value) -> Result<Value, Failure> {
    let login = identity(headers).ok_or(StatusCode::UNAUTHORIZED)?;
    let mut client = db.lock().await;
    let tx = client.transaction().await.map_err(failure)?;
    session(&tx, login).await?;
    let invalid = StatusCode::UNPROCESSABLE_ENTITY;
    let op = input["op"].as_str().ok_or(invalid)?;
    let p = match op {
        "list" => json!({}),
        "connection" => {
            json!({"request":input["request"],"venue":input["venue"],"enabled":input["enabled"].as_bool().ok_or(invalid)?})
        }
        "fee" => {
            json!({"request":input["request"],"effective":input["effective"].as_str().ok_or(invalid)?,"bps":input["bps"].as_u64().filter(|b|*b<=10000).ok_or(invalid)?})
        }
        _ => return Err(invalid),
    };
    let raw: String = tx
        .query_one(
            "SELECT oracle_admin.settings($1,$2::text::jsonb)::text",
            &[&op, &p.to_string()],
        )
        .await
        .map_err(failure)?
        .get(0);
    tx.commit().await.map_err(failure)?;
    let mut settings: Value =
        serde_json::from_str(&raw).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    settings["qualified"] = json!(["binance", "okx"]);
    Ok(settings)
}

#[derive(serde::Deserialize)]
pub(super) struct Page {
    before: Option<i64>,
}

pub(super) async fn profile(
    State(db): State<Database>,
    headers: HeaderMap,
    axum::extract::Query(page): axum::extract::Query<Page>,
) -> Response {
    let result = async {
        let login = identity(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
        let mut client = db.lock().await;
        let tx = client.transaction().await.map_err(failure)?;
        let member = login.0.starts_with('@');
        session(&tx, login).await?;
        let raw: String = tx
            .query_one(
                "SELECT oracle_basket_paper.profile($1)::text",
                &[&page.before],
            )
            .await
            .map_err(failure)?
            .get(0);
        tx.commit().await.map_err(failure)?;
        let mut profile: Value =
            serde_json::from_str(&raw).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if let Some(events) = profile["activity"].as_array_mut() {
            for entry in events {
                if let Some(fills) = entry["receipt"]
                    .get_mut("fills")
                    .and_then(Value::as_array_mut)
                {
                    for fill in fills {
                        if let Some(row) = fill.as_object_mut()
                            && let Some(price) = row.remove("customerPrice")
                        {
                            row.insert("price".into(), price);
                            row.remove("venue");
                        }
                    }
                }
            }
        }
        if member {
            member_market_view(&mut profile);
        }
        Ok(profile)
    }
    .await;
    response("paper_profile_v1", &headers, result)
}

#[derive(serde::Deserialize)]
pub(super) struct PublicBasket {
    id: Option<String>,
}
pub(super) async fn community(
    State(db): State<Database>,
    axum::extract::Query(q): axum::extract::Query<PublicBasket>,
) -> Response {
    let result = async {
        let client = db.lock().await;
        let value: String = client
            .query_one(
                "SELECT oracle_social.public_baskets($1::text::uuid)::text",
                &[&q.id],
            )
            .await
            .map_err(failure)?
            .get(0);
        let mut public =
            serde_json::from_str(&value).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        member_market_view(&mut public);
        Ok(public)
    }
    .await;
    response("published_baskets_v1", &HeaderMap::new(), result)
}

fn retain_member_rows(object: &mut serde_json::Map<String, Value>, key: &str, fields: &[&str]) {
    let Some(value) = object.get_mut(key) else {
        return;
    };
    let Some(rows) = value.as_array_mut() else {
        *value = json!([]);
        return;
    };
    rows.retain_mut(|row| {
        let Some(values) = row.as_object_mut() else {
            return false;
        };
        values.retain(|name, value| {
            fields.contains(&name.as_str()) && (value.is_string() || value.is_number())
        });
        true
    });
}

fn member_receipt(value: &mut Value) {
    let Some(receipt) = value.as_object_mut() else {
        *value = Value::Null;
        return;
    };
    receipt.retain(|key, v| match key.as_str() {
        "model" => v == "basket_paper_v1",
        "asOfMs" => v.is_u64(),
        "navBefore" | "nav" | "cash" | "fees" | "spread" => v.is_string(),
        "holdings" | "fills" => v.is_array(),
        _ => false,
    });
    retain_member_rows(
        receipt,
        "fills",
        &[
            "venue",
            "pair",
            "targetBps",
            "before",
            "after",
            "mark",
            "step",
            "side",
            "quantity",
            "price",
            "fee",
        ],
    );
}

/// Present stable routing references to members. Internal receipts retain the original identifiers.
pub(super) fn member_market_view(value: &mut Value) {
    match value {
        Value::Object(obj) => {
            obj.remove("evidence");
            obj.remove("market_id");
            if let Some(receipt) = obj.get_mut("receipt") {
                member_receipt(receipt);
            }
            retain_member_rows(obj, "holdings", &["venue", "pair", "quantity"]);
            if let Some(venue) = obj.get_mut("venue") {
                *venue = match venue.as_str() {
                    Some("binance") => json!("s1"),
                    Some("okx") => json!("s2"),
                    _ => Value::Null,
                };
            }
            if let Some(note) = obj.get_mut("note")
                && let Some(reason) = note.as_str()
            {
                *note = json!(match reason {
                    "Insufficient cash after modeled costs" =>
                        "Insufficient cash after modeled costs.",
                    "Price or held quantity is outside current venue increments" =>
                        "The current price or quantity is outside the permitted increments.",
                    _ => "The basket update is pending.",
                });
            }
            for child in obj.values_mut() {
                member_market_view(child);
            }
        }
        Value::Array(rows) => {
            for row in rows {
                member_market_view(row);
            }
        }
        _ => {}
    }
}

pub(super) fn names_provider(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    ["binance", "okx"].iter().any(|name| lower.contains(name))
}

pub(super) fn member_advisory(mut advisory: Value) -> Value {
    if let Some(fields) = advisory.as_object_mut() {
        fields.remove("model");
    }
    if names_provider(&advisory.to_string()) {
        return json!({"state":"unavailable"});
    }
    advisory
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn member_history_discards_unrecognized_routing_metadata() {
        let mut statement = json!({"note":"future-adapter returned an internal error",
            "receipt":{"futureRoute":"private","fees":"5","spread":"1",
                "fills":[{"side":"buy","quantity":"1","pair":"BTC/USD","price":"55000","fee":"5","exchangeAccount":"private","venue":"okx"}],
                "holdings":[{"pair":"BTC/USD","quantity":"1","venue":"binance","internalMarket":"private"}]}});
        member_market_view(&mut statement);
        assert_eq!(
            statement,
            json!({"note":"The basket update is pending.",
            "receipt":{"fees":"5","spread":"1",
                "fills":[{"side":"buy","quantity":"1","pair":"BTC/USD","price":"55000","fee":"5","venue":"s2"}],
                "holdings":[{"pair":"BTC/USD","quantity":"1","venue":"s1"}]}})
        );
    }

    #[test]
    fn member_commentary_omits_internal_names_and_model_aliases() {
        let advisory = json!({"state":"available","model":"internal-model","text":"Short sample."});
        assert_eq!(
            member_advisory(advisory),
            json!({"state":"available","text":"Short sample."})
        );
        for name in ["Binance", "OKX"] {
            assert_eq!(
                member_advisory(json!({"state":"available","text":name})),
                json!({"state":"unavailable"})
            );
        }
    }
}
