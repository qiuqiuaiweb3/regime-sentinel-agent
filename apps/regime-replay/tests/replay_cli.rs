use assert_cmd::Command;
use predicates::str::contains;
use std::fs;

#[test]
fn replay_cli_outputs_validation_json() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let input_path = temp_dir.path().join("replay.json");
    fs::write(
        &input_path,
        r#"{
  "price_points": [
    {"timestamp_ms": 0, "p_mid": 0.50},
    {"timestamp_ms": 1000, "p_mid": 0.62},
    {"timestamp_ms": 4000, "p_mid": 0.61}
  ],
  "alerts": [
    {
      "timestamp_ms": 750,
      "state": "EarlyRisk",
      "confidence": "Normal",
      "horizon_ms": 1000,
      "score": 1.25
    }
  ],
  "label_config": {
    "horizons_ms": [1000],
    "min_move": 0.10,
    "persist_ms": 3000
  },
  "synchronous_tolerance_ms": 100
}"#,
    )
    .expect("write replay input");

    Command::cargo_bin("regime-replay")
        .expect("regime-replay binary")
        .arg("--input")
        .arg(input_path)
        .assert()
        .success()
        .stdout(contains(r#""early":1"#))
        .stdout(contains(r#""lead_time_ms":250"#));
}
