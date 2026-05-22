use regime_core::{
    AlertDedupPolicy, AlertState, FeatureWindowMetrics, ScoreThresholds, ScoreWeights,
    ShiftDirection, ShiftLabel, ablation_report_from_feature_windows, build_feature_window,
    feature_snapshot_from_windows, generate_alerts_from_feature_windows,
    generate_deduped_alerts_from_feature_windows,
};

fn weights() -> ScoreWeights {
    ScoreWeights {
        fair_gap_velocity: 4.0,
        depth_imbalance: 1.0,
        ofi_1s: 1.0,
        volume_acceleration: 0.5,
        stale_data_penalty: 1.0,
    }
}

fn thresholds() -> ScoreThresholds {
    ScoreThresholds {
        watch: 0.5,
        early_risk: 1.0,
        shift_detected_move: 0.10,
    }
}

#[test]
fn feature_snapshot_from_windows_uses_consecutive_deltas() {
    let previous = build_feature_window(
        "btc-updown-5m",
        FeatureWindowMetrics {
            window_ts_ms: 1_000,
            window_ms: 1_000,
            p_mid: 0.50,
            p_fair: 0.49,
            ofi_1s: 0.10,
            depth_imbalance: 0.20,
            spread: 0.02,
            volume_acceleration: 0.30,
        },
    );
    let current = build_feature_window(
        "btc-updown-5m",
        FeatureWindowMetrics {
            window_ts_ms: 2_000,
            window_ms: 1_000,
            p_mid: 0.56,
            p_fair: 0.50,
            ofi_1s: 0.42,
            depth_imbalance: 0.31,
            spread: 0.03,
            volume_acceleration: 2.1,
        },
    );

    let snapshot = feature_snapshot_from_windows(&previous, &current);

    assert!((snapshot.fair_gap_velocity - 0.05).abs() < 1e-12);
    assert_eq!(snapshot.ofi_1s, 0.42);
    assert!((snapshot.p_mid_delta - 0.06).abs() < 1e-12);
    assert!((snapshot.p_fair_delta - 0.01).abs() < 1e-12);
    assert!(snapshot.liquidity_reliable);
}

#[test]
fn generate_alerts_from_feature_windows_emits_non_equilibrium_states() {
    let windows = vec![
        build_feature_window(
            "btc-updown-5m",
            FeatureWindowMetrics {
                window_ts_ms: 1_000,
                window_ms: 1_000,
                p_mid: 0.50,
                p_fair: 0.49,
                ofi_1s: 0.01,
                depth_imbalance: 0.01,
                spread: 0.02,
                volume_acceleration: 0.01,
            },
        ),
        build_feature_window(
            "btc-updown-5m",
            FeatureWindowMetrics {
                window_ts_ms: 2_000,
                window_ms: 1_000,
                p_mid: 0.56,
                p_fair: 0.50,
                ofi_1s: 0.42,
                depth_imbalance: 0.31,
                spread: 0.03,
                volume_acceleration: 2.1,
            },
        ),
    ];

    let alerts = generate_alerts_from_feature_windows(&windows, &weights(), &thresholds(), 1_000);

    assert_eq!(alerts.len(), 1);
    assert_eq!(alerts[0].timestamp_ms, 2_000);
    assert_eq!(alerts[0].state, AlertState::EarlyRisk);
    assert_eq!(alerts[0].horizon_ms, 1_000);
}

#[test]
fn generate_alerts_from_feature_windows_does_not_score_across_interleaved_markets() {
    let windows = vec![
        build_feature_window(
            "market-a",
            FeatureWindowMetrics {
                window_ts_ms: 0,
                window_ms: 1_000,
                p_mid: 0.90,
                p_fair: 0.50,
                ofi_1s: 0.0,
                depth_imbalance: 0.0,
                spread: 0.02,
                volume_acceleration: 0.0,
            },
        ),
        build_feature_window(
            "market-b",
            FeatureWindowMetrics {
                window_ts_ms: 100,
                window_ms: 1_000,
                p_mid: 0.10,
                p_fair: 0.50,
                ofi_1s: 0.0,
                depth_imbalance: 0.0,
                spread: 0.02,
                volume_acceleration: 0.0,
            },
        ),
        build_feature_window(
            "market-b",
            FeatureWindowMetrics {
                window_ts_ms: 1_100,
                window_ms: 1_000,
                p_mid: 0.11,
                p_fair: 0.50,
                ofi_1s: 0.0,
                depth_imbalance: 0.0,
                spread: 0.02,
                volume_acceleration: 0.0,
            },
        ),
    ];
    let thresholds = ScoreThresholds {
        watch: 0.5,
        early_risk: 1.0,
        shift_detected_move: 0.10,
    };

    let alerts = generate_alerts_from_feature_windows(&windows, &weights(), &thresholds, 1_000);

    assert!(alerts.is_empty());
}

#[test]
fn generate_alerts_from_feature_windows_applies_default_dedup_cooldown() {
    let windows = vec![
        build_feature_window(
            "btc-updown-5m",
            FeatureWindowMetrics {
                window_ts_ms: 0,
                window_ms: 1_000,
                p_mid: 0.50,
                p_fair: 0.49,
                ofi_1s: 0.0,
                depth_imbalance: 0.0,
                spread: 0.02,
                volume_acceleration: 1.0,
            },
        ),
        build_feature_window(
            "btc-updown-5m",
            FeatureWindowMetrics {
                window_ts_ms: 1_000,
                window_ms: 1_000,
                p_mid: 0.54,
                p_fair: 0.49,
                ofi_1s: 0.42,
                depth_imbalance: 0.31,
                spread: 0.03,
                volume_acceleration: 2.1,
            },
        ),
        build_feature_window(
            "btc-updown-5m",
            FeatureWindowMetrics {
                window_ts_ms: 2_000,
                window_ms: 1_000,
                p_mid: 0.58,
                p_fair: 0.49,
                ofi_1s: 0.43,
                depth_imbalance: 0.32,
                spread: 0.03,
                volume_acceleration: 2.2,
            },
        ),
    ];

    let alerts = generate_alerts_from_feature_windows(&windows, &weights(), &thresholds(), 5_000);

    assert_eq!(alerts.len(), 1);
    assert_eq!(alerts[0].timestamp_ms, 1_000);
}

#[test]
fn generate_deduped_alerts_applies_onset_bucket_and_cooldown_per_market_direction() {
    let windows = vec![
        build_feature_window(
            "btc-updown-5m-a",
            FeatureWindowMetrics {
                window_ts_ms: 0,
                window_ms: 1_000,
                p_mid: 0.50,
                p_fair: 0.49,
                ofi_1s: 0.0,
                depth_imbalance: 0.0,
                spread: 0.02,
                volume_acceleration: 1.0,
            },
        ),
        build_feature_window(
            "btc-updown-5m-a",
            FeatureWindowMetrics {
                window_ts_ms: 1_000,
                window_ms: 1_000,
                p_mid: 0.54,
                p_fair: 0.49,
                ofi_1s: 0.42,
                depth_imbalance: 0.31,
                spread: 0.03,
                volume_acceleration: 2.1,
            },
        ),
        build_feature_window(
            "btc-updown-5m-a",
            FeatureWindowMetrics {
                window_ts_ms: 2_000,
                window_ms: 1_000,
                p_mid: 0.58,
                p_fair: 0.49,
                ofi_1s: 0.43,
                depth_imbalance: 0.32,
                spread: 0.03,
                volume_acceleration: 2.2,
            },
        ),
        build_feature_window(
            "btc-updown-5m-a",
            FeatureWindowMetrics {
                window_ts_ms: 7_000,
                window_ms: 1_000,
                p_mid: 0.62,
                p_fair: 0.49,
                ofi_1s: 0.44,
                depth_imbalance: 0.33,
                spread: 0.03,
                volume_acceleration: 2.3,
            },
        ),
    ];
    let policy = AlertDedupPolicy {
        onset_window_ms: 5_000,
        cooldown_ms: 5_000,
    };

    let alerts = generate_deduped_alerts_from_feature_windows(
        &windows,
        &weights(),
        &thresholds(),
        1_000,
        &policy,
    );

    assert_eq!(alerts.len(), 2);
    assert_eq!(alerts[0].timestamp_ms, 1_000);
    assert_eq!(alerts[1].timestamp_ms, 7_000);
    assert_eq!(alerts[0].direction, "UP");
    assert_eq!(alerts[0].dedup_key, "btc-updown-5m-a:UP:0");
    assert_eq!(alerts[1].dedup_key, "btc-updown-5m-a:UP:5000");
}

#[test]
fn generate_deduped_alerts_keeps_separate_markets_and_directions() {
    let windows = vec![
        build_feature_window(
            "btc-updown-5m-a",
            FeatureWindowMetrics {
                window_ts_ms: 0,
                window_ms: 1_000,
                p_mid: 0.50,
                p_fair: 0.50,
                ofi_1s: 0.0,
                depth_imbalance: 0.0,
                spread: 0.02,
                volume_acceleration: 1.0,
            },
        ),
        build_feature_window(
            "btc-updown-5m-a",
            FeatureWindowMetrics {
                window_ts_ms: 1_000,
                window_ms: 1_000,
                p_mid: 0.45,
                p_fair: 0.50,
                ofi_1s: -0.42,
                depth_imbalance: -0.31,
                spread: 0.03,
                volume_acceleration: 2.1,
            },
        ),
        build_feature_window(
            "btc-updown-5m-b",
            FeatureWindowMetrics {
                window_ts_ms: 1_500,
                window_ms: 1_000,
                p_mid: 0.50,
                p_fair: 0.50,
                ofi_1s: 0.0,
                depth_imbalance: 0.0,
                spread: 0.02,
                volume_acceleration: 1.0,
            },
        ),
        build_feature_window(
            "btc-updown-5m-b",
            FeatureWindowMetrics {
                window_ts_ms: 2_000,
                window_ms: 1_000,
                p_mid: 0.55,
                p_fair: 0.50,
                ofi_1s: 0.42,
                depth_imbalance: 0.31,
                spread: 0.03,
                volume_acceleration: 2.1,
            },
        ),
    ];
    let policy = AlertDedupPolicy {
        onset_window_ms: 5_000,
        cooldown_ms: 5_000,
    };

    let alerts = generate_deduped_alerts_from_feature_windows(
        &windows,
        &weights(),
        &thresholds(),
        1_000,
        &policy,
    );

    assert_eq!(alerts.len(), 2);
    assert_eq!(alerts[0].direction, "DOWN");
    assert_eq!(alerts[1].direction, "UP");
    assert!(alerts[0].dedup_key.starts_with("btc-updown-5m-a:DOWN"));
    assert!(alerts[1].dedup_key.starts_with("btc-updown-5m-b:UP"));
}

#[test]
fn generate_deduped_alerts_does_not_score_across_interleaved_markets() {
    let windows = vec![
        build_feature_window(
            "market-a",
            FeatureWindowMetrics {
                window_ts_ms: 0,
                window_ms: 1_000,
                p_mid: 0.90,
                p_fair: 0.50,
                ofi_1s: 0.0,
                depth_imbalance: 0.0,
                spread: 0.02,
                volume_acceleration: 0.0,
            },
        ),
        build_feature_window(
            "market-b",
            FeatureWindowMetrics {
                window_ts_ms: 100,
                window_ms: 1_000,
                p_mid: 0.10,
                p_fair: 0.50,
                ofi_1s: 0.0,
                depth_imbalance: 0.0,
                spread: 0.02,
                volume_acceleration: 0.0,
            },
        ),
        build_feature_window(
            "market-b",
            FeatureWindowMetrics {
                window_ts_ms: 1_100,
                window_ms: 1_000,
                p_mid: 0.11,
                p_fair: 0.50,
                ofi_1s: 0.0,
                depth_imbalance: 0.0,
                spread: 0.02,
                volume_acceleration: 0.0,
            },
        ),
    ];
    let policy = AlertDedupPolicy {
        onset_window_ms: 5_000,
        cooldown_ms: 5_000,
    };
    let thresholds = ScoreThresholds {
        watch: 0.5,
        early_risk: 1.0,
        shift_detected_move: 0.10,
    };

    let alerts = generate_deduped_alerts_from_feature_windows(
        &windows,
        &weights(),
        &thresholds,
        1_000,
        &policy,
    );

    assert!(alerts.is_empty());
}

#[test]
fn ablation_report_shows_metric_drop_when_key_feature_is_removed() {
    let windows = vec![
        build_feature_window(
            "btc-updown-5m",
            FeatureWindowMetrics {
                window_ts_ms: 1_000,
                window_ms: 1_000,
                p_mid: 0.52,
                p_fair: 0.51,
                ofi_1s: 0.0,
                depth_imbalance: 0.0,
                spread: 0.02,
                volume_acceleration: 0.0,
            },
        ),
        build_feature_window(
            "btc-updown-5m",
            FeatureWindowMetrics {
                window_ts_ms: 2_000,
                window_ms: 1_000,
                p_mid: 0.55,
                p_fair: 0.49,
                ofi_1s: 0.0,
                depth_imbalance: 0.0,
                spread: 0.02,
                volume_acceleration: 0.0,
            },
        ),
    ];
    let labels = vec![ShiftLabel {
        baseline_time_ms: 1_500,
        onset_time_ms: 2_500,
        horizon_ms: 1_000,
        direction: ShiftDirection::Up,
        magnitude: 0.12,
    }];
    let weights = ScoreWeights {
        fair_gap_velocity: 40.0,
        depth_imbalance: 1.0,
        ofi_1s: 1.0,
        volume_acceleration: 1.0,
        stale_data_penalty: 1.0,
    };

    let report = ablation_report_from_feature_windows(
        &windows,
        &labels,
        &weights,
        &thresholds(),
        1_000,
        100,
    );

    let baseline = report
        .iter()
        .find(|metric| metric.variant == "baseline")
        .expect("baseline metric");
    let without_fair_gap = report
        .iter()
        .find(|metric| metric.variant == "without_fair_gap_velocity")
        .expect("fair gap ablation metric");

    assert_eq!(baseline.total_alerts, 1);
    assert_eq!(baseline.early, 1);
    assert_eq!(without_fair_gap.total_alerts, 0);
    assert_eq!(without_fair_gap.recall, 0.0);
}
