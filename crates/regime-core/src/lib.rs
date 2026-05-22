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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PricePoint {
    pub timestamp_ms: i64,
    pub p_mid: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShiftLabelConfig {
    pub horizons_ms: Vec<i64>,
    pub min_move: f64,
    pub persist_ms: i64,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ShiftDirection {
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShiftLabel {
    pub baseline_time_ms: i64,
    pub onset_time_ms: i64,
    pub horizon_ms: i64,
    pub direction: ShiftDirection,
    pub magnitude: f64,
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

pub fn generate_shift_labels(points: &[PricePoint], config: &ShiftLabelConfig) -> Vec<ShiftLabel> {
    let mut labels = Vec::new();

    for baseline in points {
        for horizon_ms in &config.horizons_ms {
            let Some(onset) = first_point_at_or_after(points, baseline.timestamp_ms + horizon_ms)
            else {
                continue;
            };

            let magnitude = onset.p_mid - baseline.p_mid;
            if magnitude.abs() < config.min_move {
                continue;
            }

            let Some(persisted) =
                first_point_at_or_after(points, onset.timestamp_ms + config.persist_ms)
            else {
                continue;
            };

            let persisted_magnitude = persisted.p_mid - baseline.p_mid;
            if !same_persistent_direction(magnitude, persisted_magnitude, config.min_move) {
                continue;
            }

            labels.push(ShiftLabel {
                baseline_time_ms: baseline.timestamp_ms,
                onset_time_ms: onset.timestamp_ms,
                horizon_ms: *horizon_ms,
                direction: if magnitude.is_sign_positive() {
                    ShiftDirection::Up
                } else {
                    ShiftDirection::Down
                },
                magnitude: magnitude.abs(),
            });
        }
    }

    labels
}

pub fn compute_lead_time_ms(alert_time_ms: i64, shift_onset_time_ms: i64) -> i64 {
    shift_onset_time_ms - alert_time_ms
}

fn first_point_at_or_after(points: &[PricePoint], timestamp_ms: i64) -> Option<&PricePoint> {
    points
        .iter()
        .find(|point| point.timestamp_ms >= timestamp_ms)
}

fn same_persistent_direction(initial: f64, persisted: f64, min_move: f64) -> bool {
    initial.signum() == persisted.signum() && persisted.abs() >= min_move
}
