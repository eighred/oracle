use super::*;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
};

const FIRST: u64 = 1_767_225_600_000;
const TICK: &str = "0.000000000001";
fn okx_meta() -> Json {
    json!({"code":"0","data":[{"instType":"SPOT","state":"live","instId":"TEST-USDC","baseCcy":"TEST","quoteCcy":"USDC","tickSz":TICK,"lotSz":"1"}]})
}
fn row(t: u64) -> Json {
    json!([t.to_string(), TICK, TICK, TICK, TICK, "0", "0", "0", "1"])
}

#[tokio::test]
async fn ccxt_okx_discovers_nondefault_pair_and_reverses_confirmed_rows() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let data = json!({"code":"0","data":[row(FIRST+MINUTE),row(FIRST)]});
    let server = async {
        for (path, mut expected, body) in [
            (
                "/api/v5/public/instruments",
                vec!["instType=SPOT", "instId=TEST-USDC"],
                okx_meta(),
            ),
            (
                "/api/v5/market/history-candles",
                vec![
                    "instId=TEST-USDC",
                    "bar=1m",
                    "after=1767225660001",
                    "limit=2",
                ],
                data,
            ),
        ] {
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
            let (actual, query) = target.split_once('?').unwrap();
            assert_eq!(actual, path);
            let mut fields = query.split('&').collect::<Vec<_>>();
            fields.sort();
            expected.sort();
            assert_eq!(fields, expected);
            loop {
                line.clear();
                assert!(stream.read_line(&mut line).await.unwrap() > 0);
                if line == "\r\n" {
                    break;
                }
                assert!(!line.to_lowercase().starts_with("ok-access-"));
            }
            let body = body.to_string();
            let reply = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            );
            stream.get_mut().write_all(reply.as_bytes()).await.unwrap();
        }
    };
    let (_, result) = tokio::time::timeout(Duration::from_secs(8), async {
        tokio::join!(
            server,
            read(Venue::Okx, "TEST/USDC", FIRST + MINUTE, 2, &endpoint)
        )
    })
    .await
    .unwrap();
    let h = result.unwrap();
    assert_eq!(h.closes, [TICK, TICK]);
    assert_eq!(h.tick, TICK);
    assert_eq!(h.first_open_ms, FIRST);
    assert_eq!(h.market_id, "TEST-USDC");
}

#[test]
fn suspended_swapped_incomplete_and_duplicate_inputs_are_unavailable() {
    let mut meta = okx_meta();
    meta["data"][0]["state"] = json!("suspend");
    assert!(rules(Venue::Okx, &meta, "TEST", "USDC", "TEST-USDC").is_err());
    assert!(rules(Venue::Okx, &okx_meta(), "USDC", "TEST", "TEST-USDC").is_err());
    let mut data = json!({"code":"0","data":[row(FIRST+MINUTE),row(FIRST)]});
    data["data"][0][8] = json!("0");
    assert!(closes(Venue::Okx, &data, FIRST, 2).is_err());
    data["data"][0] = row(FIRST);
    assert!(closes(Venue::Okx, &data, FIRST, 2).is_err());
    assert!(closes(Venue::Binance, &json!([]), FIRST, 1).is_err());
    let m = json!({"symbols":[{"symbol":"測試USDC","baseAsset":"測試","quoteAsset":"USDC","status":"TRADING","isSpotTradingAllowed":true,
          "filters":[{"filterType":"LOT_SIZE","stepSize":"1"},{"filterType":"PRICE_FILTER","tickSize":TICK}]}]});
    assert_eq!(
        rules(Venue::Binance, &m, "測試", "USDC", "測試USDC").unwrap(),
        (TICK, "1")
    );
}
