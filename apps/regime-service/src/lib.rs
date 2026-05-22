use axum::{
    Json, Router,
    routing::{get, post},
};
use regime_core::{
    AlertRecord, PricePoint, ShiftLabel, ShiftLabelConfig, ValidationReport, generate_shift_labels,
    validate_alerts,
};
use serde::{Deserialize, Serialize};

pub mod mongo_indexes {
    use std::time::Duration;

    use mongodb::{
        IndexModel,
        bson::Document,
        options::{CreateCollectionOptions, IndexOptions, TimeseriesOptions},
    };
    use regime_core::{CollectionKind, mongo_collection_specs, mongo_index_specs};

    #[derive(Debug, Clone)]
    pub struct CollectionCreateModel {
        pub collection_name: &'static str,
        pub options: Option<CreateCollectionOptions>,
    }

    #[derive(Debug, Clone)]
    pub struct CollectionIndexModel {
        pub collection_name: &'static str,
        pub index: IndexModel,
    }

    pub fn collection_create_models() -> Vec<CollectionCreateModel> {
        mongo_collection_specs()
            .into_iter()
            .map(|spec| {
                let options = spec.time_series.map(|time_series| {
                    let mut options = CreateCollectionOptions::default();
                    options.timeseries = Some(
                        TimeseriesOptions::builder()
                            .time_field(time_series.time_field.to_string())
                            .meta_field(time_series.meta_field.to_string())
                            .build(),
                    );
                    options.expire_after_seconds =
                        Some(Duration::from_secs(time_series.expire_after_seconds as u64));
                    options
                });

                CollectionCreateModel {
                    collection_name: spec.name,
                    options,
                }
            })
            .collect()
    }

    pub fn regular_index_models() -> Vec<CollectionIndexModel> {
        mongo_index_specs()
            .into_iter()
            .map(|spec| {
                let mut keys = Document::new();
                for field in spec.fields {
                    keys.insert(*field, 1);
                }

                let mut options = IndexOptions::default();
                options.name = Some(spec.name.to_string());
                options.unique = spec.unique.then_some(true);
                options.expire_after = spec
                    .ttl_seconds
                    .map(|seconds| Duration::from_secs(seconds as u64));

                CollectionIndexModel {
                    collection_name: collection_name(spec.collection),
                    index: IndexModel::builder().keys(keys).options(options).build(),
                }
            })
            .collect()
    }

    fn collection_name(kind: CollectionKind) -> &'static str {
        match kind {
            CollectionKind::MarketTicks => "market_ticks",
            CollectionKind::FeatureWindows => "feature_windows",
            CollectionKind::RegimeStates => "regime_states",
            CollectionKind::Alerts => "alerts",
            CollectionKind::AgentSummaries => "agent_summaries",
            CollectionKind::BacktestRuns => "backtest_runs",
        }
    }
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
}

#[derive(Debug, Deserialize)]
pub struct ReplayValidationRequest {
    price_points: Vec<PricePoint>,
    alerts: Vec<AlertRecord>,
    label_config: ShiftLabelConfig,
    synchronous_tolerance_ms: i64,
}

#[derive(Debug, Serialize)]
pub struct ReplayValidationResponse {
    labels: Vec<ShiftLabel>,
    report: ValidationReport,
}

pub fn build_router() -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/replay/validate", post(validate_replay))
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "regime-service",
    })
}

async fn validate_replay(
    Json(request): Json<ReplayValidationRequest>,
) -> Json<ReplayValidationResponse> {
    let labels = generate_shift_labels(&request.price_points, &request.label_config);
    let report = validate_alerts(&request.alerts, &labels, request.synchronous_tolerance_ms);

    Json(ReplayValidationResponse { labels, report })
}
