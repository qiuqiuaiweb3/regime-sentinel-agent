use regime_core::{FeatureWindowMetrics, build_feature_window};

#[test]
fn build_feature_window_uses_plan_vector_order() {
    let metrics = FeatureWindowMetrics {
        window_ts_ms: 1_769_000_000_000,
        window_ms: 1_000,
        p_mid: 0.52,
        p_fair: 0.49,
        ofi_1s: 0.42,
        depth_imbalance: 0.31,
        spread: 0.03,
        volume_acceleration: 2.1,
    };

    let window = build_feature_window("btc-updown-5m", metrics);

    assert_eq!(window.slug, "btc-updown-5m");
    assert_eq!(window.window_ts_ms, 1_769_000_000_000);
    assert_eq!(window.window_ms, 1_000);
    assert!((window.fair_gap - 0.03).abs() < 1e-12);
    assert_vector_close(window.feature_vector, [0.03, 0.42, 0.31, 0.03, 2.1]);
}

fn assert_vector_close(actual: [f64; 5], expected: [f64; 5]) {
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert!((actual - expected).abs() < 1e-12);
    }
}
