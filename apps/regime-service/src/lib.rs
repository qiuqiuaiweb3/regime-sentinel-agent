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
    use regime_core::FeatureWindowRecord;

    #[derive(Debug, Clone)]
    pub struct MongoInsertDocument {
        pub collection_name: &'static str,
        pub document: Document,
    }

    pub fn feature_window_insert(window: &FeatureWindowRecord) -> MongoInsertDocument {
        MongoInsertDocument {
            collection_name: "feature_windows",
            document: feature_window_document(window),
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
}

pub mod mongo_store {
    use mongodb::{Database, bson::Document};
    use regime_core::FeatureWindowRecord;

    use crate::mongo_documents::feature_window_insert;

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
