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
fn failure(e: tokio_postgres::Error) -> Failure {
    match e.code() {
        Some(&SqlState::NO_DATA_FOUND) => StatusCode::NOT_FOUND,
        Some(&SqlState::INSUFFICIENT_PRIVILEGE) => StatusCode::FORBIDDEN,
        Some(&SqlState::T_R_SERIALIZATION_FAILURE) => StatusCode::CONFLICT,
        Some(code) if matches!(&code.code()[..2], "22" | "23") => StatusCode::UNPROCESSABLE_ENTITY,
        _ => StatusCode::SERVICE_UNAVAILABLE,
    }
}
fn response(policy: &str, headers: &HeaderMap, result: Result<Value, Failure>) -> Response {
    let status = result.as_ref().err().copied().unwrap_or(StatusCode::OK);
    let principal = if status == StatusCode::UNAUTHORIZED {
        None
    } else {
        identity(headers).map(|login| login.0)
    };
    eprintln!(
        "{}",
        json!({"policy":policy,"principal":principal,"status":status.as_u16(),"effect":if result.is_ok(){"completed"}else{"refused"}})
    );
    let mut r = match result {
        Ok(v) => Json(v).into_response(),
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
    session(&tx, login).await?;
    let result = if let Some(input) = input {
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
    serde_json::from_str(&raw).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
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
        serde_json::from_str(&raw).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
    }
    .await;
    response("paper_profile_v1", &headers, result)
}
