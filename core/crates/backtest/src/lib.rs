//! Replay a frozen minute-by-minute strategy and two baselines through one spot ledger.
mod ledger;

use ledger::{COST, CostModel, Fill, Ledger, Side, ratio, show, units};
use oracle_portfolio_replay::{FIXTURE, Snapshot, replay};
use oracle_spot_amount::Amount;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const TAPE: &[u8] = include_bytes!("../fixtures/spot_minutes.json");
const MINUTE: u64 = 60_000;
const WARMUP: usize = 3;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Close {
    close_ms: u64,
    known_ms: u64,
    mark: Amount,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum Rule {
    Hold,
    Dca,
    Momentum,
}

impl Rule {
    fn decision(self, past: &[Close], step: usize) -> Option<Side> {
        match self {
            Self::Hold => None,
            Self::Dca => step.is_multiple_of(2).then_some(Side::Buy),
            Self::Momentum => match past[past.len() - 1].mark.cmp(&past[past.len() - 2].mark) {
                std::cmp::Ordering::Greater => Some(Side::Buy),
                std::cmp::Ordering::Less => Some(Side::Sell),
                std::cmp::Ordering::Equal => None,
            },
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum Posting {
    Hold,
    Rejected { side: Side, reason: &'static str },
    Filled { fill: Box<Fill> },
}

#[derive(Serialize)]
struct Row {
    decision_ms: u64,
    signal_close_ms: u64,
    signal_mark: Amount,
    valuation_close_ms: u64,
    valuation_known_ms: u64,
    valuation_mark: Amount,
    posting: Posting,
    cash: String,
    quantity: String,
    nav: String,
    realized_pnl: String,
    unrealized_pnl: String,
    drawdown: String,
}

#[derive(Serialize)]
struct Run {
    rule: Rule,
    opening_nav: String,
    closing_nav: String,
    net_pnl: String,
    total_fees: String,
    traded_notional: String,
    turnover_over_opening_nav: String,
    max_drawdown: String,
    peak_ms: u64,
    trough_ms: u64,
    ledger: Vec<Row>,
}

#[derive(Serialize)]
pub struct Report {
    schema_version: u8,
    model: &'static str,
    seed: Option<u64>,
    basis: &'static str,
    lot_btc: &'static str,
    benchmark: Rule,
    cost_model: CostModel,
    warmup_first_ms: u64,
    warmup_last_ms: u64,
    holdout_first_ms: u64,
    holdout_last_ms: u64,
    source_sha256: String,
    source: Vec<Close>,
    opening_snapshot: Snapshot,
    runs: Vec<Run>,
    ledger_sha256: String,
}

fn validate(tape: &[Close]) -> Result<(), String> {
    if tape.len() <= WARMUP {
        return Err("insufficient holdout coverage".into());
    }
    let tick = Amount::parse(COST.tick).expect("static price tick");
    for (i, bar) in tape.iter().enumerate() {
        if bar.mark == Amount::ZERO
            || !bar.mark.aligns(tick)
            || !bar.close_ms.is_multiple_of(MINUTE)
            || bar.known_ms < bar.close_ms
        {
            return Err("invalid mark or source clock".into());
        }
        if i > 0 {
            let previous = &tape[i - 1];
            if bar.close_ms.checked_sub(previous.close_ms) != Some(MINUTE) {
                return Err("missing or duplicate market minute".into());
            }
            if previous.known_ms >= bar.close_ms {
                return Err("decision unavailable before modeled fill".into());
            }
        }
    }
    Ok(())
}

fn simulate(rule: Rule, tape: &[Close], opening: (Amount, Amount, Amount)) -> Result<Run, String> {
    let anchor = &tape[WARMUP - 1];
    let mut book = Ledger::new(units(opening.0), units(opening.1), &units(opening.2));
    let initial = book.nav(&units(anchor.mark));
    if initial <= ratio(0, 1) {
        return Err("positive opening NAV required".into());
    }
    let mut peak = initial.clone();
    let mut last_nav = initial.clone();
    let mut peak_ms = anchor.close_ms;
    let mut worst = ratio(0, 1);
    let mut worst_window = (peak_ms, peak_ms);
    let mut ledger = Vec::new();
    for i in WARMUP..tape.len() {
        let signal = &tape[i - 1];
        let bar = &tape[i];
        let mark = units(bar.mark);
        let posting = match rule.decision(&tape[..i], i - WARMUP) {
            None => Posting::Hold,
            Some(side) => match book.fill(side, &ratio(1, 4), &mark, &COST) {
                Ok(fill) => Posting::Filled {
                    fill: Box::new(fill),
                },
                Err(reason) => Posting::Rejected { side, reason },
            },
        };
        let unrealized = &book.quantity * &mark - &book.cost_basis;
        last_nav = book.nav(&mark);
        if &last_nav - &initial != &book.realized + &unrealized {
            return Err("unreconciled NAV".into());
        }
        if last_nav > peak {
            peak = last_nav.clone();
            peak_ms = bar.close_ms;
        }
        let dd = (&peak - &last_nav) / &peak;
        if dd > worst {
            worst = dd.clone();
            worst_window = (peak_ms, bar.close_ms);
        }
        ledger.push(Row {
            decision_ms: signal.known_ms,
            signal_close_ms: signal.close_ms,
            signal_mark: signal.mark,
            valuation_close_ms: bar.close_ms,
            valuation_known_ms: bar.known_ms,
            valuation_mark: bar.mark,
            posting,
            cash: show(&book.cash),
            quantity: show(&book.quantity),
            nav: show(&last_nav),
            realized_pnl: show(&book.realized),
            unrealized_pnl: show(&unrealized),
            drawdown: show(&dd),
        });
    }
    Ok(Run {
        rule,
        opening_nav: show(&initial),
        closing_nav: show(&last_nav),
        net_pnl: show(&(&last_nav - &initial)),
        total_fees: show(&book.fees),
        traded_notional: show(&book.traded_notional),
        turnover_over_opening_nav: show(&(&book.traded_notional / initial)),
        max_drawdown: show(&worst),
        peak_ms: worst_window.0,
        trough_ms: worst_window.1,
        ledger,
    })
}

pub fn report() -> Result<Report, String> {
    let tape: Vec<Close> = serde_json::from_slice(TAPE).map_err(|e| e.to_string())?;
    validate(&tape)?;
    let anchor = &tape[WARMUP - 1];
    let opening_snapshot = replay(FIXTURE, anchor.close_ms, anchor.known_ms)?;
    let opening = opening_snapshot
        .spot_balances()
        .ok_or("opening portfolio unavailable")?;
    if opening.2 != anchor.mark {
        return Err("opening mark disagrees with the holdout manifest".into());
    }
    let runs = [Rule::Hold, Rule::Dca, Rule::Momentum]
        .into_iter()
        .map(|rule| simulate(rule, &tape, opening))
        .collect::<Result<Vec<_>, _>>()?;
    let ledger_sha256 = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&runs).map_err(|e| e.to_string())?)
    );
    Ok(Report {
        schema_version: 1,
        model: "close_to_close_spot_v1",
        seed: None,
        basis: "Eighred synthetic historical replay; fixed rules, no fitting, no external flows",
        lot_btc: "0.25",
        benchmark: Rule::Hold,
        cost_model: COST,
        warmup_first_ms: tape[0].close_ms,
        warmup_last_ms: anchor.close_ms,
        holdout_first_ms: tape[WARMUP].close_ms,
        holdout_last_ms: tape.last().expect("validated holdout").close_ms,
        source_sha256: format!("{:x}", Sha256::digest(TAPE)),
        source: tape,
        opening_snapshot,
        runs,
        ledger_sha256,
    })
}

#[cfg(test)]
mod tests;
