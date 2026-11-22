use serde_json::{Value, json};
use std::{path::Path, process::Command};

fn replay(valid: &str, known: &str) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_oracle-portfolio-replay"))
        .args([valid, known])
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output.stderr);
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn reconcile_original_corrected_and_subsequent_snapshots() {
    let original = replay("1767225660000", "1767225660100");
    assert_eq!(original["observation_id"], "bar-a");
    assert_eq!(original["nav"], "100");
    assert_eq!(original["inventory_value"], "50");
    assert_eq!(original["base_volume"], "8");
    assert_eq!(original["quote"], "USDT");
    let corrected = replay("1767225660000", "1767225780000");
    assert_eq!(corrected["observation_id"], "bar-a-correction");
    assert_eq!(corrected["nav"], "101");
    let next = replay("1767225720000", "1767225780000");
    assert_eq!(next["nav"], "104");
    assert_eq!(original, replay("1767225660000", "1767225660100"));
}

#[test]
fn missing_or_unreceived_bars_never_carry_forward_a_price() {
    for (valid, known) in [
        ("1767225660000", "1767225660099"), // The market closed, but the first receipt is still in the future.
        ("1767225780000", "1767225840000"), // A whole later interval is missing.
    ] {
        let result = replay(valid, known);
        assert_eq!(result["completeness"], "incomplete");
        assert_eq!(result["reason"], "closed_bar_unavailable_at_cutoff");
        assert_eq!(result.get("nav"), Some(&json!(null)));
        assert!(result.get("mark").is_none());
    }
}

#[test]
fn refusal_is_visible_to_the_cli_caller() {
    let exe = env!("CARGO_BIN_EXE_oracle-portfolio-replay");
    let output = Command::new(exe).args(["2", "1"]).output().unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("known_at_ms")
    );
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let output = Command::new(exe)
        .args(["1", "1"])
        .arg(path)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("approved fixture inventory")
    );
}
