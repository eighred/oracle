//! Exact, equal-weight one-day historical simulation for a frozen spot snapshot.
use super::{Outcome, Snapshot};
use num_rational::BigRational;
use oracle_spot_amount::Amount;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub mod basket;

const HISTORY: &[u8] = include_bytes!("../fixtures/daily_risk_index.json");
const DAY: u64 = 86_400_000;
const BASIS_POINTS: usize = 10_000;
const MIN_SCENARIOS: usize = 4;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IndexHistory {
    instrument_revision: String,
    quote: String,
    closes: Vec<Close>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Close {
    close_ms: u64,
    known_ms: u64,
    index: Amount,
}

#[derive(Serialize)]
struct Scenario {
    from_ms: u64,
    to_ms: u64,
    from_known_ms: u64,
    to_known_ms: u64,
    loss: String,
}

#[derive(Serialize)]
pub struct RiskReport<'a> {
    snapshot: &'a Snapshot,
    model: &'static str,
    sign_convention: &'static str,
    amount_encoding: &'static str,
    horizon_ms: u64,
    confidence_bps: u16,
    history_sha256: String,
    history_basis: &'static str,
    #[serde(flatten)]
    risk: Coverage,
}

#[derive(Serialize)]
#[serde(tag = "risk_coverage", rename_all = "snake_case")]
enum Coverage {
    Complete {
        sample_size: usize,
        scenarios: Vec<Scenario>,
        value_at_risk: String,
        expected_shortfall: String,
    },
    Incomplete {
        reason: &'static str,
    },
}

fn tails(losses: &mut [BigRational], confidence: u16) -> (BigRational, BigRational) {
    losses.sort_unstable();
    // Each observed loss owns 10,000 units of probability mass.
    let cutoff = losses.len() * usize::from(confidence);
    let quantile = losses[cutoff.div_ceil(BASIS_POINTS) - 1].clone();
    let mut weighted = BigRational::from_integer(0.into());
    for (i, loss) in losses.iter().enumerate().rev() {
        let weight = ((i + 1) * BASIS_POINTS)
            .saturating_sub(cutoff)
            .min(BASIS_POINTS);
        if weight == 0 {
            break;
        }
        weighted += loss * BigRational::from_integer(weight.into());
    }
    let tail_mass = losses.len() * BASIS_POINTS - cutoff;
    (
        quantile,
        weighted / BigRational::from_integer(tail_mass.into()),
    )
}

fn evaluate(snapshot: &Snapshot, history: &IndexHistory, confidence: u16) -> Coverage {
    let Outcome::Complete { quantity, mark, .. } = snapshot.result else {
        return Coverage::Incomplete {
            reason: "incomplete_portfolio_snapshot",
        };
    };
    if history.instrument_revision != snapshot.instrument_revision
        || history.quote != snapshot.quote
    {
        return Coverage::Incomplete {
            reason: "missing_matching_instrument_or_quote",
        };
    }
    let visible: Vec<_> = history
        .closes
        .iter()
        .filter(|c| c.close_ms <= snapshot.valid_at_ms && c.known_ms <= snapshot.known_at_ms)
        .collect();
    if visible.len() <= MIN_SCENARIOS {
        return Coverage::Incomplete {
            reason: "insufficient_daily_closes",
        };
    }
    if visible.iter().any(|c| {
        c.known_ms < c.close_ms || c.index == Amount::ZERO || !c.close_ms.is_multiple_of(DAY)
    }) {
        return Coverage::Incomplete {
            reason: "invalid_daily_close",
        };
    }
    let units = |x: Amount| BigRational::new(x.atoms().into(), Amount::ATOMS_PER_UNIT.into());
    let exposure = units(quantity) * units(mark);
    let mut losses = Vec::new();
    let mut scenarios = Vec::new();
    for pair in visible.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if b.close_ms.checked_sub(a.close_ms) != Some(DAY) {
            return Coverage::Incomplete {
                reason: "nonconsecutive_daily_closes",
            };
        }
        let loss = &exposure * (units(a.index) - units(b.index)) / units(a.index);
        scenarios.push(Scenario {
            from_ms: a.close_ms,
            to_ms: b.close_ms,
            from_known_ms: a.known_ms,
            to_known_ms: b.known_ms,
            loss: loss.to_string(),
        });
        losses.push(loss);
    }
    let (var, es) = tails(&mut losses, confidence);
    Coverage::Complete {
        sample_size: losses.len(),
        scenarios,
        value_at_risk: var.to_string(),
        expected_shortfall: es.to_string(),
    }
}

/// One-minute historical revaluation of the ending long spot inventory.
/// The paper journal verifies the cadence and knowledge times before calling.
pub fn minute_tail(
    marks: &[Amount],
    quantity: Amount,
    confidence: u16,
) -> Option<(String, String)> {
    if marks.len() <= MIN_SCENARIOS
        || !(1..10_000).contains(&confidence)
        || marks.contains(&Amount::ZERO)
    {
        return None;
    }
    let exposure = BigRational::new(
        (u128::from(quantity.atoms()) * u128::from(marks.last()?.atoms())).into(),
        (u128::from(Amount::ATOMS_PER_UNIT).pow(2)).into(),
    );
    let mut losses = marks
        .windows(2)
        .map(|pair| {
            let previous = BigRational::from_integer(pair[0].atoms().into());
            &exposure * (&previous - BigRational::from_integer(pair[1].atoms().into())) / previous
        })
        .collect::<Vec<_>>();
    let (var, es) = tails(&mut losses, confidence);
    Some((var.to_string(), es.to_string()))
}

pub fn report(snapshot: &Snapshot, confidence_bps: u16) -> Result<RiskReport<'_>, String> {
    if !(1..10_000).contains(&confidence_bps) {
        return Err("confidence must be 1 through 9999 basis points".into());
    }
    let history = serde_json::from_slice(HISTORY).map_err(|e| e.to_string())?;
    Ok(RiskReport {
        snapshot,
        model: "equal_weight_daily_spot_v1",
        sign_convention: "loss = negative portfolio P&L",
        amount_encoding: "exact rational in snapshot quote currency",
        horizon_ms: DAY,
        confidence_bps,
        history_sha256: format!("{:x}", Sha256::digest(HISTORY)),
        history_basis: "Eighred synthetic normalized index; four scenarios for method verification",
        risk: evaluate(snapshot, &history, confidence_bps),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FIXTURE, replay};

    #[test]
    fn minute_shocks_reprice_the_ending_inventory_with_fractional_tail_mass() {
        let marks = ["20", "25", "30", "24", "24"].map(|s| Amount::parse(s).unwrap());
        let inventory = Amount::parse("2").unwrap();
        assert_eq!(
            minute_tail(&marks, inventory, 6250),
            Some(("0".into(), "32/5".into()))
        );
        assert_eq!(
            minute_tail(&marks, Amount::ZERO, 6250),
            Some(("0".into(), "0".into()))
        );
        assert!(minute_tail(&marks[..4], inventory, 6250).is_none());
    }

    #[test]
    fn ties_and_gains_keep_their_probability_and_sign() {
        let mut losses = [-4, -2, -2, -1].map(|n| BigRational::from_integer(n.into()));
        let (var, es) = tails(&mut losses, 5000);
        assert_eq!(var.to_string(), "-2");
        assert_eq!(es.to_string(), "-3/2");
        assert_eq!(tails(&mut losses, 9999).1.to_string(), "-1");
    }

    #[test]
    fn invalid_coverage_cannot_create_a_tail_estimate() {
        let snapshot = replay(FIXTURE, 1767225660000, 1767225660100).unwrap();
        let mut history: IndexHistory = serde_json::from_slice(HISTORY).unwrap();
        history.quote = "USD".into();
        assert!(matches!(
            evaluate(&snapshot, &history, 6250),
            Coverage::Incomplete {
                reason: "missing_matching_instrument_or_quote"
            }
        ));
        history.quote = "USDT".into();
        history.closes[2].known_ms = snapshot.known_at_ms + 1;
        assert!(matches!(
            evaluate(&snapshot, &history, 6250),
            Coverage::Incomplete {
                reason: "insufficient_daily_closes"
            }
        ));
        history.closes[2].known_ms = snapshot.known_at_ms;
        history.closes[2].close_ms = history.closes[1].close_ms;
        assert!(matches!(
            evaluate(&snapshot, &history, 6250),
            Coverage::Incomplete {
                reason: "nonconsecutive_daily_closes"
            }
        ));
        assert!(report(&snapshot, 0).is_err());
        assert!(report(&snapshot, 10000).is_err());
    }
}
