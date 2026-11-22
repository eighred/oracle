use super::*;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
};

const START: u64 = 1_767_225_600_000;
const BODY: &str = r#"[[1767225600000,"100000000000.00000001","100000000000.00000002","99999999999.99999999","100000000000.00000002","0.00000001",1767225659999,"1000",10,"0","0","0"]]"#;

async fn exercise(status: &str, body: &str) -> Result<Observation, &'static str> {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/api/v3", listener.local_addr().unwrap());
    let server = async {
        let (stream, _) = listener.accept().await.unwrap();
        let mut stream = BufReader::new(stream);
        let mut line = String::new();
        stream.read_line(&mut line).await.unwrap();
        let target = line
            .trim_end()
            .strip_prefix("GET ")
            .unwrap()
            .strip_suffix(" HTTP/1.1")
            .unwrap();
        let (path, query) = target.split_once('?').unwrap();
        assert_eq!(path, "/api/v3/klines");
        let mut fields: Vec<_> = query.split('&').collect();
        fields.sort_unstable();
        assert_eq!(
            fields,
            [
                "endTime=1767225659999",
                "interval=1m",
                "limit=1",
                "startTime=1767225600000",
                "symbol=BTCUSDT"
            ]
        );
        loop {
            line.clear();
            assert!(stream.read_line(&mut line).await.unwrap() > 0);
            if line == "\r\n" {
                break;
            }
            let header = line.to_ascii_lowercase();
            assert!(!header.starts_with("authorization:") && !header.starts_with("x-mbx-apikey:"));
        }
        let reply = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.get_mut().write_all(reply.as_bytes()).await.unwrap();
    };
    let ((), result) = tokio::time::timeout(Duration::from_secs(8), async {
        tokio::join!(server, read_minute(&endpoint, START))
    })
    .await
    .expect("CCXT must reach the loopback venue");
    result
}

#[tokio::test]
async fn ccxt_reads_one_closed_public_minute_and_preserves_decimal_atoms() {
    let observation = json!(exercise("200 OK", BODY).await.unwrap());
    assert_eq!(observation["open"], "100000000000.00000001");
    assert_eq!(observation["close"], "100000000000.00000002");
    assert_eq!(observation["base_volume"], "0.00000001");
    assert_eq!(
        observation["response_json_sha256"],
        format!("{:x}", Sha256::digest(BODY))
    );
    assert_eq!(observation["market_id"], "BTCUSDT");
    assert_eq!(
        observation["source_response"],
        serde_json::from_str::<Json>(BODY).unwrap()
    );
    assert!(observation["received_ms"].as_u64().unwrap() > START + MINUTE);
    assert_eq!(observation["rights_status"], "unreviewed");
}

#[tokio::test]
async fn throttling_or_no_data_leaves_the_minute_missing() {
    assert_eq!(
        exercise("429 Too Many Requests", "{}").await.err(),
        Some("venue throttled the read")
    );
    assert_eq!(
        exercise("200 OK", "[]").await.err(),
        Some("missing requested minute")
    );
}

#[test]
fn conflicting_rows_wrong_intervals_and_numeric_prices_are_refused() {
    let original: Json = serde_json::from_str(BODY).unwrap();
    for (field, value) in [
        (0, json!(START + 1)),
        (1, json!(100)),
        (6, json!(START + MINUTE)),
        (3, json!("0")),
    ] {
        let mut modified = original.clone();
        modified[0][field] = value;
        assert!(normalize(modified, START, START + MINUTE).is_err());
    }
    assert!(normalize(json!([original[0], original[0]]), START, START + MINUTE).is_err());
    assert!(normalize(original, START, START + MINUTE - 1).is_err());
}
