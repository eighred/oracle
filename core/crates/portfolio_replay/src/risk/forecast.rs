//! Point-in-time replay of a daily forecast against the next comparable closing mark.
use super::{Coverage, DAY, RiskReport, report};
use crate::{Dataset, MINUTE, Outcome, Snapshot};
use num_rational::BigRational;
use oracle_spot_amount::Amount;
use serde::Serialize;
use sha2::{Digest, Sha256};

const OUTCOMES: &[u8] = include_bytes!("../../fixtures/next_day_spot.json");

/// The synthetic schedule freezes the forecast one second after its UTC minute closes.
pub fn freeze_at(valid_ms: u64) -> Result<u64, String> {
    if !valid_ms.is_multiple_of(MINUTE) {
        return Err("daily forecast requires a UTC minute close".into());
    }
    valid_ms
        .checked_add(1_000)
        .ok_or("invalid forecast instant".into())
}

#[derive(Serialize)]
pub struct DailyScore<'a> {
    schema_version: u8,
    forecast: RiskReport<'a>,
    outcome_valid_at_ms: u64,
    score_known_at_ms: u64,
    outcome_dataset_sha256: String,
    comparison: &'static str,
    scheduled_intervals: usize,
    scored_intervals: usize,
    uncertainty: &'static str,
    score: Score,
}

#[derive(Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum Score {
    Scored {
        observation_id: String,
        received_ms: u64,
        closing_mark: Amount,
        closing_nav: String,
        portfolio_pnl: String,
        loss: String,
        var_exceeded: bool,
    },
    Unavailable {
        reason: &'static str,
    },
}

fn compare(opening: &Snapshot, closing: Snapshot, var: &BigRational) -> Score {
    let Some((cash, quantity, mark)) = opening.spot_balances() else {
        return Score::Unavailable {
            reason: "incomplete_portfolio_snapshot",
        };
    };
    if opening.instrument_revision != closing.instrument_revision
        || opening.venue != closing.venue
        || opening.base != closing.base
        || opening.quote != closing.quote
        || opening.portfolio_id != closing.portfolio_id
    {
        return Score::Unavailable {
            reason: "incompatible_outcome_portfolio_or_instrument",
        };
    }
    let Outcome::Complete {
        cash: end_cash,
        quantity: end_quantity,
        mark: end_mark,
        observation_id,
        received_ms,
        nav,
        ..
    } = closing.result
    else {
        return Score::Unavailable {
            reason: "outcome_unavailable_at_cutoff",
        };
    };
    if cash != end_cash || quantity != end_quantity {
        return Score::Unavailable {
            reason: "outcome_holdings_changed",
        };
    }
    let pnl = BigRational::new(
        (i128::from(end_mark.atoms()) - i128::from(mark.atoms())).into(),
        Amount::ATOMS_PER_UNIT.into(),
    ) * BigRational::new(quantity.atoms().into(), Amount::ATOMS_PER_UNIT.into());
    let loss = -&pnl;
    Score::Scored {
        observation_id,
        received_ms,
        closing_mark: end_mark,
        closing_nav: nav,
        portfolio_pnl: pnl.to_string(),
        loss: loss.to_string(),
        var_exceeded: &loss > var,
    }
}

pub fn score(snapshot: &Snapshot, confidence: u16, known: u64) -> Result<DailyScore<'_>, String> {
    if snapshot.known_at_ms != freeze_at(snapshot.valid_at_ms)? || known < snapshot.known_at_ms {
        return Err("score requires a scheduled forecast visible at the scoring cutoff".into());
    }
    let end = snapshot
        .valid_at_ms
        .checked_add(DAY)
        .ok_or("invalid horizon")?;
    let forecast = report(snapshot, confidence)?;
    let digest = format!("{:x}", Sha256::digest(OUTCOMES));
    let score = match &forecast.risk {
        Coverage::Incomplete { reason } => Score::Unavailable { reason },
        Coverage::Complete { .. } if known < end => Score::Unavailable {
            reason: "horizon_not_elapsed",
        },
        Coverage::Complete { value_at_risk, .. } => {
            let data: Dataset = serde_json::from_slice(OUTCOMES).map_err(|e| e.to_string())?;
            data.validate()?;
            compare(
                snapshot,
                data.snapshot(digest.clone(), end, known)?,
                value_at_risk,
            )
        }
    };
    Ok(DailyScore {
        schema_version: 1,
        forecast,
        outcome_valid_at_ms: end,
        score_known_at_ms: known,
        outcome_dataset_sha256: digest,
        comparison: "Revalue the same frozen spot holdings after 24 hours with zero trades, fees, slippage, flows and funding. A loss strictly above VaR is an exceedance.",
        scheduled_intervals: 1,
        scored_intervals: usize::from(matches!(score, Score::Scored { .. })),
        uncertainty: "At most one synthetic outcome gives insufficient observations to estimate calibration or exception clustering.",
        score,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FIXTURE, replay};

    #[test]
    fn incomparable_outcomes_are_unscored() {
        let valid = 1767225660000;
        let opening = replay(FIXTURE, valid, freeze_at(valid).unwrap()).unwrap();
        let var = BigRational::from_integer(2.into());
        let mut data: Dataset = serde_json::from_slice(OUTCOMES).unwrap();
        data.quote = "USD".into();
        let closing = data
            .snapshot(String::new(), valid + DAY, valid + DAY + 300)
            .unwrap();
        assert!(matches!(
            compare(&opening, closing, &var),
            Score::Unavailable {
                reason: "incompatible_outcome_portfolio_or_instrument"
            }
        ));
        let mut data: Dataset = serde_json::from_slice(OUTCOMES).unwrap();
        data.portfolio.cash = Amount::ZERO;
        let closing = data
            .snapshot(String::new(), valid + DAY, valid + DAY + 300)
            .unwrap();
        assert!(matches!(
            compare(&opening, closing, &var),
            Score::Unavailable {
                reason: "outcome_holdings_changed"
            }
        ));
    }

    #[test]
    fn refuse_a_retroactive_freeze_or_premature_scoring_cutoff() {
        let valid = 1767225660000;
        let opening = replay(FIXTURE, valid, valid + 120_000).unwrap();
        assert!(score(&opening, 6250, valid + DAY).is_err());
        let opening = replay(FIXTURE, valid, freeze_at(valid).unwrap()).unwrap();
        assert!(score(&opening, 6250, valid).is_err());
        assert!(freeze_at(valid + 1).is_err());
    }
}
