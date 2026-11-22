use oracle_backtest::paper::{CADENCE_MS, Sample, plan};
use oracle_market_data::{Observation, fixture_minute, public_minute, verify_receipt};
use oracle_spot_amount::Amount;
use serde_json::{Value, json};
use tokio_postgres::Transaction;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Source {
    Generated,
    Fixture,
    Binance,
}

impl Source {
    pub fn from_kind(kind: &str) -> Result<Self, &'static str> {
        match kind {
            "prospective_synthetic" => Ok(Self::Generated),
            "ccxt_loopback_fixture" => Ok(Self::Fixture),
            "ccxt_binance_spot" => Ok(Self::Binance),
            _ => Err("unsupported paper source"),
        }
    }

    pub async fn manifest(
        self,
        tx: &Transaction<'_>,
        first: i64,
        stored_scope: Option<i64>,
    ) -> Result<Value, &'static str> {
        let mut manifest = plan(first as u64)?;
        if self == Self::Generated {
            return Ok(manifest);
        }
        let fixture = self == Self::Fixture;
        manifest["source_kind"] = json!(if fixture {
            "ccxt_loopback_fixture"
        } else {
            "ccxt_binance_spot"
        });
        manifest["source"] = json!(if fixture {
            "Eighred loopback kline fixture via CCXT Rust"
        } else {
            "Binance public BTCUSDT klines via CCXT Rust"
        });
        manifest["instrument"] = json!(if fixture {
            "BTC-USDT-ccxt-fixture-v1"
        } else {
            "binance-BTCUSDT-spot"
        });
        manifest["market_id"] = json!("BTCUSDT");
        manifest["method"] = json!("public_get_klines");
        manifest["adapter_version"] = json!("4.5.93");
        if !fixture {
            let row = tx
                .query_opt(
                    "SELECT scope_id,evidence_ref FROM oracle_paper.binance_scopes
                WHERE ($1::bigint IS NULL OR scope_id=$1) AND NOT revoked AND allowed_until > clock_timestamp() ORDER BY scope_id DESC LIMIT 1",
                    &[&stored_scope],
                )
                .await
                .map_err(|_| "source permission lookup failed")?
                .ok_or("missing active BTCUSDT internal data scope")?;
            manifest["source_scope_id"] = json!(row.get::<_, i64>(0));
            manifest["source_scope_sha256"] =
                json!(crate::digest(row.get::<_, &str>(1).as_bytes()));
        }
        Ok(manifest)
    }

    pub async fn observe(self, close_ms: u64) -> Result<Observation, &'static str> {
        let open_ms = close_ms
            .checked_sub(CADENCE_MS)
            .ok_or("invalid kline window")?;
        match self {
            Self::Binance => public_minute(open_ms).await,
            Self::Fixture => {
                let port = std::env::var("ORACLE_FIXTURE_PORT")
                    .ok()
                    .and_then(|p| p.parse::<u16>().ok())
                    .ok_or("configure the Eighred loopback fixture port")?;
                fixture_minute(port, open_ms).await
            }
            Self::Generated => Err("generated marks have no venue receipt"),
        }
    }

    pub fn replay(
        self,
        expected: u64,
        sample: Sample,
        event: &Value,
    ) -> Result<Option<Value>, &'static str> {
        if self == Self::Generated {
            if !(Amount::parse("23").unwrap()..=Amount::parse("27").unwrap()).contains(&sample.mark)
                || event.get("venue_receipt").is_some()
            {
                return Err("unexpected generated sample");
            }
            return Ok(None);
        }
        let open = expected
            .checked_sub(CADENCE_MS)
            .ok_or("invalid kline window")?;
        let receipt = verify_receipt(&event["venue_receipt"], open, self == Self::Fixture)?;
        if receipt.close != sample.mark
            || receipt.received_ms != sample.known_ms
            || receipt.close_ms_inclusive.checked_add(1) != Some(sample.close_ms)
        {
            return Err("paper mark differs from its CCXT receipt");
        }
        Ok(Some(
            json!({"open_ms":open,"close_ms_inclusive":receipt.close_ms_inclusive,
            "received_ms":receipt.received_ms,"response_json_sha256":receipt.response_json_sha256}),
        ))
    }
}
