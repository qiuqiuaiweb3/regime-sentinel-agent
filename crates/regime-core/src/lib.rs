pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FeatureSnapshot {
    pub fair_gap_velocity: f64,
    pub depth_imbalance: f64,
    pub ofi_1s: f64,
    pub volume_acceleration: f64,
    pub stale_data_penalty: f64,
    pub p_mid_delta: f64,
    pub p_fair_delta: f64,
    pub liquidity_reliable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScoreWeights {
    pub fair_gap_velocity: f64,
    pub depth_imbalance: f64,
    pub ofi_1s: f64,
    pub volume_acceleration: f64,
    pub stale_data_penalty: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScoreThresholds {
    pub watch: f64,
    pub early_risk: f64,
    pub shift_detected_move: f64,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum AlertState {
    Equilibrium,
    Watch,
    EarlyRisk,
    ShiftDetected,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum AlertConfidence {
    Normal,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AlertDecision {
    pub state: AlertState,
    pub confidence: AlertConfidence,
    pub score: f64,
}

pub fn score_alert(
    features: &FeatureSnapshot,
    weights: &ScoreWeights,
    thresholds: &ScoreThresholds,
) -> AlertDecision {
    let score = weights.fair_gap_velocity * features.fair_gap_velocity.abs()
        + weights.depth_imbalance * features.depth_imbalance.abs()
        + weights.ofi_1s * features.ofi_1s.abs()
        + weights.volume_acceleration * features.volume_acceleration
        - weights.stale_data_penalty * features.stale_data_penalty;

    let shifted = features.p_mid_delta.abs() >= thresholds.shift_detected_move
        || features.p_fair_delta.abs() >= thresholds.shift_detected_move;

    let state = if shifted {
        AlertState::ShiftDetected
    } else if score >= thresholds.early_risk {
        AlertState::EarlyRisk
    } else if score >= thresholds.watch {
        AlertState::Watch
    } else {
        AlertState::Equilibrium
    };

    let confidence = if features.liquidity_reliable {
        AlertConfidence::Normal
    } else {
        AlertConfidence::Low
    };

    AlertDecision {
        state,
        confidence,
        score,
    }
}
