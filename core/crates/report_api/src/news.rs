use axum::{Json, http::StatusCode};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

type FeedResult = Result<Json<Value>, StatusCode>;
static CACHE: Mutex<Option<(Instant, FeedResult)>> = Mutex::const_new(None);

pub(super) async fn feed() -> FeedResult {
    let mut cache = CACHE.lock().await;
    if let Some((until, result)) = cache.as_ref()
        && Instant::now() < *until
    {
        return result.clone();
    }
    let result = latest()
        .await
        .map(Json)
        .ok_or(StatusCode::SERVICE_UNAVAILABLE);
    let seconds = if result.is_ok() { 300 } else { 30 };
    *cache = Some((
        Instant::now() + Duration::from_secs(seconds),
        result.clone(),
    ));
    result
}

async fn latest() -> Option<Value> {
    let mut res = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(8))
        .build()
        .ok()?
        .get("https://www.okx.com/api/v5/support/announcements?page=1")
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?;
    let mut bytes = Vec::new();
    while let Some(chunk) = res.chunk().await.ok()? {
        if bytes.len() + chunk.len() > 131072 {
            return None;
        }
        bytes.extend_from_slice(&chunk);
    }
    headlines(&serde_json::from_slice(&bytes).ok()?)
}

fn headlines(raw: &Value) -> Option<Value> {
    if raw["code"] != "0" {
        return None;
    }
    let mut articles = Vec::new();
    for item in raw["data"][0]["details"].as_array()? {
        let (Some(title), Some(url), Some(time)) = (
            item["title"].as_str(),
            item["url"].as_str(),
            item["pTime"].as_str().and_then(|s| s.parse::<i64>().ok()),
        ) else {
            continue;
        };
        if !title.trim().is_empty()
            && title.len() <= 600
            && time > 0
            && time < 8640000000000000
            && url.starts_with("https://www.okx.com/help/")
            && !url.chars().any(|c| c.is_control() || c == '\\')
        {
            articles.push((
                time,
                json!({"title":title,"url":url,"publishedMs":time,"source":"OKX"}),
            ));
        }
    }
    articles.sort_by_key(|(time, _)| std::cmp::Reverse(*time));
    Some(json!({"articles":articles.into_iter().take(10).map(|(_,item)|item).collect::<Vec<_>>()}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn headlines_keep_publisher_links_and_publication_order() {
        let article = |url, time| json!({"url":url,"pTime":time,"title":"Announcement"});
        let result = headlines(&json!({"code":"0","data":[{"details":[
            article("https://www.okx.com/help/older","20"),article("javascript:alert(1)","90"),
            article("https://www.okx.com/help/newer","40")]}]}))
        .unwrap();
        assert_eq!(result["articles"].as_array().unwrap().len(), 2);
        assert_eq!(result["articles"][0]["publishedMs"], 40);
    }
}
