use num_rational::BigRational;
use oracle_spot_amount::Amount;
use serde::Serialize;

pub(crate) type Number = BigRational;

pub(crate) fn ratio(n: i64, d: i64) -> Number {
    Number::new(n.into(), d.into())
}

pub(crate) fn units(amount: Amount) -> Number {
    Number::new(amount.atoms().into(), Amount::ATOMS_PER_UNIT.into())
}

/// Preserve exact fractions for recurring average-cost and drawdown values.
pub(crate) fn show(value: &Number) -> String {
    let scale = 10_i64.pow(16);
    let scaled = value * ratio(scale, 1);
    if !scaled.is_integer() {
        return value.to_string();
    }
    let atoms = scaled.to_integer().to_string();
    let digits = format!("{:0>17}", atoms.trim_start_matches('-'));
    let (whole, fraction) = digits.split_at(digits.len() - 16);
    let fraction = fraction.trim_end_matches('0');
    let sign = if atoms.starts_with('-') { "-" } else { "" };
    let whole = whole.trim_start_matches('0');
    let whole = if whole.is_empty() { "0" } else { whole };
    if fraction.is_empty() {
        format!("{sign}{whole}")
    } else {
        format!("{sign}{whole}.{fraction}")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Side {
    Buy,
    Sell,
}

#[derive(Serialize)]
pub(crate) struct CostModel {
    pub version: &'static str,
    pub taker_fee_bps: i64,
    pub half_spread_bps: i64,
    pub slippage_bps: i64,
    pub funding_bps: i64,
    pub tick: &'static str,
    pub quantity_step: &'static str,
    pub min_notional: &'static str,
}

pub(crate) const COST: CostModel = CostModel {
    version: "synthetic_cash_spot_v1",
    taker_fee_bps: 10,
    half_spread_bps: 2,
    slippage_bps: 3,
    funding_bps: 0,
    tick: "0.01",
    quantity_step: "0.00000001",
    min_notional: "1",
};

#[derive(Serialize)]
pub(crate) struct Fill {
    pub side: Side,
    pub quantity: String,
    pub price: String,
    pub notional: String,
    pub fee: String,
    pub half_spread_cost: String,
    pub slippage_cost: String,
    pub tick_rounding_cost: String,
    pub cash_delta: String,
    pub quantity_delta: String,
}

pub(crate) struct Ledger {
    pub cash: Number,
    pub quantity: Number,
    pub cost_basis: Number,
    pub realized: Number,
    pub fees: Number,
    pub traded_notional: Number,
}

impl Ledger {
    pub fn new(cash: Number, quantity: Number, opening_mark: &Number) -> Self {
        Self {
            cash,
            cost_basis: &quantity * opening_mark,
            quantity,
            realized: ratio(0, 1),
            fees: ratio(0, 1),
            traded_notional: ratio(0, 1),
        }
    }

    pub fn nav(&self, mark: &Number) -> Number {
        &self.cash + &self.quantity * mark
    }

    pub fn fill(
        &mut self,
        side: Side,
        qty: &Number,
        mid: &Number,
        costs: &CostModel,
    ) -> Result<Fill, &'static str> {
        let step = units(Amount::parse(costs.quantity_step).expect("static quantity step"));
        if qty <= &ratio(0, 1) || !(qty / step).is_integer() {
            return Err("invalid_quantity_step");
        }
        let tick = units(Amount::parse(costs.tick).expect("static tick"));
        if mid <= &ratio(0, 1) || !(mid / &tick).is_integer() {
            return Err("invalid_reference_mark");
        }
        let spread = mid * ratio(costs.half_spread_bps, 10_000);
        let slip = mid * ratio(costs.slippage_bps, 10_000);
        let before_rounding = match side {
            Side::Buy => mid + &spread + &slip,
            Side::Sell => mid - &spread - &slip,
        };
        let ticks = &before_rounding / &tick;
        let price = match side {
            Side::Buy => ticks.ceil(),
            Side::Sell => ticks.floor(),
        } * tick;
        let notional = qty * &price;
        if notional < units(Amount::parse(costs.min_notional).expect("static minimum")) {
            return Err("below_minimum_notional");
        }
        let quote_scale = ratio(100_000_000, 1);
        if !(&notional * &quote_scale).is_integer() {
            return Err("unsupported_quote_precision");
        }
        let fee =
            (&notional * ratio(costs.taker_fee_bps, 10_000) * &quote_scale).ceil() / quote_scale;
        let (cash_delta, qty_delta, rounding) = match side {
            Side::Buy => {
                let debit = &notional + &fee;
                if debit > self.cash {
                    return Err("insufficient_cash");
                }
                self.cost_basis += &debit;
                (-debit, qty.clone(), &price - before_rounding)
            }
            Side::Sell => {
                if qty > &self.quantity {
                    return Err("insufficient_quantity");
                }
                let removed_cost = &self.cost_basis * qty / &self.quantity;
                let credit = &notional - &fee;
                self.realized += &credit - &removed_cost;
                self.cost_basis -= removed_cost;
                (credit, -qty, before_rounding - &price)
            }
        };
        self.cash += &cash_delta;
        self.quantity += &qty_delta;
        self.fees += &fee;
        self.traded_notional += &notional;
        Ok(Fill {
            side,
            quantity: show(qty),
            price: show(&price),
            notional: show(&notional),
            fee: show(&fee),
            half_spread_cost: show(&(qty * spread)),
            slippage_cost: show(&(qty * slip)),
            tick_rounding_cost: show(&(qty * rounding)),
            cash_delta: show(&cash_delta),
            quantity_delta: show(&qty_delta),
        })
    }
}
