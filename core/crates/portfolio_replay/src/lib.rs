//! Replay a reviewed point-in-time dataset for the first spot portfolio.

pub mod risk;
use oracle_spot_amount::Amount;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub const FIXTURE: &[u8] = include_bytes!("../fixtures/own_spot.json");
const FIXTURE_DIGEST: &str = "8fcb4463744ea12aa4d648ed91f4c3ba96ee227479edc971f4265640730361d3";
const MINUTE: u64 = 60_000;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Portfolio {
    id: String,
    effective_at_ms: u64,
    known_at_ms: u64,
    cash: Amount,
    quantity: Amount,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Bar {
    id: String,
    open_ms: u64,
    received_ms: u64,
    open: Amount,
    high: Amount,
    low: Amount,
    close: Amount,
    volume: Amount,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Dataset {
    schema_version: u8,
    dataset_id: String,
    venue: String,
    instrument_revision: String,
    base: String,
    quote: String,
    interval_ms: u64,
    price_tick: Amount,
    quantity_step: Amount,
    portfolio: Portfolio,
    bars: Vec<Bar>,
}

#[derive(Serialize)]
pub struct Snapshot {
    schema_version: u8,
    dataset_id: String,
    dataset_sha256: String,
    rights_basis: &'static str,
    venue: String,
    instrument_revision: String,
    portfolio_id: String,
    base: String,
    quote: String,
    valid_at_ms: u64,
    known_at_ms: u64,
    #[serde(flatten)]
    result: Outcome,
}

#[derive(Serialize)]
#[serde(tag = "completeness", rename_all = "snake_case")]
enum Outcome {
    Complete {
        cash: Amount,
        quantity: Amount,
        mark: Amount,
        base_volume: Amount,
        observation_id: String,
        open_ms: u64,
        received_ms: u64,
        inventory_value: String,
        nav: String,
    },
    Incomplete {
        reason: &'static str,
        nav: Option<String>,
    },
}

impl Snapshot {
    /// Opening quote cash, base quantity and unit mark for fully covered snapshots.
    pub fn spot_balances(&self) -> Option<(Amount, Amount, Amount)> {
        match self.result {
            Outcome::Complete {
                cash,
                quantity,
                mark,
                ..
            } => Some((cash, quantity, mark)),
            Outcome::Incomplete { .. } => None,
        }
    }
}

impl Dataset {
    fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1
            || self.interval_ms != MINUTE
            || self.dataset_id.is_empty()
            || self.venue.is_empty()
            || self.instrument_revision.is_empty()
            || self.base.is_empty()
            || self.quote.is_empty()
            || self.base == self.quote
            || self.portfolio.id.is_empty()
            || self.price_tick == Amount::ZERO
            || !self.portfolio.quantity.aligns(self.quantity_step)
            || self.portfolio.effective_at_ms > self.portfolio.known_at_ms
        {
            return Err("invalid dataset, instrument rules or opening holdings".into());
        }
        let mut ids = HashSet::new();
        let mut revisions = HashSet::new();
        for bar in &self.bars {
            if bar.id.is_empty()
                || !ids.insert(&bar.id)
                || !revisions.insert((bar.open_ms, bar.received_ms))
            {
                return Err("ambiguous observation identifier or revision".into());
            }
            let closed = bar.open_ms.checked_add(MINUTE).ok_or("invalid bar end")?;
            if !bar.open_ms.is_multiple_of(MINUTE) || bar.received_ms < closed {
                return Err(format!("{} is not a closed UTC minute at receipt", bar.id));
            }
            if bar.low == Amount::ZERO
                || bar.low > bar.open.min(bar.close)
                || bar.high < bar.open.max(bar.close)
                || [bar.low, bar.open, bar.high, bar.close]
                    .iter()
                    .any(|p| !p.aligns(self.price_tick))
            {
                return Err(format!("{} violates price range or tick size", bar.id));
            }
        }
        Ok(())
    }

    fn snapshot(self, digest: String, valid: u64, known: u64) -> Result<Snapshot, String> {
        if known < valid {
            return Err("a retrospective snapshot requires known_at_ms >= valid_at_ms".into());
        }
        let slot = (valid / MINUTE * MINUTE).checked_sub(MINUTE);
        let selected = self
            .bars
            .iter()
            .filter(|bar| Some(bar.open_ms) == slot && bar.received_ms <= known)
            .max_by_key(|bar| bar.received_ms);
        let p = self.portfolio;
        let result = if p.effective_at_ms > valid || p.known_at_ms > known {
            Outcome::Incomplete {
                reason: "holdings_unavailable_at_cutoff",
                nav: None,
            }
        } else if let Some(bar) = selected {
            let (inventory_value, nav) = p.quantity.valuation(bar.close, p.cash)?;
            Outcome::Complete {
                cash: p.cash,
                quantity: p.quantity,
                mark: bar.close,
                base_volume: bar.volume,
                observation_id: bar.id.clone(),
                open_ms: bar.open_ms,
                received_ms: bar.received_ms,
                inventory_value,
                nav,
            }
        } else {
            Outcome::Incomplete {
                reason: "closed_bar_unavailable_at_cutoff",
                nav: None,
            }
        };
        Ok(Snapshot {
            schema_version: self.schema_version,
            dataset_id: self.dataset_id,
            dataset_sha256: digest,
            rights_basis: "Eighred-authored synthetic fixture",
            venue: self.venue,
            instrument_revision: self.instrument_revision,
            portfolio_id: p.id,
            base: self.base,
            quote: self.quote,
            valid_at_ms: valid,
            known_at_ms: known,
            result,
        })
    }
}

/// Admit the bundled source by digest. An input file cannot grant itself usage rights.
pub fn replay(data: &[u8], valid: u64, known: u64) -> Result<Snapshot, String> {
    let digest = format!("{:x}", Sha256::digest(data));
    if digest != FIXTURE_DIGEST {
        return Err("dataset digest is outside the approved fixture inventory".into());
    }
    let dataset: Dataset = serde_json::from_slice(data).map_err(|e| e.to_string())?;
    dataset.validate()?;
    dataset.snapshot(digest, valid, known)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Dataset {
        serde_json::from_slice(FIXTURE).unwrap()
    }

    #[test]
    fn rejects_ambiguous_or_invalid_market_events() {
        let mut d = fixture();
        d.bars[2].received_ms = d.bars[0].received_ms;
        assert!(d.validate().is_err());
        let mut d = fixture();
        d.bars[1].id = d.bars[0].id.clone();
        assert!(d.validate().is_err());
        let mut d = fixture();
        d.bars[0].received_ms = d.bars[0].open_ms;
        assert!(d.validate().is_err());
        let mut d = fixture();
        d.bars[0].high = Amount::parse("0.00000001").unwrap();
        assert!(d.validate().is_err());
    }
}
