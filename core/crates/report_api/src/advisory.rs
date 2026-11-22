//! Optional commentary on the completed research calculation.
use serde_json::{Value, json};
use std::{env, time::Duration};

fn context(report: &Value) -> Value {
    let risk = &report["risk"];
    let direction = match risk["stress_pnl"].as_str() {
        Some("0") => "flat",
        Some(v) if v.starts_with('-') => "loss",
        Some(_) => "gain",
        None => "unknown",
    };
    json!({"model":risk["model"],"one_minute_scenarios":risk["sample_size"],"confidence_bps":risk["confidence_bps"],
        "position_count":report["positions"].as_array().map(Vec::len),"stress_direction":direction})
}

pub async fn explain(report: &Value) -> Value {
    let model = env::var("ORACLE_LITELLM_MODEL")
        .ok()
        .filter(|m| !m.trim().is_empty());
    let port = env::var("ORACLE_LITELLM_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .filter(|p| *p != 0);
    let (Some(model), Some(port)) = (model, port) else {
        return json!({"state":"unconfigured"});
    };
    match completion(
        port,
        &model,
        context(report),
        env::var("ORACLE_LITELLM_KEY").ok(),
    )
    .await
    {
        Some(text) => json!({"state":"available","model":model,"text":text}),
        None => json!({"state":"unavailable"}),
    }
}

async fn completion(port: u16, model: &str, context: Value, key: Option<String>) -> Option<String> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(12))
        .build()
        .ok()?;
    let mut req = client
        .post(format!("http://127.0.0.1:{port}/v1/chat/completions"))
        .header("Content-Type", "application/json");
    if let Some(key) = key {
        req = req.bearer_auth(key);
    }
    let body = json!({"model":model,"max_tokens":220,"messages":[
        {"role":"system","content":"Explain this hypothetical spot risk model's coverage and sampling limitations in plain English. Use only the supplied facts. Give no trade instructions or invented numbers."},
        {"role":"user","content":context.to_string()}]});
    let mut res = req
        .body(body.to_string())
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?;
    let mut bytes = Vec::new();
    while let Some(chunk) = res.chunk().await.ok()? {
        if bytes.len() + chunk.len() > 32768 {
            return None;
        }
        bytes.extend_from_slice(&chunk);
    }
    let parsed: Value = serde_json::from_slice(&bytes).ok()?;
    let content = parsed["choices"][0]["message"]["content"].as_str()?.trim();
    if content.is_empty() || content.len() > 8192 {
        return None;
    }
    Some(content.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn local_gateway_accepts_chat_and_returns_only_commentary() {
        use std::io::{BufRead, BufReader, Read, Write};
        let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = server.local_addr().unwrap().port();
        let worker = std::thread::spawn(move || {
            let (mut stream, _) = server.accept().unwrap();
            let mut request = BufReader::new(&stream);
            let mut line = String::new();
            request.read_line(&mut line).unwrap();
            assert!(line.starts_with("POST /v1/chat/completions "));
            let mut length = 0;
            loop {
                line.clear();
                request.read_line(&mut line).unwrap();
                if line.trim().is_empty() {
                    break;
                }
                if let Some(n) = line.to_lowercase().strip_prefix("content-length:") {
                    length = n.trim().parse().unwrap();
                }
            }
            let mut bytes = vec![0; length];
            request.read_exact(&mut bytes).unwrap();
            let call: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(call["model"], "local-test");
            assert!(call.get("tools").is_none());
            let body=json!({"choices":[{"message":{"content":"The sample covers four minute changes."}}]}).to_string();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        });
        let text = completion(port, "local-test", json!({"one_minute_scenarios":4}), None)
            .await
            .unwrap();
        worker.join().unwrap();
        assert_eq!(text, "The sample covers four minute changes.");
    }
    #[test]
    fn only_aggregate_coverage_reaches_the_gateway() {
        let r = json!({"positions":[{"pair":"PRIVATE","quantity":"999"}],"risk":{"model":"joint_minute_spot_v1","sample_size":59,"confidence_bps":9500,"stress_pnl":"-1234","nav":"789"}});
        assert_eq!(
            context(&r),
            json!({"model":"joint_minute_spot_v1","one_minute_scenarios":59,"confidence_bps":9500,"position_count":1,"stress_direction":"loss"})
        );
    }
}
