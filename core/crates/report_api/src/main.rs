mod advisory;
mod basket_worker;
mod community;
mod market_directory;
mod member;
mod news;
mod paper;
mod social;
mod spot;
mod spot_ticket;
use axum::{
    Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use oracle_portfolio_replay::{FIXTURE, replay, risk};
use ring::pbkdf2;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{env, net::Ipv4Addr, process::ExitCode, sync::Arc};
use tokio::{net::TcpListener, sync::Mutex};
use tokio_postgres::{Client, NoTls, config::Host};
use tower_http::services::ServeDir;

type Database = Arc<Mutex<Client>>;

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn canonical_bytes(mut value: serde_json::Value) -> Result<Vec<u8>, &'static str> {
    // CCXT enables insertion-order JSON maps. Existing addressed reports use sorted object keys.
    value.sort_all_objects();
    serde_json::to_vec(&value).map_err(|_| "report encoding failed")
}

async fn connect() -> Result<Client, &'static str> {
    let config = env::var("ORACLE_DATABASE_URL")
        .ok()
        .and_then(|s| s.parse::<tokio_postgres::Config>().ok())
        .ok_or("configure ORACLE_DATABASE_URL")?;
    if config.get_hosts().is_empty()
        || config.get_hosts().iter().any(|host| match host {
            Host::Tcp(h) => h != "127.0.0.1" && h != "::1",
            #[cfg(unix)]
            Host::Unix(_) => false,
        })
    {
        return Err("the local pilot requires a loopback address or Unix database socket");
    }
    let (client, connection) = config
        .connect(NoTls)
        .await
        .map_err(|_| "database connection failed")?;
    tokio::spawn(async move {
        if connection.await.is_err() {
            eprintln!("database connection closed");
        }
    });
    Ok(client)
}

async fn publish(client: &Client) -> Result<(), &'static str> {
    let snapshot =
        replay(FIXTURE, 1767225660000, 1767225660100).map_err(|_| "invalid opening snapshot")?;
    let risk = risk::report(&snapshot, 6250).map_err(|_| "invalid risk report")?;
    let backtest = oracle_backtest::report().map_err(|_| "invalid backtest")?;
    let payload = canonical_bytes(
        json!({"schema_version":1,"source_kind":"synthetic_historical",
        "risk":risk,"backtest":backtest}),
    )?;
    let id = digest(&payload);
    client.execute("INSERT INTO oracle_read.reports(portfolio,report_id,payload) VALUES($1,$2,$3) ON CONFLICT DO NOTHING",
        &[&"own-paper-01",&id,&payload]).await.map_err(|_|"report publication failed")?;
    println!("{id}");
    Ok(())
}

fn identity(headers: &HeaderMap) -> Option<(String, String)> {
    let basic = headers
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Basic ")?;
    let bytes = STANDARD.decode(basic).ok()?;
    let (user, token) = std::str::from_utf8(&bytes).ok()?.split_once(':')?;
    if user.is_empty() || user.len() > 128 || !(16..=256).contains(&token.len()) {
        return None;
    }
    Some((user.to_owned(), token.to_owned()))
}

async fn session(
    tx: &tokio_postgres::Transaction<'_>,
    login: (String, String),
) -> Result<(), StatusCode> {
    let failed = |_| StatusCode::SERVICE_UNAVAILABLE;
    if login.0.starts_with('@') {
        let proof = digest(login.1.as_bytes());
        tx.query_one("SELECT set_config('oracle.operator',$1,true),set_config('oracle.token_digest',$2,true)", &[&login.0,&proof]).await.map_err(failed)?;
        return tx
            .query_one("SELECT oracle_read.principal()", &[])
            .await
            .map_err(failed)?
            .get::<_, Option<String>>(0)
            .map(|_| ())
            .ok_or(StatusCode::UNAUTHORIZED);
    }
    let salt: Option<Vec<u8>> = tx
        .query_one("SELECT oracle_read.login_salt($1)", &[&login.0])
        .await
        .map_err(failed)?
        .get(0);
    let salt = salt.ok_or(StatusCode::UNAUTHORIZED)?;
    let verifier = tokio::task::spawn_blocking(move || {
        let mut derived = [0u8; 32];
        pbkdf2::derive(
            pbkdf2::PBKDF2_HMAC_SHA256,
            std::num::NonZeroU32::new(600_000).unwrap(),
            &salt,
            login.1.as_bytes(),
            &mut derived,
        );
        derived.to_vec()
    })
    .await
    .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    tx.query_one(
        "SELECT set_config('oracle.operator',$1,true),set_config('oracle.token_digest',encode($2::bytea,'hex'),true)",
        &[&login.0, &verifier],
    )
    .await
    .map_err(failed)?;
    let principal: Option<String> = tx
        .query_one("SELECT oracle_read.principal()", &[])
        .await
        .map_err(failed)?
        .get(0);
    principal.map(|_| ()).ok_or(StatusCode::UNAUTHORIZED)
}

async fn authorize(
    tx: &tokio_postgres::Transaction<'_>,
    resource: &str,
    login: (String, String),
) -> Result<(), StatusCode> {
    session(tx, login).await?;
    let failed = |_| StatusCode::SERVICE_UNAVAILABLE;
    let access = tx
        .query_one(
            "SELECT oracle_read.principal(),oracle_read.allowed($1)",
            &[&resource],
        )
        .await
        .map_err(failed)?;
    let principal: Option<String> = access.get(0);
    let allowed: bool = access.get(1);
    let decision = if principal.is_none() {
        "unauthenticated"
    } else if allowed {
        "allow"
    } else {
        "deny"
    };
    eprintln!(
        "{}",
        json!({"policy":"portfolio_grant_v1","principal":principal,"resource":resource,"decision":decision})
    );
    if principal.is_none() {
        return Err(StatusCode::UNAUTHORIZED);
    }
    if !allowed {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(())
}

async fn lookup(
    db: &Database,
    resource: &str,
    kind: &str,
    version: &str,
    login: (String, String),
) -> Result<(String, Vec<u8>), StatusCode> {
    let failed = |_| StatusCode::SERVICE_UNAVAILABLE;
    let mut client = db.lock().await;
    let tx = client
        .build_transaction()
        .read_only(true)
        .start()
        .await
        .map_err(failed)?;
    let member = login.0.starts_with('@');
    authorize(&tx, resource, login).await?;
    // Original research receipts and their provider identifiers are operator-only.
    if member {
        return Err(StatusCode::FORBIDDEN);
    }
    let row = tx.query_opt("SELECT report_id,payload FROM oracle_read.reports WHERE portfolio=$1 AND ($2='latest' OR report_id=$2)
        AND kind=$3 ORDER BY published_at DESC,report_id LIMIT 1",&[&resource,&version,&kind]).await.map_err(failed)?.ok_or(StatusCode::NOT_FOUND)?;
    let id: String = row.get(0);
    let bytes: Vec<u8> = row.get(1);
    if digest(&bytes) != id {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    tx.commit().await.map_err(failed)?;
    Ok((id, bytes))
}

async fn report(
    State(db): State<Database>,
    Path((portfolio, collection, version)): Path<(String, String, String)>,
    headers: HeaderMap,
) -> Response {
    let kind = match collection.as_str() {
        "reports" => "historical",
        "paper" => "paper",
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    let result = match identity(&headers) {
        Some(login) => lookup(&db, &portfolio, kind, &version, login).await,
        None => Err(StatusCode::UNAUTHORIZED),
    };
    let mut response = match result {
        Ok((id, bytes)) => {
            let source = if kind == "paper" {
                serde_json::from_slice::<serde_json::Value>(&bytes)
                    .ok()
                    .and_then(|paper| match paper["manifest"]["source_kind"].as_str()? {
                        "prospective_synthetic" => Some("synthetic"),
                        "ccxt_loopback_fixture" => Some("fixture"),
                        "ccxt_binance_spot" => Some("market"),
                        _ => None,
                    })
            } else {
                None
            };
            let mut response = (
                [
                    (header::CONTENT_TYPE.as_str(), "application/json"),
                    ("x-oracle-report-id", &id),
                ],
                bytes,
            )
                .into_response();
            if let Some(source) = source {
                response.headers_mut().insert(
                    "x-oracle-paper-source",
                    axum::http::HeaderValue::from_static(source),
                );
            }
            response
        }
        Err(StatusCode::UNAUTHORIZED) => (
            StatusCode::UNAUTHORIZED,
            [(
                header::WWW_AUTHENTICATE,
                "Basic realm=\"Oracle workstation\", charset=\"UTF-8\"",
            )],
        )
            .into_response(),
        Err(status) => status.into_response(),
    };
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
}

async fn run() -> Result<(), &'static str> {
    let args: Vec<_> = env::args().skip(1).collect();
    match args.as_slice() {
        [cmd] if matches!(cmd.as_str(), "basket-worker" | "basket-worker-fixture") => {
            basket_worker::run(connect().await?, cmd == "basket-worker-fixture").await
        }
        [cmd] if cmd == "spot-risk" => spot::cli().await,
        [cmd, rest @ ..] if cmd == "paper" => paper::command(&mut connect().await?, rest).await,
        [cmd] if cmd == "publish-synthetic" => publish(&connect().await?).await,
        [cmd, dist] if cmd == "serve" => {
            if !std::path::Path::new(dist).join("index.html").is_file() {
                return Err("build the Vue workstation first");
            }
            let client = connect().await?;
            let role=client.query_one("SELECT NOT rolsuper AND NOT rolbypassrls AND pg_has_role(current_user,'oracle_report_reader','member')
                AND NOT pg_has_role(current_user,'oracle_basket_worker','member')
                AND NOT pg_has_role(current_user,'oracle_report_writer','member')
                AND NOT pg_has_role(current_user,'oracle_paper_operator','member')
                FROM pg_roles WHERE rolname=current_user",&[]).await.map_err(|_|"reader role check failed")?;
            if !role.get::<_, bool>(0) {
                return Err("serve requires a restricted report reader");
            }
            let app = Router::new()
                .route("/api/auth/config", get(member::config))
                .route(
                    "/api/auth/session",
                    axum::routing::post(member::exchange).delete(member::logout),
                )
                .route(
                    "/api/paper/spot",
                    axum::routing::post(spot_ticket::handle)
                        .layer(axum::extract::DefaultBodyLimit::max(2048)),
                )
                .route("/api/profile", get(social::profile))
                .route(
                    "/api/community",
                    get(community::read)
                        .post(community::change)
                        .layer(axum::extract::DefaultBodyLimit::max(36_000)),
                )
                .route("/api/discover/baskets", get(social::community))
                .route("/api/discover/news", get(news::feed))
                .route("/api/discover/markets/{route}", get(market_directory::feed))
                .route(
                    "/api/platform-settings",
                    axum::routing::post(social::admin)
                        .layer(axum::extract::DefaultBodyLimit::max(4096)),
                )
                .route(
                    "/api/baskets",
                    get(social::list)
                        .post(social::change)
                        .layer(axum::extract::DefaultBodyLimit::max(16_384)),
                )
                .route(
                    "/api/portfolios/{portfolio}/{collection}/{version}",
                    get(report),
                )
                .route(
                    "/api/portfolios/{portfolio}/spot-risk",
                    axum::routing::post(spot::handle)
                        .layer(axum::extract::DefaultBodyLimit::max(16_384)),
                )
                .with_state(Arc::new(Mutex::new(client)))
                .fallback_service(ServeDir::new(dist));
            let port = env::var("ORACLE_PORT")
                .unwrap_or_else(|_| "8040".into())
                .parse::<u16>()
                .map_err(|_| "invalid port")?;
            let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))
                .await
                .map_err(|_| "loopback bind failed")?;
            println!(
                "Listening on {}",
                listener.local_addr().map_err(|_| "address unavailable")?
            );
            axum::serve(listener, app)
                .await
                .map_err(|_| "server stopped unexpectedly")
        }
        _ => Err("usage: oracle-report-api publish-synthetic | serve WEB_DIST"),
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    if matches!(
        env::args().nth(1).as_deref(),
        Some("paper" | "serve" | "spot-risk" | "basket-worker" | "basket-worker-fixture")
    ) {
        // CCXT translates venue exceptions after the panic hook runs. Keep bodies off the journal log.
        std::panic::set_hook(Box::new(|_| eprintln!("Oracle runtime exception")));
    }
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(reason) => {
            eprintln!("{reason}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod encoding_tests {
    #[test]
    fn the_original_forward_experiment_keeps_its_manifest_digest_with_ccxt_linked() {
        let manifest = oracle_backtest::paper::plan(1795313760000).unwrap();
        assert_eq!(
            super::digest(&super::canonical_bytes(manifest).unwrap()),
            "98408511b66332f3187f3a23760bcb2e4331bbba44bf0cda1477f9b366663193"
        );
    }
}
