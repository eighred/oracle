use oracle_market_data::spot::{Venue, history};
use serde::Deserialize;
use serde_json::{Value, json};
use std::time::Duration;
use tokio_postgres::Client;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Job {
    pair: String,
    token: String,
    close_ms: u64,
    first: bool,
    second: bool,
}

async fn observe(job: &Job, venue: Venue, enabled: bool) -> Option<Value> {
    if !enabled {
        return None;
    }
    let mark = history(venue, &job.pair, job.close_ms.checked_sub(60000)?, 1)
        .await
        .ok()?;
    Some(json!({
        "pair": job.pair, "venue": venue, "market": mark.market_id,
        "closeMs": mark.first_open_ms+60000, "receivedMs": mark.received_ms, "price": mark.closes.first()?,
        "metadataDigest": mark.metadata_sha256, "candleDigest": mark.candles_sha256
    }))
}

pub(super) async fn run(client: &Client) -> Result<(), &'static str> {
    loop {
        let raw: Option<String> = client
            .query_one("SELECT oracle_spot_paper.claim_mark()::text", &[])
            .await
            .map_err(|_| "Spot valuation claim failed")?
            .get(0);
        let Some(raw) = raw else {
            tokio::time::sleep(Duration::from_secs(3)).await;
            continue;
        };
        let job: Job = serde_json::from_str(&raw).map_err(|_| "Invalid spot valuation job")?;
        let (first, second) = tokio::join!(
            observe(&job, Venue::Binance, job.first),
            observe(&job, Venue::Okx, job.second)
        );
        let observations: Vec<_> = [first, second].into_iter().flatten().collect();
        client
            .execute(
                "SELECT oracle_spot_paper.finish_mark($1,$2::text::uuid,$3::text::jsonb)",
                &[&job.pair, &job.token, &json!(observations).to_string()],
            )
            .await
            .map_err(|_| "Spot valuation commit failed")?;
    }
}
