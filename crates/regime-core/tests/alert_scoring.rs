use regime_core::{
    AlertConfidence, AlertState, FeatureSnapshot, ScoreThresholds, ScoreWeights, score_alert,
};

fn default_weights() -> ScoreWeights {
    ScoreWeights {
        fair_gap_velocity: 2.0,
        depth_imbalance: 1.5,
        ofi_1s: 1.0,
        volume_acceleration: 0.5,
        stale_data_penalty: 3.0,
    }
}

fn default_thresholds() -> ScoreThresholds {
    ScoreThresholds {
        watch: 0.5,
        early_risk: 1.0,
        shift_detected_move: 0.10,
    }
}

#[test]
fn score_alert_uses_weighted_absolute_features_and_stale_penalty() {
    let features = FeatureSnapshot {
        fair_gap_velocity: -0.30,
        depth_imbalance: 0.20,
        ofi_1s: -0.10,
        volume_acceleration: 0.40,
        stale_data_penalty: 0.05,
        p_mid_delta: 0.02,
        p_fair_delta: 0.03,
        liquidity_reliable: true,
    };

    let decision = score_alert(&features, &default_weights(), &default_thresholds());

    assert_eq!(decision.state, AlertState::EarlyRisk);
    assert_eq!(decision.confidence, AlertConfidence::Normal);
    assert!((decision.score - 1.05).abs() < 1e-9);
}

#[test]
fn score_alert_prefers_shift_detected_when_market_or_fair_price_already_moved() {
    let features = FeatureSnapshot {
        fair_gap_velocity: 0.01,
        depth_imbalance: 0.01,
        ofi_1s: 0.01,
        volume_acceleration: 0.01,
        stale_data_penalty: 0.0,
        p_mid_delta: -0.11,
        p_fair_delta: 0.02,
        liquidity_reliable: true,
    };

    let decision = score_alert(&features, &default_weights(), &default_thresholds());

    assert_eq!(decision.state, AlertState::ShiftDetected);
    assert_eq!(decision.confidence, AlertConfidence::Normal);
}

#[test]
fn score_alert_downgrades_confidence_when_liquidity_is_unreliable() {
    let features = FeatureSnapshot {
        fair_gap_velocity: 0.40,
        depth_imbalance: 0.30,
        ofi_1s: 0.20,
        volume_acceleration: 0.50,
        stale_data_penalty: 0.0,
        p_mid_delta: 0.01,
        p_fair_delta: 0.01,
        liquidity_reliable: false,
    };

    let decision = score_alert(&features, &default_weights(), &default_thresholds());

    assert_eq!(decision.state, AlertState::EarlyRisk);
    assert_eq!(decision.confidence, AlertConfidence::Low);
}
