use regime_core::{
    AlertState, FeatureWindowMetrics, ScoreThresholds, ScoreWeights, build_feature_window,
    feature_snapshot_from_windows, generate_alerts_from_feature_windows,
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
