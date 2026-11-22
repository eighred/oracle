//! Joint one-minute historical shocks on today's long spot exposures, in one quote currency.
use super::tails;
use num_rational::BigRational as Number;
use oracle_spot_amount::spot_decimal;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SizeUnit {
    #[default]
    Base,
    Quote,
}

pub struct Leg<'a> {
    pub quantity: &'a str,
    pub closes: &'a [String],
    pub shock_bps: i32,
}

#[derive(Serialize)]
pub struct Summary {
    model: &'static str,
    cash: String,
    nav: String,
    sample_size: usize,
    horizon_ms: u64,
    confidence_bps: u16,
    value_at_risk: String,
    expected_shortfall: String,
    stress_pnl: String,
    stressed_nav: String,
}

pub struct Contribution {
    pub exposure: String,
    pub stress_pnl: String,
}

/// The source adapter verifies that each series covers exactly the same requested UTC grid.
pub fn evaluate(
    cash: &str,
    legs: &[Leg<'_>],
    confidence: u16,
) -> Result<(Summary, Vec<Contribution>), &'static str> {
    evaluate_sized(cash, legs, confidence, SizeUnit::Base)
}

pub fn evaluate_sized(
    cash: &str,
    legs: &[Leg<'_>],
    confidence: u16,
    unit: SizeUnit,
) -> Result<(Summary, Vec<Contribution>), &'static str> {
    let cash = spot_decimal(cash).ok_or("invalid cash")?;
    let count = legs.first().ok_or("missing positions")?.closes.len();
    if !(5..=100).contains(&count) || !(1..10_000).contains(&confidence) {
        return Err("invalid risk window or confidence");
    }
    let zero = Number::from_integer(0.into());
    let mut nav = cash.clone();
    let mut stress_pnl = zero.clone();
    let mut losses = vec![zero; count - 1];
    let mut contributions = Vec::new();
    for leg in legs {
        if leg.closes.len() != count || !(-10_000..=100_000).contains(&leg.shock_bps) {
            return Err("invalid panel or shock");
        }
        let prices = leg
            .closes
            .iter()
            .map(|s| {
                spot_decimal(s)
                    .filter(|n| n.numer() > &0.into())
                    .ok_or("invalid mark")
            })
            .collect::<Result<Vec<_>, _>>()?;
        let quantity = spot_decimal(leg.quantity).ok_or("invalid quantity")?;
        let exposure = match unit {
            SizeUnit::Base => quantity * &prices[count - 1],
            SizeUnit::Quote => quantity,
        };
        for (loss, pair) in losses.iter_mut().zip(prices.windows(2)) {
            *loss += &exposure * (&pair[0] - &pair[1]) / &pair[0];
        }
        let pnl = &exposure * Number::new(leg.shock_bps.into(), 10_000.into());
        nav += &exposure;
        stress_pnl += &pnl;
        contributions.push(Contribution {
            exposure: exposure.to_string(),
            stress_pnl: pnl.to_string(),
        });
    }
    let (var, es) = tails(&mut losses, confidence);
    Ok((
        Summary {
            model: "joint_minute_spot_v1",
            cash: cash.to_string(),
            nav: nav.to_string(),
            sample_size: count - 1,
            horizon_ms: 60_000,
            confidence_bps: confidence,
            value_at_risk: var.to_string(),
            expected_shortfall: es.to_string(),
            stress_pnl: stress_pnl.to_string(),
            stressed_nav: (nav + stress_pnl).to_string(),
        },
        contributions,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quote_allocations_reprice_the_same_hundred_unit_reference_basket() {
        let marks = ["10", "11", "9", "12", "20"].map(String::from);
        let quote = [Leg {
            quantity: "100",
            closes: &marks,
            shock_bps: -1500,
        }];
        let base = [Leg {
            quantity: "5",
            closes: &marks,
            shock_bps: -1500,
        }];
        let (a, _) = evaluate_sized("0", &quote, 6250, SizeUnit::Quote).unwrap();
        let (b, _) = evaluate("0", &base, 6250).unwrap();
        assert_eq!((a.nav.as_str(), a.stress_pnl.as_str()), ("100", "-15"));
        assert_eq!(a.value_at_risk, b.value_at_risk);
        assert_eq!(a.expected_shortfall, b.expected_shortfall);
    }
    #[test]
    fn jointly_repriced_scenarios_and_opposing_stress_contributions() {
        let a = ["100", "90", "100", "90", "100"].map(String::from);
        let b = ["90", "100", "90", "100", "90"].map(String::from);
        let legs = [
            Leg {
                quantity: "1",
                closes: &a,
                shock_bps: -2000,
            },
            Leg {
                quantity: "1",
                closes: &b,
                shock_bps: 1000,
            },
        ];
        // Full repricing gives 90 + 100 = 190 and 1000/9 + 81 = 1729/9.
        // The two equally frequent losses are zero and -19/9.
        let (r, contributions) = evaluate("10", &legs, 7500).unwrap();
        assert_eq!(r.nav, "200");
        assert_eq!(r.expected_shortfall, "0");
        assert_eq!(r.value_at_risk, "0");
        assert_eq!(r.stress_pnl, "-11");
        assert_eq!(r.stressed_nav, "189");
        assert_eq!(contributions[1].stress_pnl, "9");
    }

    #[test]
    fn joint_tail_uses_partial_probability_mass_at_the_boundary() {
        let a = ["20", "20", "18", "18", "9"].map(String::from);
        let b = ["10", "20", "10", "10", "20"].map(String::from);
        let legs = [
            Leg {
                quantity: "2",
                closes: &a,
                shock_bps: 0,
            },
            Leg {
                quantity: "0.5",
                closes: &b,
                shock_bps: 0,
            },
        ];
        // Reprice ending holdings (18 and 10) on each date pair.
        // Losses in time order: -10, 6.8, 0, -1. The worst 1.5 carry mean 68/15.
        let (r, _) = evaluate("0", &legs, 6250).unwrap();
        assert_eq!(r.value_at_risk, "0");
        assert_eq!(r.expected_shortfall, "68/15");
        assert_eq!(r.stressed_nav, "28");
    }
}
