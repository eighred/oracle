use super::{Database, digest, identity, session};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use ring::rand::{SecureRandom, SystemRandom};
use serde_json::{Value, json};
use std::{
    env,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

fn issuer() -> Option<String> {
    let url = reqwest::Url::parse(&env::var("ORACLE_OIDC_ISSUER").ok()?).ok()?;
    if (url.scheme() != "https" && !(url.scheme() == "http" && url.host_str() == Some("127.0.0.1")))
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || !url.path().ends_with("/realms/oracle")
    {
        return None;
    }
    Some(url.to_string())
}

pub(super) async fn config() -> Json<Value> {
    Json(json!({"issuer":issuer(),"clientId":"oracle-web"}))
}
pub(super) async fn exchange(
    State(db): State<Database>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    let denied = StatusCode::UNAUTHORIZED;
    let unavailable = StatusCode::SERVICE_UNAVAILABLE;
    let issuer = issuer().ok_or(unavailable)?;
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .filter(|t| t.len() <= 16384)
        .ok_or(denied)?;
    // UserInfo validates this exact bearer at the configured issuer before any claim is trusted.
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| unavailable)?;
    let answer = client
        .get(format!("{issuer}/protocol/openid-connect/userinfo"))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|_| unavailable)?;
    if !answer.status().is_success() {
        return Err(if answer.status().is_server_error() {
            unavailable
        } else {
            denied
        });
    }
    let user: Value = serde_json::from_slice(&answer.bytes().await.map_err(|_| unavailable)?)
        .map_err(|_| denied)?;
    let claims: Value = serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(token.split('.').nth(1).ok_or(denied)?)
            .map_err(|_| denied)?,
    )
    .map_err(|_| denied)?;
    let subject = user["sub"].as_str().ok_or(denied)?;
    let methods = claims["amr"].as_array().ok_or(denied)?;
    let has = |method: &str| methods.iter().any(|value| value == method);
    let verified_factors = (has("pwd") && has("otp"))
        || (has("broker_otp")
            && matches!(claims["oracle_broker"].as_str(), Some("google" | "apple")));
    if !verified_factors
        || user["email_verified"] != true
        || claims["sub"] != subject
        || claims["iss"] != issuer
        || !matches!(claims["azp"].as_str(), Some("oracle-web" | "oracle-mobile"))
        || claims["typ"] != "Bearer"
    {
        return Err(denied);
    }
    let provider_session = claims["sid"]
        .as_str()
        .filter(|value| !value.is_empty() && value.len() <= 128)
        .ok_or(denied)?;
    let preferred = user["preferred_username"]
        .as_str()
        .filter(|s| {
            (3..=32).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        })
        .ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| unavailable)?
        .as_secs() as i64;
    let expiry = claims["exp"]
        .as_i64()
        .filter(|v| *v > now)
        .ok_or(denied)?
        .min(now + 300);
    let mut secret = [0u8; 32];
    SystemRandom::new()
        .fill(&mut secret)
        .map_err(|_| unavailable)?;
    let credential = URL_SAFE_NO_PAD.encode(secret);
    let client = db.lock().await;
    let handle: String = client
        .query_one(
            "SELECT oracle_member.issue($1,$2,$3,$4,$5,$6)",
            &[
                &issuer,
                &subject,
                &preferred,
                &digest(credential.as_bytes()),
                &expiry,
                &provider_session,
            ],
        )
        .await
        .map_err(|_| StatusCode::CONFLICT)?
        .get(0);
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        Json(json!({"username":handle,"token":credential,"expires":expiry})),
    )
        .into_response())
}
pub(super) async fn logout(
    State(db): State<Database>,
    headers: HeaderMap,
) -> Result<StatusCode, StatusCode> {
    let login = identity(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    let mut client = db.lock().await;
    let tx = client
        .transaction()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    session(&tx, login).await?;
    tx.execute("SELECT oracle_member.logout()", &[])
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    tx.commit()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(StatusCode::NO_CONTENT)
}
