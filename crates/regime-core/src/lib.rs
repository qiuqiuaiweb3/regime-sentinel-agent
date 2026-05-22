use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

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

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
pub struct MarketTickMeta {
    pub slug: String,
    pub series: String,
    pub source: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
pub struct MarketTickRecord {
    pub timestamp_ms: i64,
    pub meta: MarketTickMeta,
    pub price: f64,
    pub size: f64,
    pub side: String,
    pub outcome: String,
    pub receive_lag_ms: i64,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
pub struct RegimeStateRecord {
    pub id: String,
    pub regime: String,
    pub confidence: f64,
    pub updated_at_ms: i64,
    pub previous_regime: String,
    pub indicators: serde_json::Value,
    pub market_resolved: bool,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
pub struct AlertEventRecord {
    pub slug: String,
    pub created_at_ms: i64,
    pub severity: String,
    pub state: String,
    pub direction: String,
    pub trigger: String,
    pub message: String,
    pub gemini_explained: bool,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
pub struct AgentSummaryRecord {
    pub bucket_start_ms: i64,
    pub bucket_seconds: i64,
    pub model: String,
    pub thinking_level: String,
    pub summary: String,
    pub alert_ids: Vec<String>,
    pub similar_window_ids: Vec<String>,
    pub token_usage: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
pub struct BacktestRunRecord {
    pub created_at_ms: i64,
    pub parameters: serde_json::Value,
    pub data_range: serde_json::Value,
    pub metrics: serde_json::Value,
    pub ablation: serde_json::Value,
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
pub struct FalseAlertsByMarket {
    pub market_slug: String,
    pub false_alerts: usize,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Serialize)]
pub struct HorizonPrAuc {
    pub horizon_ms: i64,
    pub pr_auc: f64,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
pub struct ValidationMetrics {
    pub median_lead_time_ms: Option<f64>,
    pub p75_lead_time_ms: Option<f64>,
    pub precision: f64,
    pub recall: f64,
    pub false_alerts_per_market: Vec<FalseAlertsByMarket>,
    pub horizon_pr_auc: Vec<HorizonPrAuc>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
pub struct ValidationReport {
    pub results: Vec<AlertValidationResult>,
    pub summary: ValidationSummary,
    pub metrics: ValidationMetrics,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
pub struct AblationMetric {
    pub variant: String,
    pub total_alerts: usize,
    pub early: usize,
    pub synchronous: usize,
    pub late: usize,
    pub false_alerts: usize,
    pub precision: f64,
    pub recall: f64,
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

pub fn feature_snapshot_from_windows(
    previous: &FeatureWindowRecord,
    current: &FeatureWindowRecord,
) -> FeatureSnapshot {
    FeatureSnapshot {
        fair_gap_velocity: current.fair_gap - previous.fair_gap,
        depth_imbalance: current.depth_imbalance,
        ofi_1s: current.ofi_1s,
        volume_acceleration: current.volume_acceleration,
        stale_data_penalty: 0.0,
        p_mid_delta: current.p_mid - previous.p_mid,
        p_fair_delta: current.p_fair - previous.p_fair,
        liquidity_reliable: true,
    }
}

pub fn generate_alerts_from_feature_windows(
    windows: &[FeatureWindowRecord],
    weights: &ScoreWeights,
    thresholds: &ScoreThresholds,
    horizon_ms: i64,
) -> Vec<AlertRecord> {
    windows
        .windows(2)
        .filter_map(|pair| {
            let current = &pair[1];
            let snapshot = feature_snapshot_from_windows(&pair[0], current);
            let decision = score_alert(&snapshot, weights, thresholds);

            if decision.state == AlertState::Equilibrium {
                return None;
            }

            Some(AlertRecord {
                timestamp_ms: current.window_ts_ms,
                state: decision.state,
                confidence: decision.confidence,
                horizon_ms,
                score: decision.score,
            })
        })
        .collect()
}

pub fn ablation_report_from_feature_windows(
    windows: &[FeatureWindowRecord],
    labels: &[ShiftLabel],
    weights: &ScoreWeights,
    thresholds: &ScoreThresholds,
    horizon_ms: i64,
    synchronous_tolerance_ms: i64,
) -> Vec<AblationMetric> {
    let variants = [
        ("baseline", *weights),
        (
            "without_fair_gap_velocity",
            ScoreWeights {
                fair_gap_velocity: 0.0,
                ..*weights
            },
        ),
        (
            "without_depth_imbalance",
            ScoreWeights {
                depth_imbalance: 0.0,
                ..*weights
            },
        ),
        (
            "without_ofi_1s",
            ScoreWeights {
                ofi_1s: 0.0,
                ..*weights
            },
        ),
        (
            "without_volume_acceleration",
            ScoreWeights {
                volume_acceleration: 0.0,
                ..*weights
            },
        ),
    ];

    variants
        .into_iter()
        .map(|(variant, weights)| {
            let alerts =
                generate_alerts_from_feature_windows(windows, &weights, thresholds, horizon_ms);
            let report = validate_alerts(&alerts, labels, synchronous_tolerance_ms);
            AblationMetric {
                variant: variant.to_string(),
                total_alerts: report.summary.total_alerts,
                early: report.summary.early,
                synchronous: report.summary.synchronous,
                late: report.summary.late,
                false_alerts: report.summary.false_alerts,
                precision: report.metrics.precision,
                recall: report.metrics.recall,
            }
        })
        .collect()
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
    validate_alerts_for_market("unknown", alerts, labels, synchronous_tolerance_ms)
}

pub fn validate_alerts_for_market(
    market_slug: impl Into<String>,
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

    let metrics = validation_metrics(market_slug.into(), alerts, labels, &results, &summary);

    ValidationReport {
        results,
        summary,
        metrics,
    }
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

fn validation_metrics(
    market_slug: String,
    alerts: &[AlertRecord],
    labels: &[ShiftLabel],
    results: &[AlertValidationResult],
    summary: &ValidationSummary,
) -> ValidationMetrics {
    let lead_times: Vec<i64> = results
        .iter()
        .filter_map(|result| result.lead_time_ms)
        .collect();
    let matched_alerts = summary.early + summary.synchronous + summary.late;
    let matched_labels: BTreeSet<(i64, i64)> = results
        .iter()
        .filter_map(|result| {
            result
                .shift_onset_time_ms
                .map(|onset| (result.horizon_ms, onset))
        })
        .collect();

    ValidationMetrics {
        median_lead_time_ms: percentile(&lead_times, 0.50),
        p75_lead_time_ms: percentile(&lead_times, 0.75),
        precision: ratio(matched_alerts, alerts.len()),
        recall: ratio(matched_labels.len(), labels.len()),
        false_alerts_per_market: vec![FalseAlertsByMarket {
            market_slug,
            false_alerts: summary.false_alerts,
        }],
        horizon_pr_auc: horizon_pr_auc(alerts, labels),
    }
}

fn percentile(values: &[i64], quantile: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }

    let mut values = values.to_vec();
    values.sort_unstable();

    if quantile == 0.50 && values.len().is_multiple_of(2) {
        let upper = values.len() / 2;
        let lower = upper - 1;
        return Some((values[lower] as f64 + values[upper] as f64) / 2.0);
    }

    let index = ((values.len() as f64 * quantile).ceil() as usize).saturating_sub(1);
    values.get(index).map(|value| *value as f64)
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        return 0.0;
    }

    numerator as f64 / denominator as f64
}

fn horizon_pr_auc(alerts: &[AlertRecord], labels: &[ShiftLabel]) -> Vec<HorizonPrAuc> {
    let horizons: BTreeSet<i64> = labels.iter().map(|label| label.horizon_ms).collect();

    horizons
        .into_iter()
        .map(|horizon_ms| {
            let horizon_labels: Vec<_> = labels
                .iter()
                .filter(|label| label.horizon_ms == horizon_ms)
                .collect();
            let mut horizon_alerts: Vec<_> = alerts
                .iter()
                .filter(|alert| alert.horizon_ms == horizon_ms)
                .collect();
            horizon_alerts.sort_by(|left, right| {
                right
                    .score
                    .partial_cmp(&left.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            HorizonPrAuc {
                horizon_ms,
                pr_auc: average_precision(&horizon_alerts, &horizon_labels),
            }
        })
        .collect()
}

fn average_precision(alerts: &[&AlertRecord], labels: &[&ShiftLabel]) -> f64 {
    if labels.is_empty() || alerts.is_empty() {
        return 0.0;
    }

    let mut true_positives = 0_usize;
    let mut precision_sum = 0.0;
    let mut matched_labels = BTreeSet::new();

    for (index, alert) in alerts.iter().enumerate() {
        let Some(label) = nearest_label_for_horizon_refs(alert, labels) else {
            continue;
        };
        let label_key = (label.horizon_ms, label.onset_time_ms);
        if !matched_labels.insert(label_key) {
            continue;
        }

        true_positives += 1;
        precision_sum += true_positives as f64 / (index + 1) as f64;
    }

    precision_sum / labels.len() as f64
}

fn nearest_label_for_horizon_refs<'a>(
    alert: &AlertRecord,
    labels: &'a [&ShiftLabel],
) -> Option<&'a ShiftLabel> {
    labels
        .iter()
        .copied()
        .filter(|label| label.horizon_ms == alert.horizon_ms)
        .min_by_key(|label| (label.onset_time_ms - alert.timestamp_ms).abs())
}
