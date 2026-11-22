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
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(8))
        .user_agent("OracleNews/1.0 (+https://github.com/eighred/oracle)")
        .build()
        .ok()?;
    let (desk, telegraph) = tokio::join!(
        download(&client, "https://www.coindesk.com/arc/outboundfeeds/rss/"),
        download(&client, "https://cointelegraph.com/rss")
    );
    let mut articles = Vec::new();
    for (content, publisher, hosts) in [
        (desk, "CoinDesk", &["www.coindesk.com", "coindesk.com"][..]),
        (telegraph, "Cointelegraph", &["cointelegraph.com"][..]),
    ] {
        if let Some(bytes) = content {
            articles.extend(rss(&bytes, publisher, hosts));
        }
    }
    articles.sort_by_key(|item| std::cmp::Reverse(item["publishedMs"].as_i64()));
    let mut seen = std::collections::HashSet::new();
    articles.retain(|item| seen.insert(item["url"].clone().to_string()));
    articles.truncate(60);
    (!articles.is_empty()).then(|| json!({"articles":articles}))
}

fn rss(bytes: &[u8], source: &str, hosts: &[&str]) -> Vec<Value> {
    let Some(xml) = std::str::from_utf8(bytes)
        .ok()
        .and_then(|text| roxmltree::Document::parse(text).ok())
    else {
        return Vec::new();
    };
    xml.descendants()
        .filter(|n| n.has_tag_name("item"))
        .filter_map(|item| {
            let field = |name: &str| {
                item.children()
                    .find(|c| c.has_tag_name(name))?
                    .text()
                    .map(str::trim)
            };
            let title = field("title")?;
            let url = reqwest::Url::parse(field("link")?).ok()?;
            let time = chrono::DateTime::parse_from_rfc2822(field("pubDate")?)
                .ok()?
                .timestamp_millis();
            let label = format!("{title} {url}").to_ascii_lowercase();
            (!super::social::names_provider(&label)
                && title.len() <= 600
                && !title.is_empty()
                && time > 0
                && url.scheme() == "https"
                && url.username().is_empty()
                && url.password().is_none()
                && url.port().is_none()
                && hosts.contains(&url.host_str()?))
            .then(|| json!({"title":title,"publishedMs":time,"source":source,"url":url.as_str()}))
        })
        .take(40)
        .collect()
}

async fn download(client: &reqwest::Client, url: &str) -> Option<Vec<u8>> {
    let mut res = client.get(url).send().await.ok()?.error_for_status().ok()?;
    let mut bytes = Vec::new();
    while let Some(chunk) = res.chunk().await.ok()? {
        if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
            return None;
        }
        bytes.extend_from_slice(&chunk);
    }
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rss_requires_publisher_link_and_publication_date() {
        let source = br#"<rss><channel>
        <item><title>A &amp; B</title><link>https://news.test/story</link><pubDate>Sat, 21 Nov 2026 12:00:00 GMT</pubDate></item>
        <item><title>Other host</title><link>https://other.test/story</link><pubDate>Sat, 21 Nov 2026 12:00:00 GMT</pubDate></item>
        </channel></rss>"#;
        let articles = rss(source, "News", &["news.test"]);
        assert_eq!(articles.len(), 1);
        assert_eq!(articles[0]["title"], "A & B");
        assert_eq!(articles[0]["publishedMs"], 1795262400000_i64);
    }
}
