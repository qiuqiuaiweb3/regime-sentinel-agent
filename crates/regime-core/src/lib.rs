use serde::{Deserialize, Serialize};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Serialize)]
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

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Serialize)]
pub struct FeatureWindowMetrics {
    pub window_ts_ms: i64,
    pub window_ms: i64,
    pub p_mid: f64,
    pub p_fair: f64,
    pub ofi_1s: f64,
    pub depth_imbalance: f64,
    pub spread: f64,
    pub volume_acceleration: f64,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
pub struct FeatureWindowRecord {
    pub slug: String,
    pub window_ts_ms: i64,
    pub window_ms: i64,
    pub p_mid: f64,
    pub p_fair: f64,
    pub fair_gap: f64,
    pub ofi_1s: f64,
    pub depth_imbalance: f64,
    pub spread: f64,
    pub volume_acceleration: f64,
    pub feature_vector: [f64; 5],
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Serialize)]
pub struct ScoreWeights {
    pub fair_gap_velocity: f64,
    pub depth_imbalance: f64,
    pub ofi_1s: f64,
    pub volume_acceleration: f64,
    pub stale_data_penalty: f64,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Serialize)]
pub struct ScoreThresholds {
    pub watch: f64,
    pub early_risk: f64,
    pub shift_detected_move: f64,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
pub enum AlertState {
    Equilibrium,
    Watch,
    EarlyRisk,
    ShiftDetected,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
pub enum AlertConfidence {
    Normal,
    Low,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Serialize)]
pub struct AlertDecision {
    pub state: AlertState,
    pub confidence: AlertConfidence,
    pub score: f64,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Serialize)]
pub struct PricePoint {
    pub timestamp_ms: i64,
    pub p_mid: f64,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
pub struct ShiftLabelConfig {
    pub horizons_ms: Vec<i64>,
    pub min_move: f64,
    pub persist_ms: i64,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
pub enum ShiftDirection {
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Serialize)]
pub struct ShiftLabel {
    pub baseline_time_ms: i64,
    pub onset_time_ms: i64,
    pub horizon_ms: i64,
    pub direction: ShiftDirection,
    pub magnitude: f64,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Serialize)]
pub struct AlertRecord {
    pub timestamp_ms: i64,
    pub state: AlertState,
    pub confidence: AlertConfidence,
    pub horizon_ms: i64,
    pub score: f64,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
pub enum DetectionTiming {
    Early,
    Synchronous,
    Late,
    FalseAlert,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Serialize)]
pub struct AlertValidationResult {
    pub alert_time_ms: i64,
    pub shift_onset_time_ms: Option<i64>,
    pub lead_time_ms: Option<i64>,
    pub horizon_ms: i64,
    pub timing: DetectionTiming,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
pub struct ValidationSummary {
    pub total_alerts: usize,
    pub early: usize,
    pub synchronous: usize,
    pub late: usize,
    pub false_alerts: usize,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
pub struct ValidationReport {
    pub results: Vec<AlertValidationResult>,
    pub summary: ValidationSummary,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
pub enum CollectionKind {
    MarketTicks,
    FeatureWindows,
    RegimeStates,
    Alerts,
    AgentSummaries,
    BacktestRuns,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
pub struct TimeSeriesSpec {
    pub time_field: &'static str,
    pub meta_field: &'static str,
    pub expire_after_seconds: i64,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
pub struct MongoCollectionSpec {
    pub kind: CollectionKind,
    pub name: &'static str,
    pub time_series: Option<TimeSeriesSpec>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
pub struct MongoIndexSpec {
    pub collection: CollectionKind,
    pub name: &'static str,
    pub fields: &'static [&'static str],
    pub unique: bool,
    pub ttl_seconds: Option<i64>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
pub struct VectorSearchSpec {
    pub collection: CollectionKind,
    pub name: &'static str,
    pub path: &'static str,
    pub dimensions: u32,
    pub similarity: &'static str,
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

pub fn build_feature_window(
    slug: impl Into<String>,
    metrics: FeatureWindowMetrics,
) -> FeatureWindowRecord {
    let fair_gap = metrics.p_mid - metrics.p_fair;

    FeatureWindowRecord {
        slug: slug.into(),
        window_ts_ms: metrics.window_ts_ms,
        window_ms: metrics.window_ms,
        p_mid: metrics.p_mid,
        p_fair: metrics.p_fair,
        fair_gap,
        ofi_1s: metrics.ofi_1s,
        depth_imbalance: metrics.depth_imbalance,
        spread: metrics.spread,
        volume_acceleration: metrics.volume_acceleration,
        feature_vector: [
            fair_gap,
            metrics.ofi_1s,
            metrics.depth_imbalance,
            metrics.spread,
            metrics.volume_acceleration,
        ],
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

pub fn validate_alerts(
    alerts: &[AlertRecord],
    labels: &[ShiftLabel],
    synchronous_tolerance_ms: i64,
) -> ValidationReport {
    let mut results = Vec::with_capacity(alerts.len());
    let mut summary = ValidationSummary {
        total_alerts: alerts.len(),
        early: 0,
        synchronous: 0,
        late: 0,
        false_alerts: 0,
    };

    for alert in alerts {
        let Some(label) = nearest_label_for_horizon(alert, labels) else {
            summary.false_alerts += 1;
            results.push(AlertValidationResult {
                alert_time_ms: alert.timestamp_ms,
                shift_onset_time_ms: None,
                lead_time_ms: None,
                horizon_ms: alert.horizon_ms,
                timing: DetectionTiming::FalseAlert,
            });
            continue;
        };

        let lead_time_ms = compute_lead_time_ms(alert.timestamp_ms, label.onset_time_ms);
        let timing = if lead_time_ms.abs() <= synchronous_tolerance_ms {
            summary.synchronous += 1;
            DetectionTiming::Synchronous
        } else if lead_time_ms > 0 {
            summary.early += 1;
            DetectionTiming::Early
        } else {
            summary.late += 1;
            DetectionTiming::Late
        };

        results.push(AlertValidationResult {
            alert_time_ms: alert.timestamp_ms,
            shift_onset_time_ms: Some(label.onset_time_ms),
            lead_time_ms: Some(lead_time_ms),
            horizon_ms: alert.horizon_ms,
            timing,
        });
    }

    ValidationReport { results, summary }
}

pub fn mongo_collection_names() -> [&'static str; 6] {
    mongo_collection_specs().map(|spec| spec.name)
}

pub fn mongo_collection_specs() -> [MongoCollectionSpec; 6] {
    [
        MongoCollectionSpec {
            kind: CollectionKind::MarketTicks,
            name: "market_ticks",
            time_series: Some(TimeSeriesSpec {
                time_field: "timestamp",
                meta_field: "meta",
                expire_after_seconds: 7 * 24 * 60 * 60,
            }),
        },
        MongoCollectionSpec {
            kind: CollectionKind::FeatureWindows,
            name: "feature_windows",
            time_series: None,
        },
        MongoCollectionSpec {
            kind: CollectionKind::RegimeStates,
            name: "regime_states",
            time_series: None,
        },
        MongoCollectionSpec {
            kind: CollectionKind::Alerts,
            name: "alerts",
            time_series: None,
        },
        MongoCollectionSpec {
            kind: CollectionKind::AgentSummaries,
            name: "agent_summaries",
            time_series: None,
        },
        MongoCollectionSpec {
            kind: CollectionKind::BacktestRuns,
            name: "backtest_runs",
            time_series: None,
        },
    ]
}

pub fn mongo_index_specs() -> [MongoIndexSpec; 6] {
    [
        MongoIndexSpec {
            collection: CollectionKind::MarketTicks,
            name: "market_ticks_slug_timestamp",
            fields: &["meta.slug", "timestamp"],
            unique: false,
            ttl_seconds: None,
        },
        MongoIndexSpec {
            collection: CollectionKind::FeatureWindows,
            name: "feature_windows_slug_window_ts",
            fields: &["slug", "window_ts"],
            unique: true,
            ttl_seconds: None,
        },
        MongoIndexSpec {
            collection: CollectionKind::RegimeStates,
            name: "regime_states_updated_at",
            fields: &["updated_at"],
            unique: false,
            ttl_seconds: None,
        },
        MongoIndexSpec {
            collection: CollectionKind::Alerts,
            name: "alerts_slug_created_at",
            fields: &["slug", "created_at"],
            unique: false,
            ttl_seconds: None,
        },
        MongoIndexSpec {
            collection: CollectionKind::AgentSummaries,
            name: "agent_summaries_bucket_start",
            fields: &["bucket_start"],
            unique: true,
            ttl_seconds: None,
        },
        MongoIndexSpec {
            collection: CollectionKind::BacktestRuns,
            name: "backtest_runs_created_at",
            fields: &["created_at"],
            unique: false,
            ttl_seconds: None,
        },
    ]
}

pub fn vector_search_specs() -> [VectorSearchSpec; 1] {
    [VectorSearchSpec {
        collection: CollectionKind::FeatureWindows,
        name: "feature_windows_vector_search",
        path: "feature_vector",
        dimensions: 5,
        similarity: "cosine",
    }]
}

fn first_point_at_or_after(points: &[PricePoint], timestamp_ms: i64) -> Option<&PricePoint> {
    points
        .iter()
        .find(|point| point.timestamp_ms >= timestamp_ms)
}

fn same_persistent_direction(initial: f64, persisted: f64, min_move: f64) -> bool {
    initial.signum() == persisted.signum() && persisted.abs() >= min_move
}

fn nearest_label_for_horizon<'a>(
    alert: &AlertRecord,
    labels: &'a [ShiftLabel],
) -> Option<&'a ShiftLabel> {
    labels
        .iter()
        .filter(|label| label.horizon_ms == alert.horizon_ms)
        .min_by_key(|label| (label.onset_time_ms - alert.timestamp_ms).abs())
}
