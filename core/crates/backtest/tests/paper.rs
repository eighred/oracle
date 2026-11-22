use oracle_backtest::paper::{CADENCE_MS, GRACE_MS, Sample, plan, project};
use oracle_spot_amount::Amount;
use serde_json::json;

#[test]
fn the_next_instruction_is_known_before_settlement_and_replays_to_one_posting() {
    let first = 60_000;
    let mut tape = Vec::new();
    let mut previous = json!({});
    for (i, mark) in ["25", "26", "25", "27", "26", "26", "24", "25", "23"]
        .into_iter()
        .enumerate()
    {
        let close_ms = first + i as u64 * CADENCE_MS;
        tape.push(Sample {
            close_ms,
            known_ms: close_ms + 100,
            mark: Amount::parse(mark).unwrap(),
        });
        let step = project(first, &tape).unwrap();
        assert_eq!(step, project(first, &tape).unwrap());
        if i == 2 {
            assert_eq!(
                step["next_instructions"],
                json!({"hold":null,"dca":"buy","momentum":"sell"})
            );
        }
        if i >= 3 {
            for run in step["runs"].as_array().unwrap() {
                let last = run["ledger"].as_array().unwrap().last().unwrap();
                assert_eq!(
                    last["posting"]["fill"]["side"],
                    previous[run["rule"].as_str().unwrap()]
                );
            }
        }
        if i == 8 {
            assert_eq!(step["runs"][0]["net_pnl"], "-4");
            assert_eq!(step["runs"][1]["net_pnl"], "-6.284515");
            assert_eq!(step["runs"][2]["net_pnl"], "-2.556745");
            assert_eq!(step["next_instructions"], json!({}));
        }
        previous = step["next_instructions"].clone();
    }
    assert!(plan(first + 1).is_err());
    tape[3].known_ms = tape[3].close_ms + GRACE_MS;
    assert!(project(first, &tape).is_err());
    tape.remove(3);
    assert!(project(first, &tape).is_err());
}
