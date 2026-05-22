use regime_core::{
    FairProbabilityDegradeReason, FairProbabilityFeatureWindowMetrics, FairProbabilityInput,
    build_feature_window_from_fair_probability, calculate_fair_probability,
    feature_snapshot_from_windows,
};
use serde_json::json;

#[test]
fn fair_probability_is_neutral_at_the_strike() {
    let result = calculate_fair_probability(FairProbabilityInput {
        current_price: 100_000.0,
        strike_price: 100_000.0,
        time_remaining_ms: 60_000,
        realized_volatility: 0.60,
        feed_lag_ms: 120,
    });

    assert!((result.p_fair - 0.50).abs() < 0.01);
    assert!(!result.degraded);
}

#[test]
fn fair_probability_moves_with_current_price_relative_to_strike() {
    let below = calculate_fair_probability(FairProbabilityInput {
        current_price: 99_800.0,
        strike_price: 100_000.0,
        time_remaining_ms: 60_000,
        realized_volatility: 0.40,
        feed_lag_ms: 100,
    });
    let above = calculate_fair_probability(FairProbabilityInput {
        current_price: 100_200.0,
        strike_price: 100_000.0,
        time_remaining_ms: 60_000,
        realized_volatility: 0.40,
        feed_lag_ms: 100,
    });

    assert!(below.p_fair < 0.50);
    assert!(above.p_fair > 0.50);
    assert!(above.p_fair > below.p_fair);
}

#[test]
fn fair_probability_is_deterministic_after_expiry_or_zero_volatility() {
    let expired_up = calculate_fair_probability(FairProbabilityInput {
        current_price: 100_001.0,
        strike_price: 100_000.0,
        time_remaining_ms: 0,
        realized_volatility: 0.40,
        feed_lag_ms: 100,
    });
    let zero_vol_down = calculate_fair_probability(FairProbabilityInput {
        current_price: 99_999.0,
        strike_price: 100_000.0,
        time_remaining_ms: 60_000,
        realized_volatility: 0.0,
        feed_lag_ms: 100,
    });

    assert_eq!(expired_up.p_fair, 1.0);
    assert_eq!(zero_vol_down.p_fair, 0.0);
}

#[test]
fn fair_probability_degrades_when_reference_feed_is_stale() {
    let result = calculate_fair_probability(FairProbabilityInput {
        current_price: 100_200.0,
        strike_price: 100_000.0,
        time_remaining_ms: 60_000,
        realized_volatility: 0.40,
        feed_lag_ms: 2_500,
    });

    assert!(result.degraded);
    assert_eq!(
        result.degrade_reason,
        Some(FairProbabilityDegradeReason::ReferenceFeedStale)
    );
}

#[test]
fn fair_probability_degrades_invalid_realized_volatility() {
    let result = calculate_fair_probability(FairProbabilityInput {
        current_price: 100_200.0,
        strike_price: 100_000.0,
        time_remaining_ms: 60_000,
        realized_volatility: f64::NAN,
        feed_lag_ms: 100,
    });

    assert_eq!(result.p_fair, 0.5);
    assert!(result.degraded);
    assert_eq!(
        result.degrade_reason,
        Some(FairProbabilityDegradeReason::InvalidVolatilityInput)
    );
}

#[test]
fn fair_probability_input_defaults_missing_feed_lag_to_fresh() {
    let input: FairProbabilityInput = serde_json::from_value(json!({
        "current_price": 100_200.0,
        "strike_price": 100_000.0,
        "time_remaining_ms": 60_000,
        "realized_volatility": 0.40
    }))
    .expect("deserialize fair probability input");

    assert_eq!(input.feed_lag_ms, 0);
}

#[test]
fn feature_window_can_compute_fair_probability_from_market_inputs() {
    let window = build_feature_window_from_fair_probability(
        "btc-updown-5m",
        FairProbabilityFeatureWindowMetrics {
            window_ts_ms: 1_769_000_000_000,
            window_ms: 1_000,
            p_mid: 0.58,
            fair_probability: FairProbabilityInput {
                current_price: 100_200.0,
                strike_price: 100_000.0,
                time_remaining_ms: 60_000,
                realized_volatility: 0.40,
                feed_lag_ms: 100,
            },
            ofi_1s: 0.42,
            depth_imbalance: 0.31,
            spread: 0.03,
            volume_acceleration: 2.1,
        },
    );

    assert!(window.p_fair > 0.50);
    assert!((window.fair_gap - (window.p_mid - window.p_fair)).abs() < 1e-12);
    assert_eq!(window.feature_vector[0], window.fair_gap);
}

#[test]
fn stale_fair_probability_flows_into_feature_snapshot_quality() {
    let previous = build_feature_window_from_fair_probability(
        "btc-updown-5m",
        FairProbabilityFeatureWindowMetrics {
            window_ts_ms: 1_769_000_000_000,
            window_ms: 1_000,
            p_mid: 0.50,
            fair_probability: FairProbabilityInput {
                current_price: 100_000.0,
                strike_price: 100_000.0,
                time_remaining_ms: 60_000,
                realized_volatility: 0.40,
                feed_lag_ms: 100,
            },
            ofi_1s: 0.0,
            depth_imbalance: 0.0,
            spread: 0.02,
            volume_acceleration: 1.0,
        },
    );
    let current = build_feature_window_from_fair_probability(
        "btc-updown-5m",
        FairProbabilityFeatureWindowMetrics {
            window_ts_ms: 1_769_000_001_000,
            window_ms: 1_000,
            p_mid: 0.56,
            fair_probability: FairProbabilityInput {
                current_price: 100_200.0,
                strike_price: 100_000.0,
                time_remaining_ms: 59_000,
                realized_volatility: 0.40,
                feed_lag_ms: 2_500,
            },
            ofi_1s: 0.42,
            depth_imbalance: 0.31,
            spread: 0.03,
            volume_acceleration: 2.1,
        },
    );

    assert!(current.fair_probability_degraded);
    assert_eq!(
        current.fair_probability_degrade_reason,
        Some(FairProbabilityDegradeReason::ReferenceFeedStale)
    );

    let snapshot = feature_snapshot_from_windows(&previous, &current);
    assert_eq!(snapshot.stale_data_penalty, 1.0);
    assert!(!snapshot.liquidity_reliable);
}

#[test]
fn previous_stale_fair_probability_also_degrades_feature_snapshot_quality() {
    let previous = build_feature_window_from_fair_probability(
        "btc-updown-5m",
        FairProbabilityFeatureWindowMetrics {
            window_ts_ms: 1_769_000_000_000,
            window_ms: 1_000,
            p_mid: 0.50,
            fair_probability: FairProbabilityInput {
                current_price: 100_000.0,
                strike_price: 100_000.0,
                time_remaining_ms: 60_000,
                realized_volatility: 0.40,
                feed_lag_ms: 2_500,
            },
            ofi_1s: 0.0,
            depth_imbalance: 0.0,
            spread: 0.02,
            volume_acceleration: 1.0,
        },
    );
    let current = build_feature_window_from_fair_probability(
        "btc-updown-5m",
        FairProbabilityFeatureWindowMetrics {
            window_ts_ms: 1_769_000_001_000,
            window_ms: 1_000,
            p_mid: 0.56,
            fair_probability: FairProbabilityInput {
                current_price: 100_200.0,
                strike_price: 100_000.0,
                time_remaining_ms: 59_000,
                realized_volatility: 0.40,
                feed_lag_ms: 100,
            },
            ofi_1s: 0.42,
            depth_imbalance: 0.31,
            spread: 0.03,
            volume_acceleration: 2.1,
        },
    );

    assert!(previous.fair_probability_degraded);
    assert!(!current.fair_probability_degraded);

    let snapshot = feature_snapshot_from_windows(&previous, &current);
    assert_eq!(snapshot.stale_data_penalty, 1.0);
    assert!(!snapshot.liquidity_reliable);
}
