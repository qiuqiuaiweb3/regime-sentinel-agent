use assert_cmd::Command;
use predicates::str::contains;
use std::{fs, path::PathBuf};

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

#[test]
fn replay_cli_outputs_validation_csv() {
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
        .arg("--format")
        .arg("csv")
        .assert()
        .success()
        .stdout(contains(
            "alert_time_ms,shift_onset_time_ms,lead_time_ms,horizon_ms,timing",
        ))
        .stdout(contains("750,1000,250,1000,Early"));
}

#[test]
fn replay_cli_generates_alerts_from_feature_windows_when_alerts_are_absent() {
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
  "feature_windows": [
    {
      "slug": "btc-updown-5m",
      "window_ts_ms": 0,
      "window_ms": 1000,
      "p_mid": 0.50,
      "p_fair": 0.49,
      "fair_gap": 0.01,
      "ofi_1s": 0.01,
      "depth_imbalance": 0.01,
      "spread": 0.02,
      "volume_acceleration": 0.01,
      "feature_vector": [0.01, 0.01, 0.01, 0.02, 0.01]
    },
    {
      "slug": "btc-updown-5m",
      "window_ts_ms": 750,
      "window_ms": 1000,
      "p_mid": 0.54,
      "p_fair": 0.49,
      "fair_gap": 0.05,
      "ofi_1s": 0.42,
      "depth_imbalance": 0.31,
      "spread": 0.03,
      "volume_acceleration": 2.1,
      "feature_vector": [0.05, 0.42, 0.31, 0.03, 2.1]
    }
  ],
  "score_weights": {
    "fair_gap_velocity": 4.0,
    "depth_imbalance": 1.0,
    "ofi_1s": 1.0,
    "volume_acceleration": 0.5,
    "stale_data_penalty": 1.0
  },
  "score_thresholds": {
    "watch": 0.5,
    "early_risk": 1.0,
    "shift_detected_move": 0.10
  },
  "alert_horizon_ms": 1000,
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
        .stdout(contains(r#""timestamp_ms":750"#))
        .stdout(contains(r#""early":1"#))
        .stdout(contains(r#""lead_time_ms":250"#))
        .stdout(contains(r#""ablation":[{"variant":"baseline""#));
}

#[test]
fn replay_cli_generates_alerts_from_fair_probability_inputs() {
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
  "fair_probability_feature_windows": [
    {
      "slug": "btc-updown-5m",
      "window_ts_ms": 0,
      "window_ms": 1000,
      "p_mid": 0.50,
      "fair_probability": {
        "current_price": 100000.0,
        "strike_price": 100000.0,
        "time_remaining_ms": 60000,
        "realized_volatility": 0.40,
        "feed_lag_ms": 100
      },
      "ofi_1s": 0.01,
      "depth_imbalance": 0.01,
      "spread": 0.02,
      "volume_acceleration": 0.01
    },
    {
      "slug": "btc-updown-5m",
      "window_ts_ms": 750,
      "window_ms": 1000,
      "p_mid": 0.58,
      "fair_probability": {
        "current_price": 100000.0,
        "strike_price": 100000.0,
        "time_remaining_ms": 59250,
        "realized_volatility": 0.40,
        "feed_lag_ms": 100
      },
      "ofi_1s": 0.42,
      "depth_imbalance": 0.31,
      "spread": 0.03,
      "volume_acceleration": 2.1
    }
  ],
  "score_weights": {
    "fair_gap_velocity": 4.0,
    "depth_imbalance": 1.0,
    "ofi_1s": 1.0,
    "volume_acceleration": 0.5,
    "stale_data_penalty": 1.0
  },
  "score_thresholds": {
    "watch": 0.5,
    "early_risk": 1.0,
    "shift_detected_move": 0.10
  },
  "alert_horizon_ms": 1000,
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
        .stdout(contains(r#""timestamp_ms":750"#))
        .stdout(contains(r#""early":1"#))
        .stdout(contains(r#""lead_time_ms":250"#));
}

#[test]
fn replay_cli_acceptance_fixture_reports_1s_5s_30s_lead_time_evidence() {
    let input_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../demo/replay/acceptance-lead-time-window.json");

    Command::cargo_bin("regime-replay")
        .expect("regime-replay binary")
        .arg("--input")
        .arg(input_path)
        .assert()
        .success()
        .stdout(contains(r#""total_alerts":3"#))
        .stdout(contains(r#""median_lead_time_ms":5000.0"#))
        .stdout(contains(r#""p75_lead_time_ms":10000.0"#))
        .stdout(contains(r#""recall":1.0"#))
        .stdout(contains(r#""horizon_ms":1000"#))
        .stdout(contains(r#""horizon_ms":5000"#))
        .stdout(contains(r#""horizon_ms":30000"#));
}
