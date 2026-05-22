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
        IndexModel, SearchIndexModel, SearchIndexType,
        bson::{Document, doc},
        options::{CreateCollectionOptions, IndexOptions, TimeseriesOptions},
    };
    use regime_core::{
        CollectionKind, mongo_collection_specs, mongo_index_specs, vector_search_specs,
    };

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

    #[derive(Debug, Clone)]
    pub struct CollectionSearchIndexModel {
        pub collection_name: &'static str,
        pub index: SearchIndexModel,
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

    pub fn vector_search_index_models() -> Vec<CollectionSearchIndexModel> {
        vector_search_specs()
            .into_iter()
            .map(|spec| CollectionSearchIndexModel {
                collection_name: collection_name(spec.collection),
                index: SearchIndexModel::builder()
                    .definition(doc! {
                        "fields": [{
                            "type": "vector",
                            "path": spec.path,
                            "numDimensions": spec.dimensions as i32,
                            "similarity": spec.similarity,
                        }]
                    })
                    .name(spec.name.to_string())
                    .index_type(SearchIndexType::VectorSearch)
                    .build(),
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

pub mod similar_windows {
    use mongodb::bson::{Bson, Document, doc};
    use regime_core::vector_search_specs;

    pub fn similar_windows_pipeline(slug: &str, query_vector: &[f64], limit: u32) -> Vec<Document> {
        let spec = vector_search_specs()[0];
        let query_vector = query_vector
            .iter()
            .copied()
            .map(Bson::Double)
            .collect::<Vec<_>>();

        vec![
            doc! {
                "$vectorSearch": {
                    "index": spec.name,
                    "path": spec.path,
                    "queryVector": query_vector,
                    "numCandidates": (limit * 20) as i32,
                    "limit": limit as i32,
                    "filter": {
                        "slug": slug,
                    },
                },
            },
            doc! {
                "$project": {
                    "_id": 0,
                    "slug": 1,
                    "window_ts": 1,
                    "window_ms": 1,
                    "p_mid": 1,
                    "p_fair": 1,
                    "fair_gap": 1,
                    "ofi_1s": 1,
                    "depth_imbalance": 1,
                    "spread": 1,
                    "volume_acceleration": 1,
                    "score": {
                        "$meta": "vectorSearchScore",
                    },
                },
            },
        ]
    }
}

pub mod mongo_bootstrap {
    use std::collections::HashSet;

    use mongodb::{Database, bson::Document};
    use serde::Serialize;

    use crate::mongo_indexes::{
        collection_create_models, regular_index_models, vector_search_index_models,
    };

    #[derive(Debug, Clone, Eq, PartialEq)]
    pub struct BootstrapIndexTarget {
        pub collection_name: &'static str,
        pub index_name: String,
    }

    #[derive(Debug, Clone, Eq, PartialEq)]
    pub struct MongoBootstrapPlan {
        pub collections_to_create: Vec<&'static str>,
        pub regular_indexes_to_create: Vec<BootstrapIndexTarget>,
        pub vector_search_indexes_to_create: Vec<BootstrapIndexTarget>,
    }

    #[derive(Debug, Clone, Eq, PartialEq, Serialize)]
    pub struct MongoBootstrapSummary {
        pub collections_created: usize,
        pub regular_indexes_requested: usize,
        pub vector_search_indexes_requested: usize,
    }

    impl MongoBootstrapSummary {
        pub fn from_plan(plan: &MongoBootstrapPlan) -> Self {
            Self {
                collections_created: plan.collections_to_create.len(),
                regular_indexes_requested: plan.regular_indexes_to_create.len(),
                vector_search_indexes_requested: plan.vector_search_indexes_to_create.len(),
            }
        }
    }

    pub fn mongo_bootstrap_plan(
        existing_collection_names: impl IntoIterator<Item = String>,
    ) -> MongoBootstrapPlan {
        let existing_collection_names = existing_collection_names
            .into_iter()
            .collect::<HashSet<_>>();
        let collections_to_create = collection_create_models()
            .into_iter()
            .filter(|model| !existing_collection_names.contains(model.collection_name))
            .map(|model| model.collection_name)
            .collect();

        let regular_indexes_to_create = regular_index_models()
            .into_iter()
            .map(|model| BootstrapIndexTarget {
                collection_name: model.collection_name,
                index_name: model
                    .index
                    .options
                    .and_then(|options| options.name)
                    .expect("regular index model name"),
            })
            .collect();

        let vector_search_indexes_to_create = vector_search_index_models()
            .into_iter()
            .map(|model| BootstrapIndexTarget {
                collection_name: model.collection_name,
                index_name: model.index.name.expect("vector search index model name"),
            })
            .collect();

        MongoBootstrapPlan {
            collections_to_create,
            regular_indexes_to_create,
            vector_search_indexes_to_create,
        }
    }

    pub async fn bootstrap_mongodb(db: &Database) -> mongodb::error::Result<MongoBootstrapSummary> {
        let existing_collection_names = db.list_collection_names().await?;
        let plan = mongo_bootstrap_plan(existing_collection_names);

        for model in collection_create_models()
            .into_iter()
            .filter(|model| plan.collections_to_create.contains(&model.collection_name))
        {
            let mut create = db.create_collection(model.collection_name);
            if let Some(options) = model.options {
                if let Some(timeseries) = options.timeseries {
                    create = create.timeseries(timeseries);
                }
                if let Some(expire_after_seconds) = options.expire_after_seconds {
                    create = create.expire_after_seconds(expire_after_seconds);
                }
            }
            create.await?;
        }

        for model in regular_index_models() {
            db.collection::<Document>(model.collection_name)
                .create_index(model.index)
                .await?;
        }

        for model in vector_search_index_models() {
            db.collection::<Document>(model.collection_name)
                .create_search_index(model.index)
                .await?;
        }

        Ok(MongoBootstrapSummary::from_plan(&plan))
    }
}

pub mod mongo_documents {
    use mongodb::bson::{Bson, DateTime, Document, doc};
    use regime_core::{AlertEventRecord, FeatureWindowRecord, MarketTickRecord, RegimeStateRecord};

    #[derive(Debug, Clone)]
    pub struct MongoInsertDocument {
        pub collection_name: &'static str,
        pub document: Document,
    }

    #[derive(Debug, Clone)]
    pub struct MongoUpdateDocument {
        pub collection_name: &'static str,
        pub filter: Document,
        pub update: Document,
        pub upsert: bool,
    }

    pub fn feature_window_insert(window: &FeatureWindowRecord) -> MongoInsertDocument {
        MongoInsertDocument {
            collection_name: "feature_windows",
            document: feature_window_document(window),
        }
    }

    pub fn market_tick_insert(tick: &MarketTickRecord) -> MongoInsertDocument {
        MongoInsertDocument {
            collection_name: "market_ticks",
            document: market_tick_document(tick),
        }
    }

    pub fn regime_state_upsert(state: &RegimeStateRecord) -> MongoUpdateDocument {
        MongoUpdateDocument {
            collection_name: "regime_states",
            filter: doc! {
                "_id": &state.id,
            },
            update: doc! {
                "$set": regime_state_document(state),
            },
            upsert: true,
        }
    }

    pub fn alert_insert(alert: &AlertEventRecord) -> MongoInsertDocument {
        MongoInsertDocument {
            collection_name: "alerts",
            document: alert_document(alert),
        }
    }

    pub fn feature_window_document(window: &FeatureWindowRecord) -> Document {
        let feature_vector = window
            .feature_vector
            .iter()
            .copied()
            .map(Bson::Double)
            .collect::<Vec<_>>();

        doc! {
            "slug": &window.slug,
            "window_ts": DateTime::from_millis(window.window_ts_ms),
            "window_ms": window.window_ms as i32,
            "p_mid": window.p_mid,
            "p_fair": window.p_fair,
            "fair_gap": window.fair_gap,
            "ofi_1s": window.ofi_1s,
            "depth_imbalance": window.depth_imbalance,
            "spread": window.spread,
            "volume_acceleration": window.volume_acceleration,
            "feature_vector": feature_vector,
        }
    }

    pub fn market_tick_document(tick: &MarketTickRecord) -> Document {
        doc! {
            "timestamp": DateTime::from_millis(tick.timestamp_ms),
            "meta": {
                "slug": &tick.meta.slug,
                "series": &tick.meta.series,
                "source": &tick.meta.source,
            },
            "price": tick.price,
            "size": tick.size,
            "side": &tick.side,
            "outcome": &tick.outcome,
            "receive_lag_ms": tick.receive_lag_ms,
        }
    }

    pub fn regime_state_document(state: &RegimeStateRecord) -> Document {
        doc! {
            "regime": &state.regime,
            "confidence": state.confidence,
            "updated_at": DateTime::from_millis(state.updated_at_ms),
            "previous_regime": &state.previous_regime,
            "indicators": mongodb::bson::to_bson(&state.indicators)
                .expect("serde_json indicators convert to BSON"),
            "market_resolved": state.market_resolved,
        }
    }

    pub fn alert_document(alert: &AlertEventRecord) -> Document {
        doc! {
            "slug": &alert.slug,
            "created_at": DateTime::from_millis(alert.created_at_ms),
            "severity": &alert.severity,
            "state": &alert.state,
            "direction": &alert.direction,
            "trigger": &alert.trigger,
            "message": &alert.message,
            "gemini_explained": alert.gemini_explained,
        }
    }
}

pub mod mongo_store {
    use mongodb::{Database, bson::Document};
    use regime_core::{AlertEventRecord, FeatureWindowRecord, MarketTickRecord, RegimeStateRecord};

    use crate::mongo_documents::{
        alert_insert, feature_window_insert, market_tick_insert, regime_state_upsert,
    };
    use crate::similar_windows::similar_windows_pipeline;

    #[derive(Debug, Clone)]
    pub struct MongoStore {
        db: Database,
    }

    impl MongoStore {
        pub fn new(db: Database) -> Self {
            Self { db }
        }

        pub async fn insert_feature_window(
            &self,
            window: &FeatureWindowRecord,
        ) -> mongodb::error::Result<()> {
            let insert = feature_window_insert(window);
            self.db
                .collection::<Document>(insert.collection_name)
                .insert_one(insert.document)
                .await?;

            Ok(())
        }

        pub async fn insert_market_tick(
            &self,
            tick: &MarketTickRecord,
        ) -> mongodb::error::Result<()> {
            let insert = market_tick_insert(tick);
            self.db
                .collection::<Document>(insert.collection_name)
                .insert_one(insert.document)
                .await?;

            Ok(())
        }

        pub async fn upsert_regime_state(
            &self,
            state: &RegimeStateRecord,
        ) -> mongodb::error::Result<()> {
            let update = regime_state_upsert(state);
            self.db
                .collection::<Document>(update.collection_name)
                .update_one(update.filter, update.update)
                .upsert(update.upsert)
                .await?;

            Ok(())
        }

        pub async fn insert_alert(&self, alert: &AlertEventRecord) -> mongodb::error::Result<()> {
            let insert = alert_insert(alert);
            self.db
                .collection::<Document>(insert.collection_name)
                .insert_one(insert.document)
                .await?;

            Ok(())
        }

        pub async fn find_similar_windows(
            &self,
            slug: &str,
            query_vector: &[f64],
            limit: u32,
        ) -> mongodb::error::Result<Vec<Document>> {
            let mut cursor = self
                .db
                .collection::<Document>("feature_windows")
                .aggregate(similar_windows_pipeline(slug, query_vector, limit))
                .await?;
            let mut documents = Vec::new();

            while cursor.advance().await? {
                documents.push(cursor.deserialize_current()?);
            }

            Ok(documents)
        }
    }
}

pub mod gemini_throttle {
    #[derive(Debug, Clone, Copy, Eq, PartialEq)]
    pub struct GeminiThrottleConfig {
        pub enabled: bool,
        pub summary_interval_minutes: u64,
        pub max_calls_per_hour: u32,
    }

    impl GeminiThrottleConfig {
        pub fn from_env_values(
            enabled: Option<&str>,
            summary_interval_minutes: Option<&str>,
            max_calls_per_hour: Option<&str>,
        ) -> Result<Self, String> {
            let enabled = parse_bool(enabled.unwrap_or("false"))?;
            let summary_interval_minutes = parse_u64(
                summary_interval_minutes.unwrap_or("30"),
                "GEMINI_SUMMARY_INTERVAL_MINUTES",
            )?;
            if summary_interval_minutes < 15 {
                return Err("GEMINI_SUMMARY_INTERVAL_MINUTES must be at least 15".to_string());
            }

            let max_calls_per_hour = parse_u32(
                max_calls_per_hour.unwrap_or("2"),
                "GEMINI_MAX_CALLS_PER_HOUR",
            )?;
            if max_calls_per_hour == 0 {
                return Err("GEMINI_MAX_CALLS_PER_HOUR must be greater than 0".to_string());
            }

            Ok(Self {
                enabled,
                summary_interval_minutes,
                max_calls_per_hour,
            })
        }

        pub fn should_start_summary(
            &self,
            now_ms: i64,
            last_summary_at_ms: Option<i64>,
            calls_started_in_last_hour: u32,
        ) -> bool {
            if !self.enabled || calls_started_in_last_hour >= self.max_calls_per_hour {
                return false;
            }

            let Some(last_summary_at_ms) = last_summary_at_ms else {
                return true;
            };

            let elapsed_ms = now_ms.saturating_sub(last_summary_at_ms);
            elapsed_ms >= (self.summary_interval_minutes as i64) * 60_000
        }
    }

    fn parse_bool(raw: &str) -> Result<bool, String> {
        match raw {
            "true" | "1" | "yes" => Ok(true),
            "false" | "0" | "no" => Ok(false),
            _ => Err("GEMINI_ENABLED must be true or false".to_string()),
        }
    }

    fn parse_u64(raw: &str, name: &str) -> Result<u64, String> {
        raw.parse::<u64>()
            .map_err(|_| format!("{name} must be an unsigned integer"))
    }

    fn parse_u32(raw: &str, name: &str) -> Result<u32, String> {
        raw.parse::<u32>()
            .map_err(|_| format!("{name} must be an unsigned integer"))
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
