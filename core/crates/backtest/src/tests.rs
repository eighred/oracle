use super::*;

#[test]
fn published_round_trip_posts_fees_once_and_refusals_do_not_change_balances() {
    let costs = CostModel {
        half_spread_bps: 0,
        slippage_bps: 0,
        ..COST
    };
    let mut ledger = Ledger::new(ratio(1000, 1), ratio(0, 1), &ratio(100, 1));
    let first = ledger
        .fill(Side::Buy, &ratio(2, 1), &ratio(100, 1), &costs)
        .unwrap();
    assert_eq!(first.fee, "0.2");
    assert_eq!(show(&ledger.cash), "799.8");
    assert!(
        ledger
            .fill(Side::Buy, &ratio(8, 1), &ratio(100, 1), &costs)
            .is_err()
    );
    assert!(
        ledger
            .fill(Side::Sell, &ratio(3, 1), &ratio(100, 1), &costs)
            .is_err()
    );
    assert!(
        ledger
            .fill(Side::Buy, &ratio(1, 1_000_000_000), &ratio(100, 1), &costs)
            .is_err()
    );
    let last = ledger
        .fill(Side::Sell, &ratio(2, 1), &ratio(105, 1), &costs)
        .unwrap();
    assert_eq!(last.fee, "0.21");
    assert_eq!(show(&ledger.cash), "1009.59");
    assert_eq!(show(&ledger.quantity), "0");
    assert_eq!(show(&ledger.realized), "9.59");
}

#[test]
fn a_future_price_cannot_change_prior_decisions_or_postings() {
    let mut tape: Vec<Close> = serde_json::from_slice(TAPE).unwrap();
    let anchor = &tape[WARMUP - 1];
    let opening = replay(FIXTURE, anchor.close_ms, anchor.known_ms)
        .unwrap()
        .spot_balances()
        .unwrap();
    let before = simulate(Rule::Momentum, &tape, opening).unwrap();
    tape.last_mut().unwrap().mark = Amount::parse("42").unwrap();
    let after = simulate(Rule::Momentum, &tape, opening).unwrap();
    let n = before.ledger.len() - 1;
    assert_eq!(
        serde_json::to_value(&before.ledger[..n]).unwrap(),
        serde_json::to_value(&after.ledger[..n]).unwrap()
    );
    assert_eq!(before.ledger[n].decision_ms, after.ledger[n].decision_ms);
    assert!(
        matches!((&before.ledger[n].posting,&after.ledger[n].posting),
        (Posting::Filled{fill:a},Posting::Filled{fill:b}) if a.side == b.side)
    );
    assert_ne!(before.closing_nav, after.closing_nav);
}

#[test]
fn a_gap_or_decision_arriving_at_the_fill_instant_refuses_the_run() {
    let mut tape: Vec<Close> = serde_json::from_slice(TAPE).unwrap();
    tape[WARMUP - 1].known_ms = tape[WARMUP].close_ms;
    assert!(
        validate(&tape)
            .unwrap_err()
            .contains("decision unavailable")
    );
    tape.remove(WARMUP);
    assert!(
        validate(&tape)
            .unwrap_err()
            .contains("missing or duplicate")
    );
}
