//! Evaluate each observed prefix with the same decisions and cash ledger as historical replay.
use super::{COST, Close, MINUTE, Rule, WARMUP, simulate};
use oracle_spot_amount::Amount;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub const SAMPLES: usize = 9;
pub const CADENCE_MS: u64 = MINUTE;
pub const GRACE_MS: u64 = 15_000;

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Sample {
    pub close_ms: u64,
    pub known_ms: u64,
    pub mark: Amount,
}

pub fn plan(first_ms: u64) -> Result<Value, &'static str> {
    if !first_ms.is_multiple_of(MINUTE) {
        return Err("first sample must fall on a UTC minute");
    }
    let last = first_ms
        .checked_add((SAMPLES as u64 - 1) * MINUTE + GRACE_MS)
        .ok_or("window exceeds supported time")?;
    let mut engine = Sha256::new();
    for source in [
        include_str!("lib.rs"),
        include_str!("ledger.rs"),
        include_str!("paper.rs"),
    ] {
        engine.update(source.replace("\r\n", "\n"));
    }
    Ok(json!({
        "engine_sha256":format!("{:x}",engine.finalize()),
        "version":1,"portfolio":"own-paper-01","source_kind":"prospective_synthetic",
        "source":"Independent uniform cents from 23.00 to 27.00 USDT sampled at receipt",
        "instrument":"BTC-USDT-fixture-v1","base":"BTC","quote":"USDT",
        "first_ms":first_ms,"last_ms":last-GRACE_MS,"warmup_samples":WARMUP,"samples":SAMPLES,
        "cadence_ms":MINUTE,"receipt_grace_ms":GRACE_MS,"opening_cash":"50","opening_quantity":"2",
        "lot_btc":"0.25","benchmark":"hold","rules":["hold","dca","momentum"],"cost_model":COST,
        "decision_model":"close_to_close_spot_v1","funding":"0","external_flows":"0",
        "acceptance":"Record all nine samples on time, commit each next instruction before its settlement, reconcile every balance and reproduce each posting after restart. Retain losses.",
        "stop":"Cancel on operator request or stop incomplete on the first missed sample."
    }))
}

pub fn project(first_ms: u64, samples: &[Sample]) -> Result<Value, &'static str> {
    if samples.is_empty() || samples.len() > SAMPLES {
        return Err("unexpected paper sample count");
    }
    let mut tape = Vec::new();
    let tick = Amount::parse(COST.tick).expect("static tick");
    for (index, sample) in samples.iter().enumerate() {
        if first_ms.checked_add(index as u64 * MINUTE) != Some(sample.close_ms)
            || sample
                .known_ms
                .checked_sub(sample.close_ms)
                .is_none_or(|lag| lag >= GRACE_MS)
            || sample.mark == Amount::ZERO
            || !sample.mark.aligns(tick)
        {
            return Err("invalid paper sample, mark or receipt time");
        }
        tape.push(Close {
            close_ms: sample.close_ms,
            known_ms: sample.known_ms,
            mark: sample.mark,
        });
    }
    let mut next = serde_json::Map::new();
    let mut runs = Vec::new();
    if samples.len() >= WARMUP {
        let opening = (
            Amount::parse("50").unwrap(),
            Amount::parse("2").unwrap(),
            tape[WARMUP - 1].mark,
        );
        for rule in [Rule::Hold, Rule::Dca, Rule::Momentum] {
            if samples.len() < SAMPLES {
                let name = serde_json::to_value(rule).map_err(|_| "invalid rule")?;
                next.insert(
                    name.as_str().ok_or("invalid rule name")?.into(),
                    json!(rule.decision(&tape, samples.len() - WARMUP)),
                );
            }
            if samples.len() > WARMUP {
                runs.push(
                    simulate(rule, &tape, opening).map_err(|_| "paper reconciliation failed")?,
                );
            }
        }
    }
    Ok(json!({"next_instructions":next,"runs":runs}))
}
