use axum::{
    Json, Router,
    extract::Query,
    extract::State,
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    routing::{get, post},
};
use futures_util::stream;
use regime_core::{
    AblationMetric, AlertRecord, FairProbabilityFeatureWindowRecord, FeatureWindowRecord,
    MarketTickRecord, PricePoint, ScoreThresholds, ScoreWeights, ShiftLabel, ShiftLabelConfig,
    ValidationReport, ablation_report_from_feature_windows,
    build_feature_window_from_fair_probability_record, generate_alerts_from_feature_windows,
    generate_shift_labels, validate_alerts_for_market,
};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::convert::Infallible;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tower_http::services::{ServeDir, ServeFile};

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
        let existing_collection_names_set = existing_collection_names
            .iter()
            .cloned()
            .collect::<HashSet<_>>();
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

        for model in collection_create_models()
            .into_iter()
            .filter(|model| existing_collection_names_set.contains(model.collection_name))
        {
            if let Some(expire_after_seconds) = model
                .options
                .and_then(|options| options.expire_after_seconds)
            {
                db.run_command(mongodb::bson::doc! {
                    "collMod": model.collection_name,
                    "expireAfterSeconds": expire_after_seconds.as_secs() as i64,
                })
                .await?;
            }
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
    use regime_core::{
        AgentSummaryRecord, AlertEventRecord, BacktestRunRecord, FeatureWindowRecord,
        MarketTickRecord, RegimeStateRecord,
    };

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

    #[derive(Debug, Clone)]
    pub struct MongoDeleteDocument {
        pub collection_name: &'static str,
        pub filter: Document,
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

    pub fn market_data_retention_deletes(slugs: &[String]) -> Vec<MongoDeleteDocument> {
        vec![
            MongoDeleteDocument {
                collection_name: "market_ticks",
                filter: doc! { "meta.slug": { "$in": slugs } },
            },
            MongoDeleteDocument {
                collection_name: "feature_windows",
                filter: doc! { "slug": { "$in": slugs } },
            },
            MongoDeleteDocument {
                collection_name: "alerts",
                filter: doc! { "slug": { "$in": slugs } },
            },
            MongoDeleteDocument {
                collection_name: "regime_states",
                filter: doc! { "_id": { "$in": slugs } },
            },
        ]
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

    pub fn agent_summary_insert(summary: &AgentSummaryRecord) -> MongoInsertDocument {
        MongoInsertDocument {
            collection_name: "agent_summaries",
            document: agent_summary_document(summary),
        }
    }

    pub fn agent_summary_bucket_upsert(summary: &AgentSummaryRecord) -> MongoUpdateDocument {
        MongoUpdateDocument {
            collection_name: "agent_summaries",
            filter: doc! {
                "bucket_start": DateTime::from_millis(summary.bucket_start_ms),
            },
            update: doc! {
                "$set": agent_summary_document(summary),
            },
            upsert: true,
        }
    }

    pub fn backtest_run_insert(run: &BacktestRunRecord) -> MongoInsertDocument {
        MongoInsertDocument {
            collection_name: "backtest_runs",
            document: backtest_run_document(run),
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
            "indicators": json_to_bson(&state.indicators),
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

    pub fn agent_summary_document(summary: &AgentSummaryRecord) -> Document {
        doc! {
            "bucket_start": DateTime::from_millis(summary.bucket_start_ms),
            "bucket_seconds": summary.bucket_seconds as i32,
            "model": &summary.model,
            "thinking_level": &summary.thinking_level,
            "summary": &summary.summary,
            "alert_ids": summary.alert_ids.clone(),
            "similar_window_ids": summary.similar_window_ids.clone(),
            "token_usage": json_to_bson(&summary.token_usage),
        }
    }

    pub fn backtest_run_document(run: &BacktestRunRecord) -> Document {
        doc! {
            "created_at": DateTime::from_millis(run.created_at_ms),
            "parameters": json_to_bson(&run.parameters),
            "data_range": json_to_bson(&run.data_range),
            "metrics": json_to_bson(&run.metrics),
            "ablation": json_to_bson(&run.ablation),
        }
    }

    fn json_to_bson(value: &serde_json::Value) -> Bson {
        mongodb::bson::to_bson(value).expect("serde_json value converts to BSON")
    }
}

pub mod mongo_store {
    use mongodb::{
        Database,
        bson::{Document, doc},
    };
    use regime_core::{
        AgentSummaryRecord, AlertEventRecord, BacktestRunRecord, FeatureWindowRecord,
        MarketTickRecord, RegimeStateRecord,
    };

    use crate::mongo_documents::{
        agent_summary_bucket_upsert, agent_summary_insert, alert_insert, backtest_run_insert,
        feature_window_insert, market_data_retention_deletes, market_tick_insert,
        regime_state_upsert,
    };
    use crate::similar_windows::similar_windows_pipeline;

    #[derive(Debug, Clone)]
    pub struct MongoStore {
        db: Database,
    }

    #[derive(Debug, Clone, Default, serde::Serialize)]
    pub struct MarketRetentionReport {
        pub retained_markets: usize,
        pub deleted_slugs: Vec<String>,
        pub deleted_market_ticks: u64,
        pub deleted_feature_windows: u64,
        pub deleted_alerts: u64,
        pub deleted_regime_states: u64,
    }

    impl MongoStore {
        pub fn new(db: Database) -> Self {
            Self { db }
        }

        pub async fn prune_old_market_data(
            &self,
            retained_markets: usize,
        ) -> mongodb::error::Result<MarketRetentionReport> {
            let mut cursor = self
                .db
                .collection::<Document>("market_ticks")
                .aggregate(vec![
                    doc! {
                        "$group": {
                            "_id": "$meta.slug",
                            "last_timestamp": { "$max": "$timestamp" }
                        }
                    },
                    doc! { "$sort": { "last_timestamp": -1 } },
                    doc! { "$skip": retained_markets as i64 },
                    doc! { "$project": { "_id": 1 } },
                ])
                .await?;
            let mut deleted_slugs = Vec::new();
            while cursor.advance().await? {
                let document = cursor.deserialize_current()?;
                if let Ok(slug) = document.get_str("_id") {
                    deleted_slugs.push(slug.to_string());
                }
            }

            if deleted_slugs.is_empty() {
                return Ok(MarketRetentionReport {
                    retained_markets,
                    ..MarketRetentionReport::default()
                });
            }

            let mut report = MarketRetentionReport {
                retained_markets,
                deleted_slugs: deleted_slugs.clone(),
                ..MarketRetentionReport::default()
            };
            for delete in market_data_retention_deletes(&deleted_slugs) {
                let deleted_count = self
                    .db
                    .collection::<Document>(delete.collection_name)
                    .delete_many(delete.filter)
                    .await?
                    .deleted_count;
                match delete.collection_name {
                    "market_ticks" => report.deleted_market_ticks = deleted_count,
                    "feature_windows" => report.deleted_feature_windows = deleted_count,
                    "alerts" => report.deleted_alerts = deleted_count,
                    "regime_states" => report.deleted_regime_states = deleted_count,
                    _ => {}
                }
            }

            Ok(report)
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

        pub async fn insert_agent_summary(
            &self,
            summary: &AgentSummaryRecord,
        ) -> mongodb::error::Result<()> {
            let insert = agent_summary_insert(summary);
            self.db
                .collection::<Document>(insert.collection_name)
                .insert_one(insert.document)
                .await?;

            Ok(())
        }

        pub async fn replace_agent_summary(
            &self,
            summary: &AgentSummaryRecord,
        ) -> mongodb::error::Result<()> {
            let update = agent_summary_bucket_upsert(summary);
            self.db
                .collection::<Document>(update.collection_name)
                .update_one(update.filter, update.update)
                .upsert(update.upsert)
                .await?;

            self.db
                .collection::<Document>("agent_summaries")
                .delete_many(doc! {
                    "bucket_start": {
                        "$lt": mongodb::bson::DateTime::from_millis(summary.bucket_start_ms)
                    }
                })
                .await?;

            Ok(())
        }

        pub async fn insert_backtest_run(
            &self,
            run: &BacktestRunRecord,
        ) -> mongodb::error::Result<()> {
            let insert = backtest_run_insert(run);
            self.db
                .collection::<Document>(insert.collection_name)
                .insert_one(insert.document)
                .await?;

            Ok(())
        }

        pub async fn latest_regime_state(
            &self,
            slug: Option<&str>,
        ) -> mongodb::error::Result<Option<Document>> {
            let filter = slug.map(|slug| doc! { "_id": slug }).unwrap_or_default();
            self.db
                .collection::<Document>("regime_states")
                .find_one(filter)
                .sort(doc! { "updated_at": -1 })
                .await
        }

        pub async fn recent_alerts(
            &self,
            slug: Option<&str>,
            limit: i64,
        ) -> mongodb::error::Result<Vec<Document>> {
            let filter = slug.map(|slug| doc! { "slug": slug }).unwrap_or_default();
            let mut cursor = self
                .db
                .collection::<Document>("alerts")
                .find(filter)
                .sort(doc! { "created_at": -1 })
                .limit(limit)
                .await?;
            let mut documents = Vec::new();

            while cursor.advance().await? {
                documents.push(cursor.deserialize_current()?);
            }

            Ok(documents)
        }

        pub async fn recent_backtest_runs(
            &self,
            limit: i64,
        ) -> mongodb::error::Result<Vec<Document>> {
            let mut cursor = self
                .db
                .collection::<Document>("backtest_runs")
                .find(doc! {})
                .sort(doc! { "created_at": -1 })
                .limit(limit)
                .await?;
            let mut documents = Vec::new();

            while cursor.advance().await? {
                documents.push(cursor.deserialize_current()?);
            }

            Ok(documents)
        }

        pub async fn latest_agent_summary(&self) -> mongodb::error::Result<Option<Document>> {
            self.db
                .collection::<Document>("agent_summaries")
                .find_one(doc! {})
                .sort(doc! { "bucket_start": -1 })
                .await
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

pub mod demo_seed {
    use crate::mongo_documents::{
        agent_summary_document, alert_document, backtest_run_document, feature_window_document,
        market_tick_document, regime_state_document,
    };
    use mongodb::Database;
    use mongodb::bson::{Document, doc};
    use regime_core::{
        AgentSummaryRecord, AlertEventRecord, BacktestRunRecord, FeatureWindowMetrics,
        FeatureWindowRecord, MarketTickMeta, MarketTickRecord, RegimeStateRecord,
        build_feature_window,
    };
    use serde::Serialize;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[derive(Debug, Clone)]
    pub struct DemoSeedRecords {
        pub market_tick: MarketTickRecord,
        pub feature_window: FeatureWindowRecord,
        pub regime_state: RegimeStateRecord,
        pub alert: AlertEventRecord,
        pub agent_summary: AgentSummaryRecord,
        pub backtest_run: BacktestRunRecord,
    }

    impl DemoSeedRecords {
        pub fn collection_names(&self) -> [&'static str; 6] {
            [
                "market_ticks",
                "feature_windows",
                "regime_states",
                "alerts",
                "agent_summaries",
                "backtest_runs",
            ]
        }
    }

    #[derive(Debug, Serialize)]
    pub struct DemoSeedSummary {
        pub run_id: String,
        pub slug: String,
        pub written_collections: [&'static str; 6],
        pub early_alert_lead_time_ms: i64,
    }

    #[derive(Debug, Clone)]
    pub struct DemoSeedCountQuery {
        pub collection_name: &'static str,
        pub filter: Document,
    }

    #[derive(Debug, Serialize)]
    pub struct DemoSeedCount {
        pub collection_name: &'static str,
        pub count: u64,
    }

    pub fn demo_seed_records() -> DemoSeedRecords {
        let run_id = generate_demo_seed_run_id();
        demo_seed_records_at(unix_timestamp_ms(), &run_id)
    }

    pub fn generate_demo_seed_run_id() -> String {
        format!("demo-{}", unix_timestamp_ms())
    }

    pub fn demo_seed_records_at(base_ms: i64, run_id: &str) -> DemoSeedRecords {
        let slug = "btc-updown-5m-demo";
        let market_tick = MarketTickRecord {
            timestamp_ms: base_ms,
            meta: MarketTickMeta {
                slug: slug.to_string(),
                series: "btc-updown-5m".to_string(),
                source: "demo-replay".to_string(),
            },
            price: 0.54,
            size: 100.0,
            side: "BUY".to_string(),
            outcome: "UP".to_string(),
            receive_lag_ms: 42,
        };
        let feature_window = build_feature_window(
            slug,
            FeatureWindowMetrics {
                window_ts_ms: base_ms,
                window_ms: 1_000,
                p_mid: 0.54,
                p_fair: 0.49,
                ofi_1s: 0.42,
                depth_imbalance: 0.31,
                spread: 0.03,
                volume_acceleration: 2.1,
            },
        );
        let regime_state = RegimeStateRecord {
            id: slug.to_string(),
            regime: "EARLY_RISK".to_string(),
            confidence: 0.71,
            updated_at_ms: base_ms,
            previous_regime: "EQUILIBRIUM".to_string(),
            indicators: json!({
                "fair_gap": 0.05,
                "ofi_1s": 0.42,
                "depth_imbalance": 0.31,
                "volume_acceleration": 2.1,
                "lead_time_ms": 250,
                "demo_run_id": run_id
            }),
            market_resolved: false,
        };
        let alert = AlertEventRecord {
            slug: slug.to_string(),
            created_at_ms: base_ms,
            severity: "HIGH".to_string(),
            state: "EARLY_RISK".to_string(),
            direction: "UP".to_string(),
            trigger: "fair_gap_velocity+ofi_1s+volume_acceleration".to_string(),
            message: "Demo early-risk alert fired 250ms before the labeled shift.".to_string(),
            gemini_explained: false,
        };
        let agent_summary = AgentSummaryRecord {
            bucket_start_ms: base_ms,
            bucket_seconds: 1_800,
            model: "gemini-3-flash-preview".to_string(),
            thinking_level: "LOW".to_string(),
            summary:
                "Demo summary: order flow, fair-gap movement, and volume acceleration raised early regime-shift risk."
                    .to_string(),
            alert_ids: vec!["demo-alert-early-risk".to_string()],
            similar_window_ids: vec!["demo-window-high-volatility".to_string()],
            token_usage: json!({ "estimated": true, "demo_run_id": run_id }),
        };
        let backtest_run = BacktestRunRecord {
            created_at_ms: base_ms,
            parameters: json!({
                "input": "demo/replay/high-volatility-btc-window.json",
                "alert_horizon_ms": 1000,
                "synchronous_tolerance_ms": 100,
                "demo_run_id": run_id
            }),
            data_range: json!({
                "window_id": "high-volatility-btc-window",
                "labels_generated": 3
            }),
            metrics: json!({
                "median_lead_time_ms": 250.0,
                "p75_lead_time_ms": 250.0,
                "precision": 1.0,
                "recall": 0.3333333333333333
            }),
            ablation: json!([
                { "variant": "baseline", "early": 1, "false_alerts": 0 },
                { "variant": "without_volume_acceleration", "early": 0, "false_alerts": 0 }
            ]),
        };

        DemoSeedRecords {
            market_tick,
            feature_window,
            regime_state,
            alert,
            agent_summary,
            backtest_run,
        }
    }

    fn unix_timestamp_ms() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time is after Unix epoch")
            .as_millis() as i64
    }

    pub fn demo_seed_count_queries(run_id: &str) -> [DemoSeedCountQuery; 6] {
        let slug = "btc-updown-5m-demo";
        [
            DemoSeedCountQuery {
                collection_name: "market_ticks",
                filter: doc! { "meta.slug": slug, "demo_run_id": run_id },
            },
            DemoSeedCountQuery {
                collection_name: "feature_windows",
                filter: doc! { "slug": slug, "demo_run_id": run_id },
            },
            DemoSeedCountQuery {
                collection_name: "regime_states",
                filter: doc! {
                    "_id": slug,
                    "demo_run_id": run_id,
                    "indicators.demo_run_id": run_id,
                },
            },
            DemoSeedCountQuery {
                collection_name: "alerts",
                filter: doc! { "slug": slug, "demo_run_id": run_id },
            },
            DemoSeedCountQuery {
                collection_name: "agent_summaries",
                filter: doc! { "alert_ids": "demo-alert-early-risk", "demo_run_id": run_id },
            },
            DemoSeedCountQuery {
                collection_name: "backtest_runs",
                filter: doc! {
                    "data_range.window_id": "high-volatility-btc-window",
                    "demo_run_id": run_id,
                },
            },
        ]
    }

    pub async fn write_demo_seed(
        db: &Database,
        run_id: &str,
    ) -> mongodb::error::Result<DemoSeedSummary> {
        let records = demo_seed_records_at(unix_timestamp_ms(), run_id);
        db.collection::<Document>("market_ticks")
            .insert_one(with_demo_run_id(
                market_tick_document(&records.market_tick),
                run_id,
            ))
            .await?;
        db.collection::<Document>("feature_windows")
            .insert_one(with_demo_run_id(
                feature_window_document(&records.feature_window),
                run_id,
            ))
            .await?;

        let mut regime_document =
            with_demo_run_id(regime_state_document(&records.regime_state), run_id);
        regime_document.insert("_id", records.regime_state.id.clone());
        db.collection::<Document>("regime_states")
            .replace_one(doc! { "_id": &records.regime_state.id }, regime_document)
            .upsert(true)
            .await?;

        db.collection::<Document>("alerts")
            .insert_one(with_demo_run_id(alert_document(&records.alert), run_id))
            .await?;
        db.collection::<Document>("agent_summaries")
            .insert_one(with_demo_run_id(
                agent_summary_document(&records.agent_summary),
                run_id,
            ))
            .await?;
        db.collection::<Document>("backtest_runs")
            .insert_one(with_demo_run_id(
                backtest_run_document(&records.backtest_run),
                run_id,
            ))
            .await?;

        Ok(DemoSeedSummary {
            run_id: run_id.to_string(),
            slug: records.regime_state.id.clone(),
            written_collections: records.collection_names(),
            early_alert_lead_time_ms: 250,
        })
    }

    pub async fn count_demo_seed(
        db: &mongodb::Database,
        run_id: &str,
    ) -> mongodb::error::Result<Vec<DemoSeedCount>> {
        let mut counts = Vec::new();
        for query in demo_seed_count_queries(run_id) {
            let count = db
                .collection::<Document>(query.collection_name)
                .count_documents(query.filter)
                .await?;
            counts.push(DemoSeedCount {
                collection_name: query.collection_name,
                count,
            });
        }
        Ok(counts)
    }

    pub fn validate_demo_seed_counts(counts: &[DemoSeedCount]) -> Result<(), String> {
        let missing = counts
            .iter()
            .filter(|count| count.count == 0)
            .map(|count| count.collection_name)
            .collect::<Vec<_>>();
        if missing.is_empty() {
            return Ok(());
        }

        Err(format!(
            "missing demo seed documents in collections: {}",
            missing.join(", ")
        ))
    }

    fn with_demo_run_id(mut document: Document, run_id: &str) -> Document {
        document.insert("demo_run_id", run_id);
        document
    }
}

pub mod gemini_throttle {
    use std::sync::{Arc, Mutex};

    #[derive(Debug, Clone, Copy, Eq, PartialEq)]
    pub struct GeminiThrottleConfig {
        pub enabled: bool,
        pub summary_interval_minutes: u64,
        pub max_calls_per_hour: u32,
        pub manual_cooldown_seconds: u64,
    }

    #[derive(Debug, Clone, Default)]
    pub struct GeminiCallBudget {
        calls_started_at_ms: Arc<Mutex<Vec<i64>>>,
    }

    impl GeminiCallBudget {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn reserve_summary_call(
            &self,
            config: &GeminiThrottleConfig,
            now_ms: i64,
            last_summary_at_ms: Option<i64>,
        ) -> bool {
            let mut calls = self.calls_started_at_ms(now_ms);
            if !config.should_start_summary(now_ms, last_summary_at_ms, calls.len() as u32) {
                return false;
            }
            calls.push(now_ms);
            true
        }

        pub fn reserve_manual_explain_call(
            &self,
            config: &GeminiThrottleConfig,
            now_ms: i64,
            last_manual_explain_at_ms: Option<i64>,
        ) -> bool {
            let mut calls = self.calls_started_at_ms(now_ms);
            if !config.should_start_manual_explain(
                now_ms,
                last_manual_explain_at_ms,
                calls.len() as u32,
            ) {
                return false;
            }
            calls.push(now_ms);
            true
        }

        pub fn calls_started_in_last_hour(&self, now_ms: i64) -> u32 {
            self.calls_started_at_ms(now_ms).len() as u32
        }

        fn calls_started_at_ms(&self, now_ms: i64) -> std::sync::MutexGuard<'_, Vec<i64>> {
            let mut calls = self
                .calls_started_at_ms
                .lock()
                .expect("Gemini call budget lock is not poisoned");
            calls.retain(|started_at_ms| now_ms.saturating_sub(*started_at_ms) < 3_600_000);
            calls
        }
    }

    impl GeminiThrottleConfig {
        pub fn from_env_values(
            enabled: Option<&str>,
            summary_interval_minutes: Option<&str>,
            max_calls_per_hour: Option<&str>,
            manual_cooldown_seconds: Option<&str>,
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
            let manual_cooldown_seconds = parse_u64(
                manual_cooldown_seconds.unwrap_or("300"),
                "GEMINI_MANUAL_COOLDOWN_SECONDS",
            )?;
            if manual_cooldown_seconds == 0 {
                return Err("GEMINI_MANUAL_COOLDOWN_SECONDS must be greater than 0".to_string());
            }

            Ok(Self {
                enabled,
                summary_interval_minutes,
                max_calls_per_hour,
                manual_cooldown_seconds,
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

        pub fn should_start_manual_explain(
            &self,
            now_ms: i64,
            last_manual_explain_at_ms: Option<i64>,
            calls_started_in_last_hour: u32,
        ) -> bool {
            if !self.enabled || calls_started_in_last_hour >= self.max_calls_per_hour {
                return false;
            }

            self.manual_retry_after_seconds(now_ms, last_manual_explain_at_ms) == Some(0)
        }

        pub fn manual_retry_after_seconds(
            &self,
            now_ms: i64,
            last_manual_explain_at_ms: Option<i64>,
        ) -> Option<u64> {
            let Some(last_manual_explain_at_ms) = last_manual_explain_at_ms else {
                return Some(0);
            };

            let cooldown_ms = self.manual_cooldown_seconds as i64 * 1_000;
            let elapsed_ms = now_ms.saturating_sub(last_manual_explain_at_ms);
            if elapsed_ms >= cooldown_ms {
                return Some(0);
            }

            Some(((cooldown_ms - elapsed_ms) as u64).div_ceil(1_000))
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

pub mod gemini_summary {
    use crate::gemini_throttle::GeminiCallBudget;
    use crate::gemini_throttle::GeminiThrottleConfig;
    use anyhow::{Context, anyhow};
    use regime_core::{AgentSummaryRecord, RegimeStateRecord};
    use serde_json::{Value, json};
    use std::path::Path;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    pub const DEFAULT_GEMINI_ENDPOINT_BASE: &str =
        "https://generativelanguage.googleapis.com/v1beta";
    pub const DEFAULT_GEMINI_MODEL: &str = "gemini-3-pro-preview";
    pub const DEFAULT_GEMINI_THINKING_LEVEL: &str = "low";
    pub const DEFAULT_GEMINI_LOCATION: &str = "asia-northeast3";

    #[derive(Debug, Clone, Copy, Eq, PartialEq)]
    pub enum GeminiProvider {
        VertexAi,
        DeveloperApi,
    }

    #[derive(Debug, Clone, Eq, PartialEq)]
    pub enum GeminiAuth {
        ApiKey(String),
        BearerToken(String),
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct GeminiSummaryConfig {
        pub throttle: GeminiThrottleConfig,
        pub provider: GeminiProvider,
        pub api_key: Option<String>,
        pub access_token: Option<String>,
        pub project_id: Option<String>,
        pub location: String,
        pub model: String,
        pub endpoint_base: String,
        pub thinking_level: String,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct GeminiGenerateRequest {
        pub url: String,
        pub auth: Option<GeminiAuth>,
        pub body: Value,
    }

    impl GeminiSummaryConfig {
        #[allow(clippy::too_many_arguments)]
        pub fn from_env_values(
            enabled: Option<&str>,
            summary_interval_minutes: Option<&str>,
            max_calls_per_hour: Option<&str>,
            manual_cooldown_seconds: Option<&str>,
            api_key: Option<&str>,
            model: Option<&str>,
            endpoint_base: Option<&str>,
            provider: Option<&str>,
            project_id: Option<&str>,
            location: Option<&str>,
            access_token: Option<&str>,
        ) -> Result<Self, String> {
            Ok(Self {
                throttle: GeminiThrottleConfig::from_env_values(
                    enabled,
                    summary_interval_minutes,
                    max_calls_per_hour,
                    manual_cooldown_seconds,
                )?,
                provider: parse_provider(provider.unwrap_or("vertex"))?,
                api_key: api_key.map(str::to_string),
                access_token: access_token.map(str::to_string),
                project_id: project_id.map(str::to_string),
                location: location.unwrap_or(DEFAULT_GEMINI_LOCATION).to_string(),
                model: model.unwrap_or(DEFAULT_GEMINI_MODEL).to_string(),
                endpoint_base: endpoint_base
                    .unwrap_or(DEFAULT_GEMINI_ENDPOINT_BASE)
                    .trim_end_matches('/')
                    .to_string(),
                thinking_level: DEFAULT_GEMINI_THINKING_LEVEL.to_string(),
            })
        }

        pub fn from_env() -> Result<Self, String> {
            Self::from_env_values(
                std::env::var("GEMINI_ENABLED").ok().as_deref(),
                std::env::var("GEMINI_SUMMARY_INTERVAL_MINUTES")
                    .ok()
                    .as_deref(),
                std::env::var("GEMINI_MAX_CALLS_PER_HOUR").ok().as_deref(),
                std::env::var("GEMINI_MANUAL_COOLDOWN_SECONDS")
                    .or_else(|_| std::env::var("MANUAL_EXPLAIN_COOLDOWN_SECONDS"))
                    .ok()
                    .as_deref(),
                std::env::var("GEMINI_API_KEY").ok().as_deref(),
                std::env::var("GEMINI_MODEL").ok().as_deref(),
                std::env::var("GEMINI_ENDPOINT_BASE").ok().as_deref(),
                std::env::var("GEMINI_PROVIDER").ok().as_deref(),
                std::env::var("GOOGLE_CLOUD_PROJECT").ok().as_deref(),
                std::env::var("GEMINI_LOCATION")
                    .or_else(|_| std::env::var("GOOGLE_CLOUD_REGION"))
                    .ok()
                    .as_deref(),
                std::env::var("GEMINI_ACCESS_TOKEN").ok().as_deref(),
            )
        }
    }

    pub fn build_gemini_request(
        config: &GeminiSummaryConfig,
        prompt: &str,
    ) -> Result<GeminiGenerateRequest, String> {
        let mut body = json!({
            "contents": [
                {
                    "role": "user",
                    "parts": [
                        { "text": prompt }
                    ]
                }
            ],
            "generationConfig": {
                "temperature": 0.2,
                "maxOutputTokens": 1024
            }
        });
        if config.model.starts_with("gemini-3") {
            body["generationConfig"]["thinkingConfig"] = json!({
                "thinkingLevel": config.thinking_level.to_ascii_uppercase()
            });
        }

        match config.provider {
            GeminiProvider::DeveloperApi => {
                let api_key = config.api_key.clone().ok_or_else(|| {
                    "GEMINI_API_KEY is required when GEMINI_PROVIDER=developer_api".to_string()
                })?;
                Ok(GeminiGenerateRequest {
                    url: format!(
                        "{}/models/{}:generateContent",
                        config.endpoint_base, config.model
                    ),
                    auth: Some(GeminiAuth::ApiKey(api_key)),
                    body,
                })
            }
            GeminiProvider::VertexAi => {
                let project_id = config.project_id.as_deref().ok_or_else(|| {
                    "GOOGLE_CLOUD_PROJECT is required when GEMINI_PROVIDER=vertex".to_string()
                })?;
                Ok(GeminiGenerateRequest {
                    url: format!(
                        "{}/projects/{}/locations/{}/publishers/google/models/{}:generateContent",
                        vertex_endpoint_base(&config.location, &config.model),
                        project_id,
                        config.location,
                        config.model
                    ),
                    auth: config.access_token.clone().map(GeminiAuth::BearerToken),
                    body,
                })
            }
        }
    }

    fn parse_provider(raw: &str) -> Result<GeminiProvider, String> {
        match raw {
            "vertex" | "vertex_ai" => Ok(GeminiProvider::VertexAi),
            "developer_api" | "api_key" => Ok(GeminiProvider::DeveloperApi),
            _ => Err("GEMINI_PROVIDER must be vertex or developer_api".to_string()),
        }
    }

    fn vertex_endpoint_base(location: &str, model: &str) -> String {
        if location == "global" || model.starts_with("gemini-3") {
            "https://aiplatform.googleapis.com/v1".to_string()
        } else {
            format!("https://{location}-aiplatform.googleapis.com/v1")
        }
    }

    async fn metadata_access_token(client: &reqwest::Client) -> anyhow::Result<String> {
        let body = client
            .get("http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token")
            .header("Metadata-Flavor", "Google")
            .send()
            .await
            .context("request Cloud Run metadata access token")?
            .json::<Value>()
            .await
            .context("decode Cloud Run metadata access token")?;

        body.get("access_token")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| anyhow!("metadata token response did not include access_token"))
    }

    pub fn build_summary_prompt(state: &RegimeStateRecord, recent_alert_count: usize) -> String {
        format!(
            "Summarize the current prediction-market regime for a hackathon dashboard in 3 short bullet points. State: {}. Previous state: {}. Confidence: {:.2}. Market resolved: {}. There are {} recent alerts. Indicators JSON: {}. Do not suggest trades or position sizing.",
            state.regime,
            state.previous_regime,
            state.confidence,
            state.market_resolved,
            recent_alert_count,
            state.indicators
        )
    }

    pub fn demo_summary_state(now_ms: i64) -> RegimeStateRecord {
        RegimeStateRecord {
            id: "btc-updown-5m-demo".to_string(),
            regime: "EARLY_RISK".to_string(),
            confidence: 0.71,
            updated_at_ms: now_ms,
            previous_regime: "WATCH".to_string(),
            indicators: json!({
                "fair_gap": 0.05,
                "ofi_1s": 0.42,
                "depth_imbalance": 0.31,
                "volume_acceleration": 2.1,
                "lead_time_ms": 250
            }),
            market_resolved: false,
        }
    }

    pub fn parse_gemini_text(value: &Value) -> Result<String, String> {
        value
            .get("candidates")
            .and_then(Value::as_array)
            .and_then(|candidates| candidates.first())
            .and_then(|candidate| candidate.get("content"))
            .and_then(|content| content.get("parts"))
            .and_then(Value::as_array)
            .and_then(|parts| {
                parts
                    .iter()
                    .find(|part| part.get("text").and_then(Value::as_str).is_some())
            })
            .and_then(|part| part.get("text"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| {
                format!(
                    "Gemini response did not include candidates[0].content.parts[].text: {value}"
                )
            })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn summary_record(
        bucket_start_ms: i64,
        bucket_seconds: i64,
        model: &str,
        thinking_level: &str,
        summary: &str,
        alert_ids: Vec<String>,
        similar_window_ids: Vec<String>,
        token_usage: Value,
    ) -> AgentSummaryRecord {
        AgentSummaryRecord {
            bucket_start_ms,
            bucket_seconds,
            model: model.to_string(),
            thinking_level: thinking_level.to_string(),
            summary: summary.to_string(),
            alert_ids,
            similar_window_ids,
            token_usage,
        }
    }

    pub async fn request_gemini_summary(
        client: &reqwest::Client,
        config: &GeminiSummaryConfig,
        prompt: &str,
    ) -> anyhow::Result<String> {
        let request = build_gemini_request(config, prompt).map_err(anyhow::Error::msg)?;
        let mut builder = client.post(&request.url).json(&request.body);
        match request.auth {
            Some(GeminiAuth::ApiKey(api_key)) => {
                builder = builder.header("x-goog-api-key", api_key);
            }
            Some(GeminiAuth::BearerToken(access_token)) => {
                builder = builder.bearer_auth(access_token);
            }
            None => {
                let access_token = metadata_access_token(client).await?;
                builder = builder.bearer_auth(access_token);
            }
        }

        let response = builder
            .send()
            .await
            .context("send Gemini generateContent request")?;
        let status = response.status();
        let body = response
            .json::<Value>()
            .await
            .context("decode Gemini generateContent response")?;
        if !status.is_success() {
            return Err(anyhow!(
                "Gemini generateContent failed with {status}: {body}"
            ));
        }
        parse_gemini_text(&body).map_err(anyhow::Error::msg)
    }

    pub async fn persist_agent_summary_or_fallback(
        store: Option<&crate::mongo_store::MongoStore>,
        summary: &AgentSummaryRecord,
        fallback_path: &Path,
    ) -> anyhow::Result<()> {
        let Some(store) = store else {
            return crate::live_collector::append_ndjson_fallback(
                fallback_path,
                "agent_summary",
                summary,
            );
        };

        if let Err(error) = store.replace_agent_summary(summary).await {
            tracing::warn!(
                ?error,
                "agent summary MongoDB write failed; using NDJSON fallback"
            );
            crate::live_collector::append_ndjson_fallback(fallback_path, "agent_summary", summary)?;
        }

        Ok(())
    }

    pub async fn run_gemini_summary_scheduler(
        config: GeminiSummaryConfig,
        store: Option<crate::mongo_store::MongoStore>,
        fallback_path: impl AsRef<Path>,
        call_budget: GeminiCallBudget,
    ) -> anyhow::Result<()> {
        if !config.throttle.enabled {
            return Ok(());
        }
        if config.provider == GeminiProvider::DeveloperApi && config.api_key.is_none() {
            tracing::warn!(
                "Developer API Gemini summaries enabled without GEMINI_API_KEY; scheduler not started"
            );
            return Ok(());
        }

        let client = reqwest::Client::new();
        let fallback_path = fallback_path.as_ref().to_path_buf();
        let bucket_seconds = (config.throttle.summary_interval_minutes * 60) as i64;
        let mut last_summary_at_ms = Some(unix_timestamp_ms());
        let mut interval = tokio::time::interval(Duration::from_secs(60));

        loop {
            interval.tick().await;
            let now_ms = unix_timestamp_ms();
            if !call_budget.reserve_summary_call(&config.throttle, now_ms, last_summary_at_ms) {
                continue;
            }

            let state = scheduler_snapshot_state(now_ms);
            let prompt = build_summary_prompt(&state, 0);
            match request_gemini_summary(&client, &config, &prompt).await {
                Ok(summary) => {
                    let record = summary_record(
                        now_ms - (now_ms % (bucket_seconds * 1_000)),
                        bucket_seconds,
                        &config.model,
                        &config.thinking_level,
                        &summary,
                        Vec::new(),
                        Vec::new(),
                        json!({ "estimated": true }),
                    );
                    persist_agent_summary_or_fallback(store.as_ref(), &record, &fallback_path)
                        .await?;
                    last_summary_at_ms = Some(now_ms);
                }
                Err(error) => {
                    tracing::warn!(?error, "Gemini summary request failed");
                }
            }
        }
    }

    fn scheduler_snapshot_state(now_ms: i64) -> RegimeStateRecord {
        RegimeStateRecord {
            id: "dashboard-snapshot".to_string(),
            regime: "EARLY_RISK".to_string(),
            confidence: 0.72,
            updated_at_ms: now_ms,
            previous_regime: "WATCH".to_string(),
            indicators: json!({
                "source": "scheduler_snapshot",
                "fair_gap": 0.03,
                "ofi_1s": 0.42
            }),
            market_resolved: false,
        }
    }

    fn unix_timestamp_ms() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64
    }
}

pub mod live_collector {
    use anyhow::{Context, anyhow};
    use futures_util::{SinkExt, StreamExt, future::join_all};
    use regime_core::{MarketTickMeta, MarketTickRecord, RegimeStateRecord};
    use serde::Serialize;
    use serde_json::{Value, json};
    use std::collections::BTreeSet;
    use std::fs::OpenOptions;
    use std::io::{BufRead, BufReader, Write};
    use std::path::{Path, PathBuf};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    use tokio::net::TcpStream;
    use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};

    pub const DEFAULT_MARKET_WS_URL: &str = "wss://ws-subscriptions-clob.polymarket.com/ws/market";
    pub const DEFAULT_REFERENCE_WS_URL: &str = "wss://ws-live-data.polymarket.com";
    pub const DEFAULT_REFERENCE_SYMBOL: &str = "btc/usd";
    pub const DEFAULT_STALE_AFTER_MS: i64 = 1_500;
    pub const AUTO_MARKET_SLUG: &str = "auto";
    pub const DEFAULT_GAMMA_API_BASE_URL: &str = "https://gamma-api.polymarket.com";
    pub const DEFAULT_CLOB_API_BASE_URL: &str = "https://clob.polymarket.com";
    pub const DEFAULT_WINDOW_STEP_SECONDS: i64 = 300;
    const AUTO_DISCOVERY_SETTLE_SECONDS: i64 = 5;

    #[derive(Debug, Clone, PartialEq)]
    pub struct LiveCollectorConfig {
        pub enabled: bool,
        pub auto_discovery: bool,
        pub slug: String,
        pub series: String,
        pub asset_ids: Vec<String>,
        pub outcomes: Vec<String>,
        pub market_ws_url: String,
        pub reference_ws_url: String,
        pub reference_symbol: String,
        pub ndjson_path: PathBuf,
        pub stale_after_ms: i64,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct LiveMarketMeta {
        pub slug: String,
        pub series: String,
        pub source: String,
        asset_outcomes: Vec<(String, String)>,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct DiscoveredLiveMarket {
        pub slug: String,
        pub series: String,
        pub asset_ids: Vec<String>,
        pub outcomes: Vec<String>,
        pub window_start_s: i64,
        pub window_end_s: i64,
    }

    #[derive(Debug, Clone, PartialEq, Serialize)]
    pub struct LiveSmokeReport {
        pub slug: String,
        pub duration_seconds: u64,
        pub ndjson_path: String,
        pub ndjson_bytes: u64,
        pub market_ticks: usize,
        pub reference_ticks: usize,
        pub stale_states: usize,
        pub outcomes: Vec<String>,
        pub first_tick_timestamp_ms: Option<i64>,
        pub last_tick_timestamp_ms: Option<i64>,
        pub passed: bool,
    }

    impl LiveCollectorConfig {
        #[allow(clippy::too_many_arguments)]
        pub fn from_env_values(
            enabled: Option<&str>,
            slug: Option<&str>,
            series: Option<&str>,
            asset_ids: Option<&str>,
            outcomes: Option<&str>,
            market_ws_url: Option<&str>,
            ndjson_path: Option<&str>,
            stale_after_ms: Option<&str>,
        ) -> Result<Self, String> {
            let enabled = parse_bool(enabled.unwrap_or("false"))?;
            let stale_after_ms = stale_after_ms
                .unwrap_or("1500")
                .parse::<i64>()
                .map_err(|_| "LIVE_COLLECTOR_STALE_AFTER_MS must be an integer".to_string())?;
            if stale_after_ms <= 0 {
                return Err("LIVE_COLLECTOR_STALE_AFTER_MS must be greater than 0".to_string());
            }
            if !enabled {
                return Ok(Self {
                    enabled,
                    auto_discovery: false,
                    slug: "disabled".to_string(),
                    series: "disabled".to_string(),
                    asset_ids: Vec::new(),
                    outcomes: Vec::new(),
                    market_ws_url: market_ws_url.unwrap_or(DEFAULT_MARKET_WS_URL).to_string(),
                    reference_ws_url: DEFAULT_REFERENCE_WS_URL.to_string(),
                    reference_symbol: DEFAULT_REFERENCE_SYMBOL.to_string(),
                    ndjson_path: PathBuf::from(ndjson_path.unwrap_or("data/live-fallback.ndjson")),
                    stale_after_ms,
                });
            }

            let slug = required_string(slug, "LIVE_MARKET_SLUG")?;
            let series = required_string(series, "LIVE_MARKET_SERIES")?;
            if slug == AUTO_MARKET_SLUG {
                return Ok(Self {
                    enabled,
                    auto_discovery: true,
                    slug: slug.to_string(),
                    series: series.to_string(),
                    asset_ids: Vec::new(),
                    outcomes: Vec::new(),
                    market_ws_url: market_ws_url.unwrap_or(DEFAULT_MARKET_WS_URL).to_string(),
                    reference_ws_url: DEFAULT_REFERENCE_WS_URL.to_string(),
                    reference_symbol: DEFAULT_REFERENCE_SYMBOL.to_string(),
                    ndjson_path: PathBuf::from(ndjson_path.unwrap_or("data/live-fallback.ndjson")),
                    stale_after_ms,
                });
            }

            let asset_ids = parse_csv(required_string(asset_ids, "POLYMARKET_ASSET_IDS")?);
            if asset_ids.is_empty() {
                return Err("POLYMARKET_ASSET_IDS must include at least one asset id".to_string());
            }

            let outcomes = outcomes.map(parse_csv).unwrap_or_else(|| {
                (0..asset_ids.len())
                    .map(|index| format!("OUTCOME_{index}"))
                    .collect()
            });
            if outcomes.len() != asset_ids.len() {
                return Err(
                    "POLYMARKET_OUTCOMES must match POLYMARKET_ASSET_IDS length".to_string()
                );
            }

            Ok(Self {
                enabled,
                auto_discovery: false,
                slug: slug.to_string(),
                series: series.to_string(),
                asset_ids,
                outcomes,
                market_ws_url: market_ws_url.unwrap_or(DEFAULT_MARKET_WS_URL).to_string(),
                reference_ws_url: DEFAULT_REFERENCE_WS_URL.to_string(),
                reference_symbol: DEFAULT_REFERENCE_SYMBOL.to_string(),
                ndjson_path: PathBuf::from(ndjson_path.unwrap_or("data/live-fallback.ndjson")),
                stale_after_ms,
            })
        }

        pub fn from_env() -> Result<Self, String> {
            let mut config = Self::from_env_values(
                std::env::var("LIVE_COLLECTOR_ENABLED").ok().as_deref(),
                std::env::var("LIVE_MARKET_SLUG").ok().as_deref(),
                std::env::var("LIVE_MARKET_SERIES").ok().as_deref(),
                std::env::var("POLYMARKET_ASSET_IDS").ok().as_deref(),
                std::env::var("POLYMARKET_OUTCOMES").ok().as_deref(),
                std::env::var("POLYMARKET_MARKET_WS_URL").ok().as_deref(),
                std::env::var("LIVE_COLLECTOR_NDJSON_PATH").ok().as_deref(),
                std::env::var("LIVE_COLLECTOR_STALE_AFTER_MS")
                    .ok()
                    .as_deref(),
            )?;
            config.reference_ws_url = std::env::var("REFERENCE_PRICE_WS_URL")
                .unwrap_or_else(|_| DEFAULT_REFERENCE_WS_URL.to_string());
            config.reference_symbol = std::env::var("REFERENCE_PRICE_SYMBOL")
                .unwrap_or_else(|_| DEFAULT_REFERENCE_SYMBOL.to_string());
            Ok(config)
        }

        pub fn subscription_message(&self) -> Value {
            json!({
                "assets_ids": self.asset_ids,
                "type": "market",
                "custom_feature_enabled": true
            })
        }

        pub fn market_meta(&self) -> LiveMarketMeta {
            LiveMarketMeta {
                slug: self.slug.clone(),
                series: self.series.clone(),
                source: "clob".to_string(),
                asset_outcomes: self
                    .asset_ids
                    .iter()
                    .cloned()
                    .zip(self.outcomes.iter().cloned())
                    .collect(),
            }
        }

        pub fn reference_subscription_message(&self) -> Value {
            json!({
                "action": "subscribe",
                "subscriptions": [{
                    "topic": "crypto_prices_chainlink",
                    "type": "*",
                    "filters": json!({ "symbol": self.reference_symbol }).to_string()
                }]
            })
        }

        pub fn ndjson_path_for_role(&self, role: &str) -> PathBuf {
            let extension = self
                .ndjson_path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("ndjson");
            self.ndjson_path
                .with_extension(format!("{role}.{extension}"))
        }

        pub fn with_ndjson_path(mut self, ndjson_path: PathBuf) -> Self {
            self.ndjson_path = ndjson_path;
            self
        }

        pub fn with_discovered_market(mut self, market: &DiscoveredLiveMarket) -> Self {
            self.auto_discovery = false;
            self.slug = market.slug.clone();
            self.series = market.series.clone();
            self.asset_ids = market.asset_ids.clone();
            self.outcomes = market.outcomes.clone();
            self
        }
    }

    pub fn target_window_start_seconds(now_s: i64, step_s: i64) -> i64 {
        if step_s <= 0 {
            return now_s;
        }

        now_s - now_s.rem_euclid(step_s)
    }

    pub fn parse_gamma_event_market(
        slug: &str,
        series: &str,
        value: &Value,
        window_start_s: i64,
        step_s: i64,
    ) -> Result<DiscoveredLiveMarket, String> {
        let markets = value
            .get("markets")
            .and_then(Value::as_array)
            .ok_or_else(|| "Gamma event response missing markets".to_string())?;
        let mut last_error = None;

        for market in markets {
            if !bool_field(market, "active", false) || bool_field(market, "closed", false) {
                continue;
            }

            match parse_gamma_market_fields(slug, series, market, window_start_s, step_s) {
                Ok(discovered) => return Ok(discovered),
                Err(error) => last_error = Some(error),
            }
        }

        Err(last_error.unwrap_or_else(|| "Gamma event has no active open CLOB market".to_string()))
    }

    pub async fn discover_live_market(
        config: &LiveCollectorConfig,
        now_s: i64,
    ) -> anyhow::Result<DiscoveredLiveMarket> {
        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (compatible; regime-sentinel-agent/0.1)")
            .timeout(Duration::from_secs(8))
            .build()
            .context("build Gamma API client")?;
        let base_url = std::env::var("GAMMA_API_BASE_URL")
            .unwrap_or_else(|_| DEFAULT_GAMMA_API_BASE_URL.to_string());
        let target_start = target_window_start_seconds(now_s, DEFAULT_WINDOW_STEP_SECONDS);

        let mut last_error = None;
        for window_start_s in [target_start] {
            let slug = format!("{}-{window_start_s}", config.series);
            let url = format!("{}/events/slug/{slug}", base_url.trim_end_matches('/'));
            match fetch_gamma_event(&client, &url).await.and_then(|value| {
                parse_gamma_event_market(
                    &slug,
                    &config.series,
                    &value,
                    window_start_s,
                    DEFAULT_WINDOW_STEP_SECONDS,
                )
                .map_err(anyhow::Error::msg)
            }) {
                Ok(market) => return Ok(market),
                Err(error) => last_error = Some(error),
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow!("no active {} market discovered", config.series)))
    }

    async fn fetch_gamma_event(client: &reqwest::Client, url: &str) -> anyhow::Result<Value> {
        let response = client
            .get(url)
            .send()
            .await
            .with_context(|| format!("fetch Gamma event {url}"))?;
        let status = response.status();
        if !status.is_success() {
            return Err(anyhow!("Gamma event request returned {status}"));
        }
        response
            .json::<Value>()
            .await
            .context("parse Gamma event JSON")
    }

    pub fn market_ticks_from_message(
        payload: &str,
        meta: &LiveMarketMeta,
        received_at_ms: i64,
    ) -> Result<Vec<MarketTickRecord>, String> {
        if matches!(payload.trim(), "PING" | "PONG") {
            return Ok(Vec::new());
        }
        let value: Value = serde_json::from_str(payload)
            .map_err(|error| format!("invalid websocket JSON: {error}"))?;
        market_ticks_from_value(&value, meta, received_at_ms)
    }

    pub fn reference_tick_from_chainlink_message(
        payload: &str,
        config: &LiveCollectorConfig,
        received_at_ms: i64,
    ) -> Result<Option<MarketTickRecord>, String> {
        if matches!(payload.trim(), "PING" | "PONG") {
            return Ok(None);
        }
        let value: Value = serde_json::from_str(payload)
            .map_err(|error| format!("invalid reference JSON: {error}"))?;
        if value.get("topic").and_then(Value::as_str) != Some("crypto_prices_chainlink")
            || value.get("type").and_then(Value::as_str) != Some("update")
        {
            return Ok(None);
        }
        let payload = value
            .get("payload")
            .ok_or_else(|| "Chainlink reference message missing payload".to_string())?;
        let symbol = string_field(payload, "symbol")?;
        if symbol != config.reference_symbol {
            return Ok(None);
        }
        let timestamp_ms = timestamp_ms(payload)
            .or_else(|| timestamp_ms(&value))
            .unwrap_or(received_at_ms);

        Ok(Some(MarketTickRecord {
            timestamp_ms,
            meta: MarketTickMeta {
                slug: config.slug.clone(),
                series: config.series.clone(),
                source: "chainlink".to_string(),
            },
            price: f64_field(payload, "value")?,
            size: 0.0,
            side: "ORACLE".to_string(),
            outcome: symbol.to_string(),
            receive_lag_ms: (received_at_ms - timestamp_ms).max(0),
        }))
    }

    pub fn stale_data_penalty(
        last_event_timestamp_ms: Option<i64>,
        now_ms: i64,
        stale_after_ms: i64,
    ) -> f64 {
        match last_event_timestamp_ms {
            Some(last_event_timestamp_ms) if now_ms - last_event_timestamp_ms <= stale_after_ms => {
                0.0
            }
            _ => 1.0,
        }
    }

    pub fn stale_regime_state(
        config: &LiveCollectorConfig,
        last_event_timestamp_ms: Option<i64>,
        now_ms: i64,
    ) -> Option<RegimeStateRecord> {
        let stale_data_penalty =
            stale_data_penalty(last_event_timestamp_ms, now_ms, config.stale_after_ms);
        if stale_data_penalty == 0.0 {
            return None;
        }

        Some(RegimeStateRecord {
            id: config.slug.clone(),
            regime: "STALE_DATA".to_string(),
            confidence: 0.0,
            updated_at_ms: now_ms,
            previous_regime: "UNKNOWN".to_string(),
            indicators: json!({
                "stale_data_penalty": stale_data_penalty,
                "last_event_timestamp_ms": last_event_timestamp_ms,
                "stale_after_ms": config.stale_after_ms
            }),
            market_resolved: false,
        })
    }

    pub async fn persist_market_tick_or_fallback(
        store: Option<&crate::mongo_store::MongoStore>,
        tick: &MarketTickRecord,
        fallback_path: &Path,
    ) -> anyhow::Result<()> {
        append_ndjson_fallback(fallback_path, "market_tick", tick)?;

        if let Some(store) = store {
            match tokio::time::timeout(Duration::from_millis(500), store.insert_market_tick(tick))
                .await
            {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    tracing::warn!(
                        ?error,
                        "market tick MongoDB write failed after NDJSON append"
                    );
                }
                Err(error) => {
                    tracing::warn!(
                        ?error,
                        "market tick MongoDB write timed out after NDJSON append"
                    );
                }
            }
        }

        Ok(())
    }

    pub async fn persist_regime_state_or_fallback(
        store: Option<&crate::mongo_store::MongoStore>,
        state: &RegimeStateRecord,
        fallback_path: &Path,
    ) -> anyhow::Result<()> {
        append_ndjson_fallback(fallback_path, "regime_state", state)?;

        if let Some(store) = store {
            match tokio::time::timeout(Duration::from_millis(500), store.upsert_regime_state(state))
                .await
            {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    tracing::warn!(
                        ?error,
                        "regime state MongoDB write failed after NDJSON append"
                    );
                }
                Err(error) => {
                    tracing::warn!(
                        ?error,
                        "regime state MongoDB write timed out after NDJSON append"
                    );
                }
            }
        }

        Ok(())
    }

    pub async fn run_live_collector(
        config: LiveCollectorConfig,
        store: Option<crate::mongo_store::MongoStore>,
    ) -> anyhow::Result<()> {
        run_live_collector_with_deadline(config, store, None).await
    }

    async fn run_live_collector_with_deadline(
        config: LiveCollectorConfig,
        store: Option<crate::mongo_store::MongoStore>,
        deadline_s: Option<i64>,
    ) -> anyhow::Result<()> {
        if !config.enabled {
            return Ok(());
        }

        let meta = config.market_meta();
        let mut last_event_timestamp_ms = None;
        let midpoint_client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (compatible; regime-sentinel-agent/0.1)")
            .timeout(Duration::from_secs(2))
            .build()
            .context("build CLOB midpoint client")?;
        let mut midpoint_poll = tokio::time::interval(Duration::from_secs(1));
        midpoint_poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            if deadline_reached(deadline_s) {
                return Ok(());
            }
            poll_midpoint_snapshot(
                &config,
                &meta,
                &midpoint_client,
                store.as_ref(),
                &mut last_event_timestamp_ms,
            )
            .await;

            let ws_stream = loop {
                let connect = connect_live_websocket(&config.market_ws_url);
                tokio::pin!(connect);

                let connect_result = loop {
                    tokio::select! {
                        _ = midpoint_poll.tick() => {
                            if deadline_reached(deadline_s) {
                                return Ok(());
                            }
                            poll_midpoint_snapshot(
                                &config,
                                &meta,
                                &midpoint_client,
                                store.as_ref(),
                                &mut last_event_timestamp_ms,
                            )
                            .await;
                        }
                        result = &mut connect => break result,
                    }
                };

                match connect_result {
                    Ok((ws_stream, _)) => break ws_stream,
                    Err(error) => {
                        tracing::warn!(?error, "connect Polymarket market websocket failed");
                        let reconnect_delay = tokio::time::sleep(Duration::from_secs(3));
                        tokio::pin!(reconnect_delay);
                        loop {
                            tokio::select! {
                                _ = midpoint_poll.tick() => {
                                    if deadline_reached(deadline_s) {
                                        return Ok(());
                                    }
                                    poll_midpoint_snapshot(
                                        &config,
                                        &meta,
                                        &midpoint_client,
                                        store.as_ref(),
                                        &mut last_event_timestamp_ms,
                                    )
                                    .await;
                                }
                                _ = &mut reconnect_delay => break,
                            }
                        }
                    }
                }
            };

            let (mut write, mut read) = ws_stream.split();
            match tokio::time::timeout(
                Duration::from_secs(1),
                write.send(Message::Text(
                    config.subscription_message().to_string().into(),
                )),
            )
            .await
            {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    tracing::warn!(?error, "send Polymarket market subscription failed");
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    continue;
                }
                Err(error) => {
                    tracing::warn!(?error, "send Polymarket market subscription timed out");
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    continue;
                }
            }
            let mut heartbeat = tokio::time::interval(Duration::from_secs(10));

            loop {
                tokio::select! {
                    _ = midpoint_poll.tick() => {
                        if deadline_reached(deadline_s) {
                            return Ok(());
                        }
                        match fetch_midpoint_ticks(&config, &meta, &midpoint_client).await {
                            Ok(ticks) => {
                                persist_market_ticks(ticks, store.as_ref(), &config.ndjson_path, &mut last_event_timestamp_ms).await;
                            }
                            Err(error) => {
                                tracing::debug!(?error, "poll CLOB midpoint snapshot failed");
                            }
                        }
                    }
                    _ = heartbeat.tick() => {
                        if deadline_reached(deadline_s) {
                            return Ok(());
                        }
                        match tokio::time::timeout(
                            Duration::from_secs(1),
                            write.send(Message::Text("PING".into())),
                        )
                        .await
                        {
                            Ok(Ok(())) => {}
                            Ok(Err(error)) => {
                                tracing::warn!(?error, "send Polymarket market heartbeat failed");
                                break;
                            }
                            Err(error) => {
                                tracing::warn!(?error, "send Polymarket market heartbeat timed out");
                                break;
                            }
                        }
                        if let Some(state) = stale_regime_state(&config, last_event_timestamp_ms, unix_timestamp_ms()) {
                            persist_regime_state_or_fallback(store.as_ref(), &state, &config.ndjson_path).await?;
                        }
                    }
                    message = read.next() => {
                        match message {
                            Some(Ok(Message::Text(text))) => {
                                handle_market_message(text.as_ref(), &meta, store.as_ref(), &config.ndjson_path, &mut last_event_timestamp_ms).await;
                            }
                            Some(Ok(Message::Binary(bytes))) => {
                                if let Ok(text) = std::str::from_utf8(&bytes) {
                                    handle_market_message(text, &meta, store.as_ref(), &config.ndjson_path, &mut last_event_timestamp_ms).await;
                                }
                            }
                            Some(Ok(Message::Ping(payload))) => {
                                write.send(Message::Pong(payload)).await.context("send websocket pong")?;
                            }
                            Some(Ok(Message::Close(close))) => {
                                tracing::warn!(?close, "Polymarket market websocket closed");
                                break;
                            }
                            Some(Ok(_)) => {}
                            Some(Err(error)) => {
                                tracing::warn!(?error, "Polymarket market websocket read failed");
                                break;
                            }
                            None => break,
                        }
                    }
                }
            }

            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    }

    async fn fetch_midpoint_ticks(
        config: &LiveCollectorConfig,
        meta: &LiveMarketMeta,
        client: &reqwest::Client,
    ) -> anyhow::Result<Vec<MarketTickRecord>> {
        if config.asset_ids.is_empty() {
            return Ok(Vec::new());
        }
        let base_url = std::env::var("CLOB_API_BASE_URL")
            .unwrap_or_else(|_| DEFAULT_CLOB_API_BASE_URL.to_string());
        let requests = config.asset_ids.iter().map(|asset_id| {
            let url = format!(
                "{}/midpoint?token_id={}",
                base_url.trim_end_matches('/'),
                asset_id
            );
            async move { fetch_single_midpoint_tick(client, &url, meta, asset_id).await }
        });

        let mut ticks = Vec::new();
        let mut last_error = None;
        for result in join_all(requests).await {
            match result {
                Ok(tick) => ticks.push(tick),
                Err(error) => last_error = Some(error),
            }
        }

        if let Some(error) = last_error {
            if ticks.is_empty() {
                return Err(error);
            }
            tracing::debug!(
                ?error,
                "partial CLOB midpoint snapshot failed while other tokens succeeded"
            );
        }
        Ok(ticks)
    }

    async fn poll_midpoint_snapshot(
        config: &LiveCollectorConfig,
        meta: &LiveMarketMeta,
        client: &reqwest::Client,
        store: Option<&crate::mongo_store::MongoStore>,
        last_event_timestamp_ms: &mut Option<i64>,
    ) {
        match fetch_midpoint_ticks(config, meta, client).await {
            Ok(ticks) => {
                persist_market_ticks(ticks, store, &config.ndjson_path, last_event_timestamp_ms)
                    .await;
            }
            Err(error) => {
                tracing::warn!(?error, "poll CLOB midpoint snapshot failed");
            }
        }
    }

    async fn fetch_single_midpoint_tick(
        client: &reqwest::Client,
        url: &str,
        meta: &LiveMarketMeta,
        asset_id: &str,
    ) -> anyhow::Result<MarketTickRecord> {
        let value = client
            .get(url)
            .send()
            .await
            .with_context(|| format!("request CLOB midpoint {url}"))?
            .error_for_status()
            .with_context(|| format!("CLOB midpoint response status {url}"))?
            .json::<Value>()
            .await
            .with_context(|| format!("decode CLOB midpoint response {url}"))?;

        midpoint_tick_from_midpoint_response(&value, meta, asset_id, unix_timestamp_ms())
            .map_err(anyhow::Error::msg)
    }

    pub async fn run_reference_price_collector(
        config: LiveCollectorConfig,
        store: Option<crate::mongo_store::MongoStore>,
    ) -> anyhow::Result<()> {
        run_reference_price_collector_with_deadline(config, store, None).await
    }

    async fn run_reference_price_collector_with_deadline(
        config: LiveCollectorConfig,
        store: Option<crate::mongo_store::MongoStore>,
        deadline_s: Option<i64>,
    ) -> anyhow::Result<()> {
        if !config.enabled {
            return Ok(());
        }

        loop {
            if deadline_reached(deadline_s) {
                return Ok(());
            }
            let ws_stream = match tokio::time::timeout(
                Duration::from_secs(8),
                connect_live_websocket(&config.reference_ws_url),
            )
            .await
            {
                Ok(Ok((ws_stream, _))) => ws_stream,
                Ok(Err(error)) => {
                    tracing::warn!(?error, "connect reference price websocket failed");
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    continue;
                }
                Err(error) => {
                    tracing::warn!(?error, "connect reference price websocket failed");
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    continue;
                }
            };

            let (mut write, mut read) = ws_stream.split();
            match tokio::time::timeout(
                Duration::from_secs(1),
                write.send(Message::Text(
                    config.reference_subscription_message().to_string().into(),
                )),
            )
            .await
            {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    tracing::warn!(?error, "send reference price subscription failed");
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    continue;
                }
                Err(error) => {
                    tracing::warn!(?error, "send reference price subscription timed out");
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    continue;
                }
            }
            let mut heartbeat = tokio::time::interval(Duration::from_secs(5));

            loop {
                tokio::select! {
                    _ = heartbeat.tick() => {
                        if deadline_reached(deadline_s) {
                            return Ok(());
                        }
                        match tokio::time::timeout(
                            Duration::from_secs(1),
                            write.send(Message::Text("PING".into())),
                        )
                        .await
                        {
                            Ok(Ok(())) => {}
                            Ok(Err(error)) => {
                                tracing::warn!(?error, "send reference price heartbeat failed");
                                break;
                            }
                            Err(error) => {
                                tracing::warn!(?error, "send reference price heartbeat timed out");
                                break;
                            }
                        }
                    }
                    message = read.next() => {
                        match message {
                            Some(Ok(Message::Text(text))) => {
                                handle_reference_message(
                                    text.as_ref(),
                                    &config,
                                    store.as_ref(),
                                    &config.ndjson_path,
                                )
                                .await;
                            }
                            Some(Ok(Message::Binary(bytes))) => {
                                if let Ok(text) = std::str::from_utf8(&bytes) {
                                    handle_reference_message(
                                        text,
                                        &config,
                                        store.as_ref(),
                                        &config.ndjson_path,
                                    )
                                    .await;
                                }
                            }
                            Some(Ok(Message::Ping(payload))) => {
                                write
                                    .send(Message::Pong(payload))
                                    .await
                                    .context("send reference websocket pong")?;
                            }
                            Some(Ok(Message::Close(close))) => {
                                tracing::warn!(?close, "reference price websocket closed");
                                break;
                            }
                            Some(Ok(_)) => {}
                            Some(Err(error)) => {
                                tracing::warn!(?error, "reference price websocket read failed");
                                break;
                            }
                            None => break,
                        }
                    }
                }
            }

            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    }

    pub async fn run_auto_rotating_live_collectors(
        config: LiveCollectorConfig,
        store: Option<crate::mongo_store::MongoStore>,
    ) -> anyhow::Result<()> {
        if !config.enabled {
            return Ok(());
        }

        loop {
            let now_s = unix_timestamp_ms() / 1_000;
            let market = match discover_live_market(&config, now_s).await {
                Ok(market) => market,
                Err(error) => {
                    tracing::warn!(?error, "auto market discovery failed");
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    continue;
                }
            };
            let deadline_s = market.window_end_s + AUTO_DISCOVERY_SETTLE_SECONDS;
            tracing::info!(
                slug = %market.slug,
                deadline_s,
                "starting auto-discovered live collectors"
            );

            let market_config = config
                .clone()
                .with_discovered_market(&market)
                .with_ndjson_path(config.ndjson_path_for_role("market"));
            let reference_config = config
                .clone()
                .with_discovered_market(&market)
                .with_ndjson_path(config.ndjson_path_for_role("reference"));
            let market_store = store.clone();
            let reference_store = store.clone();
            let (market_result, reference_result) = tokio::join!(
                run_live_collector_with_deadline(market_config, market_store, Some(deadline_s)),
                run_reference_price_collector_with_deadline(
                    reference_config,
                    reference_store,
                    Some(deadline_s)
                )
            );

            if let Err(error) = market_result {
                tracing::warn!(?error, "auto market collector window stopped with error");
            }
            if let Err(error) = reference_result {
                tracing::warn!(?error, "auto reference collector window stopped with error");
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    async fn connect_live_websocket(
        url: &str,
    ) -> tokio_tungstenite::tungstenite::Result<(
        WebSocketStream<MaybeTlsStream<TcpStream>>,
        tokio_tungstenite::tungstenite::handshake::client::Response,
    )> {
        connect_async(url).await
    }

    pub fn append_ndjson_fallback<T: Serialize>(
        path: impl AsRef<Path>,
        kind: &str,
        record: &T,
    ) -> anyhow::Result<()> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent).context("create NDJSON fallback directory")?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .context("open NDJSON fallback")?;
        serde_json::to_writer(
            &mut file,
            &json!({
                "kind": kind,
                "record": record
            }),
        )
        .context("serialize NDJSON fallback")?;
        writeln!(file).context("write NDJSON newline")?;
        Ok(())
    }

    pub fn summarize_live_smoke_ndjson(
        slug: &str,
        duration_seconds: u64,
        path: impl AsRef<Path>,
    ) -> anyhow::Result<LiveSmokeReport> {
        let path = path.as_ref();
        let file = std::fs::File::open(path).context("open live smoke NDJSON")?;
        let reader = BufReader::new(file);
        let mut market_ticks = 0;
        let mut reference_ticks = 0;
        let mut stale_states = 0;
        let mut outcomes = BTreeSet::new();
        let mut first_tick_timestamp_ms = None::<i64>;
        let mut last_tick_timestamp_ms = None::<i64>;

        for line in reader.lines() {
            let line = line.context("read live smoke NDJSON line")?;
            if line.trim().is_empty() {
                continue;
            }
            let value: Value =
                serde_json::from_str(&line).context("parse live smoke NDJSON line")?;
            let kind = value.get("kind").and_then(Value::as_str);
            if kind == Some("regime_state") {
                stale_states += 1;
                continue;
            }
            if kind != Some("market_tick") {
                continue;
            }

            let record = &value["record"];
            if record["meta"]["slug"].as_str() != Some(slug) {
                continue;
            }

            match record["meta"]["source"].as_str() {
                Some("clob") => market_ticks += 1,
                Some("chainlink") => reference_ticks += 1,
                _ => {}
            }
            if let Some(outcome) = record["outcome"].as_str() {
                outcomes.insert(outcome.to_string());
            }
            if let Some(timestamp_ms) = record["timestamp_ms"].as_i64() {
                first_tick_timestamp_ms = Some(
                    first_tick_timestamp_ms
                        .map_or(timestamp_ms, |current| current.min(timestamp_ms)),
                );
                last_tick_timestamp_ms = Some(
                    last_tick_timestamp_ms
                        .map_or(timestamp_ms, |current| current.max(timestamp_ms)),
                );
            }
        }

        let outcomes: Vec<String> = outcomes.into_iter().collect();
        let passed = live_smoke_passed(market_ticks, reference_ticks, &outcomes, &[]);
        let ndjson_bytes = std::fs::metadata(path)
            .map(|metadata| metadata.len())
            .unwrap_or(0);

        Ok(LiveSmokeReport {
            slug: slug.to_string(),
            duration_seconds,
            ndjson_path: path.display().to_string(),
            ndjson_bytes,
            market_ticks,
            reference_ticks,
            stale_states,
            outcomes,
            first_tick_timestamp_ms,
            last_tick_timestamp_ms,
            passed,
        })
    }

    pub fn live_smoke_passed(
        market_ticks: usize,
        reference_ticks: usize,
        outcomes: &[String],
        required_outcomes: &[&str],
    ) -> bool {
        market_ticks > 0
            && reference_ticks > 0
            && required_outcomes
                .iter()
                .all(|required| outcomes.iter().any(|outcome| outcome == required))
    }

    async fn handle_market_message(
        payload: &str,
        meta: &LiveMarketMeta,
        store: Option<&crate::mongo_store::MongoStore>,
        fallback_path: &Path,
        last_event_timestamp_ms: &mut Option<i64>,
    ) {
        match market_ticks_from_message(payload, meta, unix_timestamp_ms()) {
            Ok(ticks) => {
                persist_market_ticks(ticks, store, fallback_path, last_event_timestamp_ms).await;
            }
            Err(error) => {
                tracing::warn!(%error, "parse Polymarket market websocket message failed");
            }
        }
    }

    async fn persist_market_ticks(
        ticks: Vec<MarketTickRecord>,
        store: Option<&crate::mongo_store::MongoStore>,
        fallback_path: &Path,
        last_event_timestamp_ms: &mut Option<i64>,
    ) {
        for tick in ticks {
            *last_event_timestamp_ms = Some(tick.timestamp_ms);
            if let Err(error) = persist_market_tick_or_fallback(store, &tick, fallback_path).await {
                tracing::warn!(?error, "persist market tick failed");
            }
        }
    }

    async fn handle_reference_message(
        payload: &str,
        config: &LiveCollectorConfig,
        store: Option<&crate::mongo_store::MongoStore>,
        fallback_path: &Path,
    ) {
        match reference_tick_from_chainlink_message(payload, config, unix_timestamp_ms()) {
            Ok(Some(tick)) => {
                if let Err(error) =
                    persist_market_tick_or_fallback(store, &tick, fallback_path).await
                {
                    tracing::warn!(?error, "persist reference price tick failed");
                }
            }
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(%error, "parse reference price websocket message failed");
            }
        }
    }

    fn market_ticks_from_value(
        value: &Value,
        meta: &LiveMarketMeta,
        received_at_ms: i64,
    ) -> Result<Vec<MarketTickRecord>, String> {
        if let Some(items) = value.as_array() {
            let mut ticks = Vec::new();
            for item in items {
                ticks.extend(market_ticks_from_value(item, meta, received_at_ms)?);
            }
            return Ok(ticks);
        }

        let event_type = value
            .get("event_type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match event_type {
            "price_change" => price_change_ticks(value, meta, received_at_ms),
            "last_trade_price" => single_price_tick(value, value, meta, received_at_ms),
            "best_bid_ask" => best_bid_ask_tick(value, meta, received_at_ms)
                .map(|tick| tick.into_iter().collect()),
            "book" => book_tick(value, meta, received_at_ms).map(|tick| tick.into_iter().collect()),
            _ => Ok(Vec::new()),
        }
    }

    pub fn midpoint_ticks_from_midpoints_response(
        value: &Value,
        meta: &LiveMarketMeta,
        received_at_ms: i64,
    ) -> Result<Vec<MarketTickRecord>, String> {
        let prices = value
            .as_object()
            .ok_or_else(|| "midpoints response must be an object".to_string())?;
        let mut ticks = Vec::new();

        for (asset_id, outcome) in &meta.asset_outcomes {
            let Some(price_value) = prices.get(asset_id) else {
                continue;
            };
            ticks.push(MarketTickRecord {
                timestamp_ms: received_at_ms,
                meta: MarketTickMeta {
                    slug: meta.slug.clone(),
                    series: meta.series.clone(),
                    source: meta.source.clone(),
                },
                price: f64_value(price_value, "midpoint")?,
                size: 0.0,
                side: "BBA".to_string(),
                outcome: outcome.clone(),
                receive_lag_ms: 0,
            });
        }

        Ok(ticks)
    }

    pub fn midpoint_tick_from_midpoint_response(
        value: &Value,
        meta: &LiveMarketMeta,
        asset_id: &str,
        received_at_ms: i64,
    ) -> Result<MarketTickRecord, String> {
        Ok(MarketTickRecord {
            timestamp_ms: received_at_ms,
            meta: MarketTickMeta {
                slug: meta.slug.clone(),
                series: meta.series.clone(),
                source: meta.source.clone(),
            },
            price: f64_field(value, "mid")?,
            size: 0.0,
            side: "BBA".to_string(),
            outcome: meta.outcome_for_asset(asset_id),
            receive_lag_ms: 0,
        })
    }

    fn price_change_ticks(
        value: &Value,
        meta: &LiveMarketMeta,
        received_at_ms: i64,
    ) -> Result<Vec<MarketTickRecord>, String> {
        let changes = value
            .get("price_changes")
            .and_then(Value::as_array)
            .ok_or_else(|| "price_change missing price_changes".to_string())?;
        let mut ticks = Vec::with_capacity(changes.len());
        for change in changes {
            ticks.extend(single_price_tick(change, value, meta, received_at_ms)?);
        }
        Ok(ticks)
    }

    fn single_price_tick(
        item: &Value,
        parent: &Value,
        meta: &LiveMarketMeta,
        received_at_ms: i64,
    ) -> Result<Vec<MarketTickRecord>, String> {
        let timestamp_ms = timestamp_ms(parent).unwrap_or(received_at_ms);
        let asset_id = string_field(item, "asset_id")?;
        Ok(vec![MarketTickRecord {
            timestamp_ms,
            meta: MarketTickMeta {
                slug: meta.slug.clone(),
                series: meta.series.clone(),
                source: meta.source.clone(),
            },
            price: f64_field(item, "price")?,
            size: f64_field(item, "size")?,
            side: string_field(item, "side")?.to_string(),
            outcome: meta.outcome_for_asset(asset_id),
            receive_lag_ms: (received_at_ms - timestamp_ms).max(0),
        }])
    }

    fn best_bid_ask_tick(
        value: &Value,
        meta: &LiveMarketMeta,
        received_at_ms: i64,
    ) -> Result<Option<MarketTickRecord>, String> {
        let best_bid = f64_field(value, "best_bid")?;
        let best_ask = f64_field(value, "best_ask")?;
        let timestamp_ms = timestamp_ms(value).unwrap_or(received_at_ms);
        let asset_id = string_field(value, "asset_id")?;
        Ok(Some(MarketTickRecord {
            timestamp_ms,
            meta: MarketTickMeta {
                slug: meta.slug.clone(),
                series: meta.series.clone(),
                source: meta.source.clone(),
            },
            price: (best_bid + best_ask) / 2.0,
            size: 0.0,
            side: "BBA".to_string(),
            outcome: meta.outcome_for_asset(asset_id),
            receive_lag_ms: (received_at_ms - timestamp_ms).max(0),
        }))
    }

    fn book_tick(
        value: &Value,
        meta: &LiveMarketMeta,
        received_at_ms: i64,
    ) -> Result<Option<MarketTickRecord>, String> {
        let best_bid = price_levels(value, "bids")?.into_iter().reduce(f64::max);
        let best_ask = price_levels(value, "asks")?.into_iter().reduce(f64::min);
        let (Some(best_bid), Some(best_ask)) = (best_bid, best_ask) else {
            return Ok(None);
        };
        let timestamp_ms = timestamp_ms(value).unwrap_or(received_at_ms);
        let asset_id = string_field(value, "asset_id")?;

        Ok(Some(MarketTickRecord {
            timestamp_ms,
            meta: MarketTickMeta {
                slug: meta.slug.clone(),
                series: meta.series.clone(),
                source: meta.source.clone(),
            },
            price: (best_bid + best_ask) / 2.0,
            size: 0.0,
            side: "BOOK".to_string(),
            outcome: meta.outcome_for_asset(asset_id),
            receive_lag_ms: (received_at_ms - timestamp_ms).max(0),
        }))
    }

    fn price_levels(value: &Value, field: &str) -> Result<Vec<f64>, String> {
        let levels = value
            .get(field)
            .and_then(Value::as_array)
            .ok_or_else(|| format!("book missing {field}"))?;
        levels
            .iter()
            .map(|level| f64_field(level, "price"))
            .collect()
    }

    fn timestamp_ms(value: &Value) -> Option<i64> {
        value
            .get("timestamp")
            .and_then(|timestamp| match timestamp {
                Value::String(raw) => raw.parse::<i64>().ok(),
                Value::Number(raw) => raw.as_i64(),
                _ => None,
            })
    }

    fn string_field<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
        value
            .get(field)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("missing string field {field}"))
    }

    fn f64_field(value: &Value, field: &str) -> Result<f64, String> {
        match value.get(field) {
            Some(raw) => f64_value(raw, field),
            _ => Err(format!("missing float field {field}")),
        }
    }

    fn f64_value(value: &Value, field: &str) -> Result<f64, String> {
        match value {
            Value::String(raw) => raw
                .parse::<f64>()
                .map_err(|_| format!("invalid float field {field}")),
            Value::Number(raw) => raw
                .as_f64()
                .ok_or_else(|| format!("invalid float field {field}")),
            _ => Err(format!("missing float field {field}")),
        }
    }

    fn parse_bool(value: &str) -> Result<bool, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(true),
            "0" | "false" | "no" | "off" => Ok(false),
            _ => Err("LIVE_COLLECTOR_ENABLED must be a boolean".to_string()),
        }
    }

    fn parse_gamma_market_fields(
        slug: &str,
        series: &str,
        market: &Value,
        window_start_s: i64,
        step_s: i64,
    ) -> Result<DiscoveredLiveMarket, String> {
        let asset_ids = string_array_field(market, "clobTokenIds")?;
        if asset_ids.len() < 2 {
            return Err("Gamma market must include at least two clobTokenIds".to_string());
        }

        let outcomes: Vec<String> = match string_array_field(market, "outcomes") {
            Ok(values) if !values.is_empty() => values
                .into_iter()
                .map(|value| value.trim().to_ascii_uppercase())
                .collect(),
            _ => (0..asset_ids.len())
                .map(|index| format!("OUTCOME_{index}"))
                .collect(),
        };
        if outcomes.len() != asset_ids.len() {
            return Err("Gamma market outcomes length must match clobTokenIds".to_string());
        }

        Ok(DiscoveredLiveMarket {
            slug: slug.to_string(),
            series: series.to_string(),
            asset_ids,
            outcomes,
            window_start_s,
            window_end_s: window_start_s + step_s,
        })
    }

    fn string_array_field(value: &Value, field: &str) -> Result<Vec<String>, String> {
        let field_value = value
            .get(field)
            .ok_or_else(|| format!("Gamma market missing {field}"))?;
        string_array_value(field_value).map_err(|error| format!("{field}: {error}"))
    }

    fn string_array_value(value: &Value) -> Result<Vec<String>, String> {
        match value {
            Value::Array(items) => items
                .iter()
                .map(|item| {
                    item.as_str()
                        .map(|value| value.trim().to_string())
                        .filter(|value| !value.is_empty())
                        .ok_or_else(|| "expected non-empty string item".to_string())
                })
                .collect(),
            Value::String(raw) => {
                let parsed: Value = serde_json::from_str(raw)
                    .map_err(|error| format!("invalid encoded array: {error}"))?;
                string_array_value(&parsed)
            }
            _ => Err("expected string array or encoded string array".to_string()),
        }
    }

    fn bool_field(value: &Value, field: &str, default: bool) -> bool {
        match value.get(field) {
            Some(Value::Bool(value)) => *value,
            Some(Value::String(value)) => match value.to_ascii_lowercase().as_str() {
                "true" | "1" | "yes" => true,
                "false" | "0" | "no" => false,
                _ => default,
            },
            _ => default,
        }
    }

    fn required_string<'a>(value: Option<&'a str>, name: &str) -> Result<&'a str, String> {
        value
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("{name} is required"))
    }

    fn parse_csv(value: &str) -> Vec<String> {
        value
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
            .collect()
    }

    fn deadline_reached(deadline_s: Option<i64>) -> bool {
        match deadline_s {
            Some(deadline_s) => unix_timestamp_ms() / 1_000 >= deadline_s,
            None => false,
        }
    }

    fn unix_timestamp_ms() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64
    }

    impl LiveMarketMeta {
        fn outcome_for_asset(&self, asset_id: &str) -> String {
            self.asset_outcomes
                .iter()
                .find(|(known_asset_id, _)| known_asset_id == asset_id)
                .map(|(_, outcome)| outcome.clone())
                .unwrap_or_else(|| "UNKNOWN".to_string())
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
    #[serde(default)]
    alerts: Vec<AlertRecord>,
    #[serde(default)]
    feature_windows: Vec<FeatureWindowRecord>,
    #[serde(default)]
    fair_probability_feature_windows: Vec<FairProbabilityFeatureWindowRecord>,
    score_weights: Option<ScoreWeights>,
    score_thresholds: Option<ScoreThresholds>,
    alert_horizon_ms: Option<i64>,
    label_config: ShiftLabelConfig,
    synchronous_tolerance_ms: i64,
}

#[derive(Debug, Serialize)]
pub struct ReplayValidationResponse {
    alerts: Vec<AlertRecord>,
    labels: Vec<ShiftLabel>,
    report: ValidationReport,
    ablation: Vec<AblationMetric>,
}

#[derive(Debug, Deserialize)]
struct DashboardSnapshotQuery {
    mode: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AgentToolQuery {
    slug: Option<String>,
    limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct FindSimilarWindowsRequest {
    slug: String,
    query_vector: Vec<f64>,
    limit: Option<u32>,
}

#[derive(Debug, Clone)]
struct AppState {
    agent_tool_mongodb_enabled: bool,
    manual_explain: ManualExplainRuntime,
    live_dashboard_paths: Option<LiveDashboardPaths>,
}

#[derive(Debug, Clone)]
struct ManualExplainRuntime {
    throttle: gemini_throttle::GeminiThrottleConfig,
    call_budget: gemini_throttle::GeminiCallBudget,
    state: Arc<Mutex<ManualExplainState>>,
    mode: ManualExplainMode,
}

#[derive(Debug, Default)]
struct ManualExplainState {
    last_started_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum ManualExplainMode {
    Generate,
    DryRun,
}

#[derive(Debug, Clone)]
struct LiveDashboardPaths {
    market_path: PathBuf,
    reference_path: PathBuf,
}

const DEFAULT_AGENT_TOOL_MONGODB_TIMEOUT_MS: u64 = 1_500;
const MIN_AGENT_TOOL_MONGODB_TIMEOUT_MS: u64 = 250;
const MAX_AGENT_TOOL_MONGODB_TIMEOUT_MS: u64 = 5_000;

pub fn agent_tool_mongodb_timeout_from_value(raw_value: Option<&str>) -> Duration {
    let timeout_ms = raw_value
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(DEFAULT_AGENT_TOOL_MONGODB_TIMEOUT_MS)
        .clamp(
            MIN_AGENT_TOOL_MONGODB_TIMEOUT_MS,
            MAX_AGENT_TOOL_MONGODB_TIMEOUT_MS,
        );

    Duration::from_millis(timeout_ms)
}

#[derive(Debug, Serialize)]
pub struct DashboardSnapshot {
    mode: String,
    market: DashboardMarket,
    regime: DashboardRegime,
    price_points: Vec<DashboardPricePoint>,
    alerts: Vec<DashboardAlert>,
    gemini_summary: DashboardGeminiSummary,
    similar_windows: Vec<DashboardSimilarWindow>,
    regime_indicators: Vec<DashboardRegimeIndicator>,
    validation: DashboardValidation,
}

#[derive(Debug, Serialize)]
pub struct DashboardRegime {
    state: &'static str,
    confidence: &'static str,
    updated_at_ms: i64,
    description: String,
}

#[derive(Debug, Serialize)]
pub struct DashboardMarket {
    slug: String,
    series: String,
    title: String,
}

#[derive(Debug, Serialize)]
pub struct DashboardPricePoint {
    timestamp_ms: i64,
    p_mid: f64,
    p_fair: f64,
}

#[derive(Debug, Serialize)]
pub struct DashboardAlert {
    timestamp_ms: i64,
    state: &'static str,
    lead_time_ms: i64,
    score: f64,
}

#[derive(Debug, Serialize)]
pub struct DashboardGeminiSummary {
    enabled: bool,
    generated_at_ms: Option<i64>,
    next_update_at_ms: Option<i64>,
    interval_seconds: Option<i64>,
    coverage: String,
    summary: String,
}

#[derive(Debug, Serialize)]
pub struct DashboardSimilarWindow {
    slug: String,
    window_ts_ms: i64,
    score: f64,
    fair_gap: f64,
    ofi_1s: f64,
    depth_imbalance: f64,
}

#[derive(Debug, Serialize)]
pub struct DashboardRegimeIndicator {
    key: &'static str,
    label: &'static str,
    value: f64,
    unit: &'static str,
    status: &'static str,
    description: &'static str,
}

#[derive(Debug, Serialize)]
pub struct DashboardValidation {
    median_lead_time_ms: Option<i64>,
    p75_lead_time_ms: Option<i64>,
    precision: f64,
    recall: f64,
    degraded_confidence: bool,
    reason: &'static str,
    horizons: Vec<DashboardValidationHorizon>,
}

#[derive(Debug, Serialize)]
pub struct DashboardValidationHorizon {
    horizon_ms: i64,
    pr_auc: f64,
}

pub fn build_router() -> Router {
    build_router_with_gemini_call_budget(gemini_throttle::GeminiCallBudget::new())
}

pub fn build_router_with_gemini_call_budget(
    call_budget: gemini_throttle::GeminiCallBudget,
) -> Router {
    let static_dir =
        PathBuf::from(std::env::var("REGIME_STATIC_DIR").unwrap_or_else(|_| "build".to_string()));

    if static_dir.join("index.html").exists() {
        return build_router_with_static_dir_and_budget(static_dir, call_budget);
    }

    build_api_router_with_budget(call_budget)
}

pub fn build_router_with_agent_tool_mongodb(enabled: bool) -> Router {
    build_api_router_with_state(AppState {
        agent_tool_mongodb_enabled: enabled,
        manual_explain: manual_explain_runtime_from_env(
            ManualExplainMode::Generate,
            gemini_throttle::GeminiCallBudget::new(),
        ),
        live_dashboard_paths: live_dashboard_paths_from_env(),
    })
}

pub fn build_router_with_manual_explain_config(
    agent_tool_mongodb_enabled: bool,
    throttle: gemini_throttle::GeminiThrottleConfig,
) -> Router {
    build_router_with_manual_explain_runtime(
        agent_tool_mongodb_enabled,
        throttle,
        gemini_throttle::GeminiCallBudget::new(),
        ManualExplainMode::DryRun,
    )
}

pub fn build_router_with_live_dashboard_paths(
    market_path: impl AsRef<Path>,
    reference_path: impl AsRef<Path>,
) -> Router {
    build_api_router_with_state(AppState {
        agent_tool_mongodb_enabled: false,
        manual_explain: manual_explain_runtime_from_env(
            ManualExplainMode::Generate,
            gemini_throttle::GeminiCallBudget::new(),
        ),
        live_dashboard_paths: Some(LiveDashboardPaths {
            market_path: market_path.as_ref().to_path_buf(),
            reference_path: reference_path.as_ref().to_path_buf(),
        }),
    })
}

fn build_router_with_manual_explain_runtime(
    agent_tool_mongodb_enabled: bool,
    throttle: gemini_throttle::GeminiThrottleConfig,
    call_budget: gemini_throttle::GeminiCallBudget,
    mode: ManualExplainMode,
) -> Router {
    build_api_router_with_state(AppState {
        agent_tool_mongodb_enabled,
        manual_explain: ManualExplainRuntime {
            throttle,
            call_budget,
            state: Arc::new(Mutex::new(ManualExplainState::default())),
            mode,
        },
        live_dashboard_paths: live_dashboard_paths_from_env(),
    })
}

pub fn build_router_with_static_dir(static_dir: impl AsRef<Path>) -> Router {
    build_router_with_static_dir_and_budget(static_dir, gemini_throttle::GeminiCallBudget::new())
}

fn build_router_with_static_dir_and_budget(
    static_dir: impl AsRef<Path>,
    call_budget: gemini_throttle::GeminiCallBudget,
) -> Router {
    let static_dir = static_dir.as_ref().to_path_buf();
    let index_file = static_dir.join("index.html");

    build_api_router_with_budget(call_budget)
        .fallback_service(ServeDir::new(static_dir).fallback(ServeFile::new(index_file)))
}

fn build_api_router_with_budget(call_budget: gemini_throttle::GeminiCallBudget) -> Router {
    build_api_router_with_state(AppState {
        agent_tool_mongodb_enabled: true,
        manual_explain: manual_explain_runtime_from_env(ManualExplainMode::Generate, call_budget),
        live_dashboard_paths: live_dashboard_paths_from_env(),
    })
}

fn build_api_router_with_state(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/openapi.json", get(openapi_spec))
        .route("/api/dashboard/snapshot", get(dashboard_snapshot))
        .route("/api/dashboard/events", get(dashboard_events))
        .route("/api/agent/current-regime", get(get_current_regime))
        .route("/api/agent/recent-alerts", get(query_recent_alerts))
        .route("/api/agent/similar-windows", post(find_similar_windows))
        .route("/api/agent/backtest-metrics", get(get_backtest_metrics))
        .route("/api/agent/market-summary", get(generate_market_summary))
        .route("/api/agent/explain-now", post(explain_now))
        .route("/api/replay/validate", post(validate_replay))
        .with_state(state)
}

fn manual_explain_runtime_from_env(
    mode: ManualExplainMode,
    call_budget: gemini_throttle::GeminiCallBudget,
) -> ManualExplainRuntime {
    let throttle = gemini_summary::GeminiSummaryConfig::from_env()
        .map(|config| config.throttle)
        .unwrap_or_else(|error| {
            tracing::warn!(%error, "Gemini config invalid; manual explain disabled");
            gemini_throttle::GeminiThrottleConfig {
                enabled: false,
                summary_interval_minutes: 30,
                max_calls_per_hour: 2,
                manual_cooldown_seconds: 300,
            }
        });

    ManualExplainRuntime {
        throttle,
        call_budget,
        state: Arc::new(Mutex::new(ManualExplainState::default())),
        mode,
    }
}

fn live_dashboard_paths_from_env() -> Option<LiveDashboardPaths> {
    let collector_enabled = std::env::var("LIVE_COLLECTOR_ENABLED")
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false);
    let configured_path = std::env::var("LIVE_COLLECTOR_NDJSON_PATH").ok();
    if !collector_enabled && configured_path.is_none() {
        return None;
    }

    let base_path =
        PathBuf::from(configured_path.unwrap_or_else(|| "data/live-fallback.ndjson".to_string()));
    let extension = base_path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("ndjson");
    Some(LiveDashboardPaths {
        market_path: base_path.with_extension(format!("market.{extension}")),
        reference_path: base_path.with_extension(format!("reference.{extension}")),
    })
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "regime-service",
    })
}

async fn openapi_spec() -> Json<serde_json::Value> {
    let server_url = std::env::var("SERVICE_PUBLIC_URL").unwrap_or_else(|_| {
        "https://regime-sentinel-agent-998092298764.asia-northeast3.run.app".to_string()
    });
    Json(serde_json::json!({
        "openapi": "3.0.3",
        "info": {
            "title": "Regime Sentinel Agent API",
            "version": regime_core::VERSION
        },
        "servers": [
            {
                "url": server_url
            }
        ],
        "paths": {
            "/health": {
                "get": {
                    "operationId": "getHealth",
                    "summary": "Read service health",
                    "responses": {
                        "200": {
                            "description": "Service health"
                        }
                    }
                }
            },
            "/api/dashboard/snapshot": {
                "get": {
                    "operationId": "getDashboardSnapshot",
                    "summary": "Read current regime dashboard snapshot",
                    "parameters": [{
                        "name": "mode",
                        "in": "query",
                        "required": false,
                        "schema": {
                            "type": "string",
                            "enum": ["live", "replay"]
                        }
                    }],
                    "responses": {
                        "200": {
                            "description": "Dashboard snapshot with regime, prices, alerts, summary, similar windows, and validation",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/DashboardSnapshot"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/api/agent/current-regime": {
                "get": {
                    "operationId": "getCurrentRegime",
                    "summary": "Read the latest regime state from MongoDB memory or demo fallback",
                    "parameters": [{
                        "name": "slug",
                        "in": "query",
                        "required": false,
                        "schema": {
                            "type": "string"
                        }
                    }],
                    "responses": {
                        "200": {
                            "description": "Latest regime state"
                        }
                    }
                }
            },
            "/api/agent/recent-alerts": {
                "get": {
                    "operationId": "queryRecentAlerts",
                    "summary": "Read recent regime alerts from MongoDB memory or demo fallback",
                    "parameters": [
                        {
                            "name": "slug",
                            "in": "query",
                            "required": false,
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "limit",
                            "in": "query",
                            "required": false,
                            "schema": {
                                "type": "integer",
                                "format": "int64",
                                "minimum": 1,
                                "maximum": 50
                            }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Recent alert list"
                        }
                    }
                }
            },
            "/api/agent/similar-windows": {
                "post": {
                    "operationId": "findSimilarWindows",
                    "summary": "Find historical windows with MongoDB Vector Search",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/FindSimilarWindowsRequest"
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Similar historical windows"
                        }
                    }
                }
            },
            "/api/agent/backtest-metrics": {
                "get": {
                    "operationId": "getBacktestMetrics",
                    "summary": "Read latest backtest metrics from MongoDB memory or demo fallback",
                    "parameters": [{
                        "name": "limit",
                        "in": "query",
                        "required": false,
                        "schema": {
                            "type": "integer",
                            "format": "int64",
                            "minimum": 1,
                            "maximum": 10
                        }
                    }],
                    "responses": {
                        "200": {
                            "description": "Backtest metrics"
                        }
                    }
                }
            },
            "/api/agent/market-summary": {
                "get": {
                    "operationId": "generateMarketSummary",
                    "summary": "Read cached Gemini market summary without forcing a new model call",
                    "parameters": [{
                        "name": "slug",
                        "in": "query",
                        "required": false,
                        "schema": {
                            "type": "string"
                        }
                    }],
                    "responses": {
                        "200": {
                            "description": "Cached market summary"
                        }
                    }
                }
            },
            "/api/agent/explain-now": {
                "post": {
                    "operationId": "explainNow",
                    "summary": "Run a manually requested Gemini explanation subject to cooldown and hourly call limits",
                    "responses": {
                        "200": {
                            "description": "Manual explanation generated, dry-run generated, or Gemini is disabled"
                        },
                        "429": {
                            "description": "Manual explanation is cooling down or hourly call cap is exhausted"
                        },
                        "502": {
                            "description": "Gemini generation failed"
                        }
                    }
                }
            },
            "/api/replay/validate": {
                "post": {
                    "operationId": "validateReplay",
                    "summary": "Validate replay alerts with strict fair-probability or legacy feature windows",
                    "description": "Use fair_probability_feature_windows for strict computed p_fair validation; feature_windows remains a legacy compatibility path for caller-provided p_fair replay fixtures.",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/ReplayValidationRequest"
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Replay validation report"
                        }
                    }
                }
            }
        },
        "components": {
            "schemas": {
                "ReplayValidationRequest": {
                    "type": "object",
                    "required": ["price_points", "label_config", "synchronous_tolerance_ms"],
                    "properties": {
                        "price_points": {
                            "type": "array",
                            "items": {
                                "$ref": "#/components/schemas/PricePoint"
                            }
                        },
                        "alerts": {
                            "type": "array",
                            "items": {
                                "$ref": "#/components/schemas/AlertRecord"
                            },
                            "default": []
                        },
                        "feature_windows": {
                            "description": "Legacy compatibility path: accepts caller-provided p_fair/fair_gap feature windows.",
                            "type": "array",
                            "items": {
                                "$ref": "#/components/schemas/FeatureWindowRecord"
                            },
                            "default": []
                        },
                        "fair_probability_feature_windows": {
                            "description": "Strict acceptance path: computes p_fair from current_price, strike_price, time_remaining_ms, realized_volatility, and feed_lag_ms.",
                            "type": "array",
                            "items": {
                                "$ref": "#/components/schemas/FairProbabilityFeatureWindowRecord"
                            },
                            "default": []
                        },
                        "score_weights": {
                            "$ref": "#/components/schemas/ScoreWeights"
                        },
                        "score_thresholds": {
                            "$ref": "#/components/schemas/ScoreThresholds"
                        },
                        "alert_horizon_ms": {
                            "type": "integer",
                            "format": "int64"
                        },
                        "label_config": {
                            "$ref": "#/components/schemas/ShiftLabelConfig"
                        },
                        "synchronous_tolerance_ms": {
                            "type": "integer",
                            "format": "int64"
                        }
                    }
                },
                "FindSimilarWindowsRequest": {
                    "type": "object",
                    "required": ["slug", "query_vector"],
                    "properties": {
                        "slug": {
                            "type": "string"
                        },
                        "query_vector": {
                            "type": "array",
                            "items": {
                                "type": "number",
                                "format": "double"
                            },
                            "minItems": 5,
                            "maxItems": 5
                        },
                        "limit": {
                            "type": "integer",
                            "format": "int32",
                            "minimum": 1,
                            "maximum": 25
                        }
                    }
                },
                "DashboardSnapshot": {
                    "type": "object",
                    "required": ["mode", "market", "regime", "price_points", "alerts", "gemini_summary", "similar_windows", "regime_indicators", "validation"],
                    "properties": {
                        "mode": {
                            "type": "string",
                            "enum": ["live", "replay"]
                        },
                        "market": {
                            "$ref": "#/components/schemas/DashboardMarket"
                        },
                        "regime": {
                            "$ref": "#/components/schemas/DashboardRegime"
                        },
                        "price_points": {
                            "type": "array",
                            "items": {
                                "$ref": "#/components/schemas/DashboardPricePoint"
                            }
                        },
                        "alerts": {
                            "type": "array",
                            "items": {
                                "$ref": "#/components/schemas/DashboardAlert"
                            }
                        },
                        "gemini_summary": {
                            "$ref": "#/components/schemas/DashboardGeminiSummary"
                        },
                        "similar_windows": {
                            "type": "array",
                            "items": {
                                "$ref": "#/components/schemas/DashboardSimilarWindow"
                            }
                        },
                        "regime_indicators": {
                            "type": "array",
                            "items": {
                                "$ref": "#/components/schemas/DashboardRegimeIndicator"
                            }
                        },
                        "validation": {
                            "$ref": "#/components/schemas/DashboardValidation"
                        }
                    }
                },
                "DashboardRegime": {
                    "type": "object",
                    "required": ["state", "confidence", "updated_at_ms", "description"],
                    "properties": {
                        "state": {
                            "type": "string"
                        },
                        "confidence": {
                            "type": "string"
                        },
                        "updated_at_ms": {
                            "type": "integer",
                            "format": "int64"
                        },
                        "description": {
                            "type": "string"
                        }
                    }
                },
                "DashboardMarket": {
                    "type": "object",
                    "required": ["slug", "series", "title"],
                    "properties": {
                        "slug": {
                            "type": "string"
                        },
                        "series": {
                            "type": "string"
                        },
                        "title": {
                            "type": "string"
                        }
                    }
                },
                "DashboardPricePoint": {
                    "type": "object",
                    "required": ["timestamp_ms", "p_mid", "p_fair"],
                    "properties": {
                        "timestamp_ms": {
                            "type": "integer",
                            "format": "int64"
                        },
                        "p_mid": {
                            "type": "number",
                            "format": "double"
                        },
                        "p_fair": {
                            "type": "number",
                            "format": "double"
                        }
                    }
                },
                "DashboardAlert": {
                    "type": "object",
                    "required": ["timestamp_ms", "state", "lead_time_ms", "score"],
                    "properties": {
                        "timestamp_ms": {
                            "type": "integer",
                            "format": "int64"
                        },
                        "state": {
                            "type": "string"
                        },
                        "lead_time_ms": {
                            "type": "integer",
                            "format": "int64"
                        },
                        "score": {
                            "type": "number",
                            "format": "double"
                        }
                    }
                },
                "DashboardGeminiSummary": {
                    "type": "object",
                    "required": ["enabled", "generated_at_ms", "next_update_at_ms", "interval_seconds", "coverage", "summary"],
                    "properties": {
                        "enabled": {
                            "type": "boolean"
                        },
                        "generated_at_ms": {
                            "type": "integer",
                            "format": "int64",
                            "nullable": true
                        },
                        "next_update_at_ms": {
                            "type": "integer",
                            "format": "int64",
                            "nullable": true
                        },
                        "interval_seconds": {
                            "type": "integer",
                            "format": "int64",
                            "nullable": true
                        },
                        "coverage": {
                            "type": "string"
                        },
                        "summary": {
                            "type": "string"
                        }
                    }
                },
                "DashboardSimilarWindow": {
                    "type": "object",
                    "required": ["slug", "window_ts_ms", "score", "fair_gap", "ofi_1s", "depth_imbalance"],
                    "properties": {
                        "slug": {
                            "type": "string"
                        },
                        "window_ts_ms": {
                            "type": "integer",
                            "format": "int64"
                        },
                        "score": {
                            "type": "number",
                            "format": "double"
                        },
                        "fair_gap": {
                            "type": "number",
                            "format": "double"
                        },
                        "ofi_1s": {
                            "type": "number",
                            "format": "double"
                        },
                        "depth_imbalance": {
                            "type": "number",
                            "format": "double"
                        }
                    }
                },
                "DashboardRegimeIndicator": {
                    "type": "object",
                    "required": ["key", "label", "value", "unit", "status", "description"],
                    "properties": {
                        "key": {
                            "type": "string"
                        },
                        "label": {
                            "type": "string"
                        },
                        "value": {
                            "type": "number",
                            "format": "double"
                        },
                        "unit": {
                            "type": "string"
                        },
                        "status": {
                            "type": "string"
                        },
                        "description": {
                            "type": "string"
                        }
                    }
                },
                "DashboardValidation": {
                    "type": "object",
                    "required": ["median_lead_time_ms", "p75_lead_time_ms", "precision", "recall", "degraded_confidence", "reason", "horizons"],
                    "properties": {
                        "median_lead_time_ms": {
                            "type": "integer",
                            "format": "int64",
                            "nullable": true
                        },
                        "p75_lead_time_ms": {
                            "type": "integer",
                            "format": "int64",
                            "nullable": true
                        },
                        "precision": {
                            "type": "number",
                            "format": "double"
                        },
                        "recall": {
                            "type": "number",
                            "format": "double"
                        },
                        "degraded_confidence": {
                            "type": "boolean"
                        },
                        "reason": {
                            "type": "string"
                        },
                        "horizons": {
                            "type": "array",
                            "items": {
                                "$ref": "#/components/schemas/DashboardValidationHorizon"
                            }
                        }
                    }
                },
                "DashboardValidationHorizon": {
                    "type": "object",
                    "required": ["horizon_ms", "pr_auc"],
                    "properties": {
                        "horizon_ms": {
                            "type": "integer",
                            "format": "int64"
                        },
                        "pr_auc": {
                            "type": "number",
                            "format": "double"
                        }
                    }
                },
                "PricePoint": {
                    "type": "object",
                    "required": ["timestamp_ms", "p_mid"],
                    "properties": {
                        "timestamp_ms": {
                            "type": "integer",
                            "format": "int64"
                        },
                        "p_mid": {
                            "type": "number",
                            "format": "double"
                        }
                    }
                },
                "AlertRecord": {
                    "type": "object",
                    "required": ["timestamp_ms", "state", "confidence", "horizon_ms", "score"],
                    "properties": {
                        "timestamp_ms": {
                            "type": "integer",
                            "format": "int64"
                        },
                        "state": {
                            "type": "string",
                            "enum": ["Equilibrium", "Watch", "EarlyRisk", "ShiftDetected"]
                        },
                        "confidence": {
                            "type": "string",
                            "enum": ["Normal", "Low"]
                        },
                        "horizon_ms": {
                            "type": "integer",
                            "format": "int64"
                        },
                        "score": {
                            "type": "number",
                            "format": "double"
                        }
                    }
                },
                "FeatureWindowRecord": {
                    "type": "object",
                    "required": [
                        "slug",
                        "window_ts_ms",
                        "window_ms",
                        "p_mid",
                        "p_fair",
                        "fair_gap",
                        "ofi_1s",
                        "depth_imbalance",
                        "spread",
                        "volume_acceleration",
                        "feature_vector"
                    ],
                    "properties": {
                        "slug": {
                            "type": "string"
                        },
                        "window_ts_ms": {
                            "type": "integer",
                            "format": "int64"
                        },
                        "window_ms": {
                            "type": "integer",
                            "format": "int64"
                        },
                        "p_mid": {
                            "type": "number",
                            "format": "double"
                        },
                        "p_fair": {
                            "type": "number",
                            "format": "double"
                        },
                        "fair_gap": {
                            "type": "number",
                            "format": "double"
                        },
                        "ofi_1s": {
                            "type": "number",
                            "format": "double"
                        },
                        "depth_imbalance": {
                            "type": "number",
                            "format": "double"
                        },
                        "spread": {
                            "type": "number",
                            "format": "double"
                        },
                        "volume_acceleration": {
                            "type": "number",
                            "format": "double"
                        },
                        "feature_vector": {
                            "type": "array",
                            "items": {
                                "type": "number",
                                "format": "double"
                            },
                            "minItems": 5,
                            "maxItems": 5
                        }
                    }
                },
                "FairProbabilityFeatureWindowRecord": {
                    "type": "object",
                    "required": [
                        "slug",
                        "window_ts_ms",
                        "window_ms",
                        "p_mid",
                        "fair_probability",
                        "ofi_1s",
                        "depth_imbalance",
                        "spread",
                        "volume_acceleration"
                    ],
                    "properties": {
                        "slug": {
                            "type": "string"
                        },
                        "window_ts_ms": {
                            "type": "integer",
                            "format": "int64"
                        },
                        "window_ms": {
                            "type": "integer",
                            "format": "int64"
                        },
                        "p_mid": {
                            "type": "number",
                            "format": "double"
                        },
                        "fair_probability": {
                            "$ref": "#/components/schemas/FairProbabilityInput"
                        },
                        "ofi_1s": {
                            "type": "number",
                            "format": "double"
                        },
                        "depth_imbalance": {
                            "type": "number",
                            "format": "double"
                        },
                        "spread": {
                            "type": "number",
                            "format": "double"
                        },
                        "volume_acceleration": {
                            "type": "number",
                            "format": "double"
                        }
                    }
                },
                "FairProbabilityInput": {
                    "type": "object",
                    "required": [
                        "current_price",
                        "strike_price",
                        "time_remaining_ms",
                        "realized_volatility"
                    ],
                    "properties": {
                        "current_price": {
                            "type": "number",
                            "format": "double"
                        },
                        "strike_price": {
                            "type": "number",
                            "format": "double"
                        },
                        "time_remaining_ms": {
                            "type": "integer",
                            "format": "int64"
                        },
                        "realized_volatility": {
                            "type": "number",
                            "format": "double"
                        },
                        "feed_lag_ms": {
                            "type": "integer",
                            "format": "int64",
                            "default": 0
                        }
                    }
                },
                "ScoreWeights": {
                    "type": "object",
                    "required": [
                        "fair_gap_velocity",
                        "depth_imbalance",
                        "ofi_1s",
                        "volume_acceleration",
                        "stale_data_penalty"
                    ],
                    "properties": {
                        "fair_gap_velocity": {
                            "type": "number",
                            "format": "double"
                        },
                        "depth_imbalance": {
                            "type": "number",
                            "format": "double"
                        },
                        "ofi_1s": {
                            "type": "number",
                            "format": "double"
                        },
                        "volume_acceleration": {
                            "type": "number",
                            "format": "double"
                        },
                        "stale_data_penalty": {
                            "type": "number",
                            "format": "double"
                        }
                    }
                },
                "ScoreThresholds": {
                    "type": "object",
                    "required": ["watch", "early_risk", "shift_detected_move"],
                    "properties": {
                        "watch": {
                            "type": "number",
                            "format": "double"
                        },
                        "early_risk": {
                            "type": "number",
                            "format": "double"
                        },
                        "shift_detected_move": {
                            "type": "number",
                            "format": "double"
                        }
                    }
                },
                "ShiftLabelConfig": {
                    "type": "object",
                    "required": ["horizons_ms", "min_move", "persist_ms"],
                    "properties": {
                        "horizons_ms": {
                            "type": "array",
                            "items": {
                                "type": "integer",
                                "format": "int64"
                            }
                        },
                        "min_move": {
                            "type": "number",
                            "format": "double"
                        },
                        "persist_ms": {
                            "type": "integer",
                            "format": "int64"
                        }
                    }
                }
            }
        }
    }))
}

async fn dashboard_snapshot(
    State(state): State<AppState>,
    Query(query): Query<DashboardSnapshotQuery>,
) -> Json<DashboardSnapshot> {
    let mode = dashboard_mode(query.mode.as_deref());
    Json(dashboard_snapshot_for_state_with_summary(&state, mode).await)
}

async fn dashboard_events(
    State(state): State<AppState>,
) -> Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>> {
    let stream = stream::unfold(
        (tokio::time::interval(Duration::from_secs(1)), state),
        |(mut interval, state)| async {
            interval.tick().await;
            let snapshot = dashboard_snapshot_for_state_with_summary(&state, "live").await;
            let data = serde_json::to_string(&snapshot).expect("dashboard snapshot serializes");
            Some((
                Ok(Event::default().event("snapshot").data(data)),
                (interval, state),
            ))
        },
    );

    Sse::new(stream).keep_alive(KeepAlive::default())
}

fn dashboard_snapshot_for_state(state: &AppState, mode: &str) -> DashboardSnapshot {
    state
        .live_dashboard_paths
        .as_ref()
        .and_then(|paths| {
            live_dashboard_snapshot(mode, paths, state.manual_explain.throttle.enabled)
        })
        .unwrap_or_else(|| sample_dashboard_snapshot(mode))
}

async fn dashboard_snapshot_for_state_with_summary(
    state: &AppState,
    mode: &str,
) -> DashboardSnapshot {
    let mut snapshot = dashboard_snapshot_for_state(state, mode);
    if let Some(summary) = cached_dashboard_gemini_summary(state).await {
        snapshot.gemini_summary = summary;
    }
    snapshot
}

fn dashboard_mode(mode: Option<&str>) -> &'static str {
    match mode {
        Some("replay") => "replay",
        _ => "live",
    }
}

async fn cached_dashboard_gemini_summary(state: &AppState) -> Option<DashboardGeminiSummary> {
    let store = agent_tool_mongo_store(state).await?;
    let summary = match store.latest_agent_summary().await {
        Ok(Some(summary)) => summary,
        Ok(None) => return None,
        Err(error) => {
            tracing::warn!(?error, "read dashboard Gemini summary from MongoDB failed");
            return None;
        }
    };

    dashboard_gemini_summary_from_document(&summary, state.manual_explain.throttle.enabled)
}

fn dashboard_gemini_summary_from_document(
    summary: &mongodb::bson::Document,
    enabled: bool,
) -> Option<DashboardGeminiSummary> {
    let bucket_start_ms = summary
        .get_datetime("bucket_start")
        .ok()
        .map(|value| value.timestamp_millis())?;
    let interval_seconds = summary
        .get_i32("bucket_seconds")
        .ok()
        .map(i64::from)
        .unwrap_or(1_800);
    let text = summary.get_str("summary").ok()?.to_string();

    Some(DashboardGeminiSummary {
        enabled,
        generated_at_ms: Some(bucket_start_ms),
        next_update_at_ms: next_gemini_update_at_ms(Some(bucket_start_ms), interval_seconds),
        interval_seconds: Some(interval_seconds),
        coverage: format!("last {} minutes", interval_seconds / 60),
        summary: text,
    })
}

fn live_dashboard_snapshot(
    mode: &str,
    paths: &LiveDashboardPaths,
    gemini_enabled: bool,
) -> Option<DashboardSnapshot> {
    let mut ticks = recent_market_ticks_from_ndjson(&paths.market_path, 20_000);
    ticks.extend(recent_market_ticks_from_ndjson(&paths.reference_path, 500));
    if ticks.is_empty() {
        return None;
    }

    ticks.sort_by_key(|tick| tick.timestamp_ms);
    let latest_tick = ticks.last().expect("non-empty live ticks");
    let latest_slug = latest_tick.meta.slug.clone();
    let series = latest_tick.meta.series.clone();
    let now_ms = unix_timestamp_ms();
    let slug = dashboard_active_market_slug(&latest_slug, &series, now_ms / 1_000);
    let current_ticks = ticks
        .iter()
        .filter(|tick| tick.meta.slug == slug)
        .cloned()
        .collect::<Vec<_>>();
    let scoped_ticks = if current_ticks.is_empty() && slug == latest_slug {
        ticks
    } else {
        current_ticks
    };
    let latest_ts = scoped_ticks
        .iter()
        .map(|tick| tick.timestamp_ms)
        .max()
        .unwrap_or(now_ms);

    let mut up_points = scoped_ticks
        .iter()
        .filter(|tick| {
            tick.meta.source == "clob"
                && is_up_outcome(&tick.outcome)
                && is_midpoint_tick(&tick.side)
        })
        .map(|tick| DashboardPricePoint {
            timestamp_ms: tick.timestamp_ms,
            p_mid: tick.price,
            p_fair: 0.5,
        })
        .collect::<Vec<_>>();
    up_points.sort_by_key(|point| point.timestamp_ms);
    up_points.dedup_by_key(|point| point.timestamp_ms);

    if up_points.len() > 120 {
        up_points = up_points.split_off(up_points.len() - 120);
    }

    let has_clob_midpoint = !up_points.is_empty();
    let latest_up = up_points.last().map(|point| point.p_mid).unwrap_or(0.5);
    let fair_gap = if has_clob_midpoint {
        latest_up - 0.5
    } else {
        0.0
    };
    let mid_velocity_1s = point_velocity(&up_points, 1_000);
    let mid_velocity_5s = point_velocity(&up_points, 5_000);
    let order_flow_1s = order_flow_imbalance(&scoped_ticks, latest_ts, 1_000);
    let reference_velocity_1s = reference_velocity(&scoped_ticks, 1_000);
    let shift_score = live_shift_score(
        fair_gap,
        mid_velocity_1s,
        order_flow_1s,
        reference_velocity_1s,
    );
    let (regime_state, regime_description) = if has_clob_midpoint {
        live_regime_description(latest_up, mid_velocity_1s, order_flow_1s, shift_score)
    } else {
        (
            "WAITING_LIVE_CLOB",
            "Live collector is connected to the current window, but no Polymarket Up midpoint tick has been received yet.".to_string(),
        )
    };
    let market_tick_count = scoped_ticks
        .iter()
        .filter(|tick| tick.meta.source == "clob")
        .count();
    let reference_tick_count = scoped_ticks
        .iter()
        .filter(|tick| tick.meta.source == "chainlink")
        .count();

    Some(DashboardSnapshot {
        mode: mode.to_string(),
        market: DashboardMarket {
            slug: slug.clone(),
            series,
            title: std::env::var("LIVE_MARKET_TITLE").unwrap_or_else(|_| slug.clone()),
        },
        regime: DashboardRegime {
            state: regime_state,
            confidence: if !has_clob_midpoint {
                "Low"
            } else if latest_ts + 5_000 >= unix_timestamp_ms() {
                "Normal"
            } else {
                "Low"
            },
            updated_at_ms: latest_ts,
            description: regime_description,
        },
        price_points: up_points,
        alerts: Vec::new(),
        gemini_summary: DashboardGeminiSummary {
            enabled: gemini_enabled,
            generated_at_ms: Some(latest_ts),
            next_update_at_ms: next_gemini_update_at_ms(Some(latest_ts), 1_800),
            interval_seconds: Some(1_800),
            coverage: "live collector".to_string(),
            summary: if has_clob_midpoint {
                "Live collector view: chart uses Polymarket Up midpoint ticks when available; the dashed fair line is neutral until live strike/start-price fair-probability is wired in.".to_string()
            } else {
                "Live collector view: waiting for the first Polymarket Up midpoint tick; Chainlink BTC/USD is used only for reference velocity, not as p_mid.".to_string()
            },
        },
        similar_windows: if has_clob_midpoint {
            vec![DashboardSimilarWindow {
                slug,
                window_ts_ms: latest_ts,
                score: 1.0,
                fair_gap,
                ofi_1s: market_tick_count as f64,
                depth_imbalance: reference_tick_count as f64,
            }]
        } else {
            Vec::new()
        },
        regime_indicators: vec![
            DashboardRegimeIndicator {
                key: "fair_gap",
                label: "Fair gap",
                value: fair_gap,
                unit: "pp",
                status: live_indicator_status(has_clob_midpoint, fair_gap.abs(), 0.03, 0.08),
                description: "Up midpoint minus the neutral fair line; positive values mean the market is pricing Up above neutral.",
            },
            DashboardRegimeIndicator {
                key: "mid_velocity_1s",
                label: "Mid velocity 1s",
                value: mid_velocity_1s,
                unit: "pp/s",
                status: live_indicator_status(has_clob_midpoint, mid_velocity_1s.abs(), 0.02, 0.07),
                description: "One-second change rate of the Polymarket Up midpoint.",
            },
            DashboardRegimeIndicator {
                key: "order_flow_1s",
                label: "Order flow 1s",
                value: order_flow_1s,
                unit: "",
                status: live_indicator_status(has_clob_midpoint, order_flow_1s.abs(), 0.25, 0.60),
                description: "Signed one-second flow proxy: Up buys and Down sells are positive; Up sells and Down buys are negative.",
            },
            DashboardRegimeIndicator {
                key: "btc_velocity_1s",
                label: "BTC velocity 1s",
                value: reference_velocity_1s,
                unit: "$/s",
                status: live_indicator_status(
                    has_clob_midpoint,
                    reference_velocity_1s.abs(),
                    20.0,
                    75.0,
                ),
                description: "One-second Chainlink BTC/USD reference price velocity.",
            },
            DashboardRegimeIndicator {
                key: "shift_score",
                label: "Shift score",
                value: shift_score,
                unit: "",
                status: live_shift_score_status(has_clob_midpoint, shift_score),
                description: "Combined live heuristic from fair gap, midpoint velocity, order flow, and BTC reference velocity.",
            },
            DashboardRegimeIndicator {
                key: "mid_velocity_5s",
                label: "Mid velocity 5s",
                value: mid_velocity_5s,
                unit: "pp/s",
                status: live_indicator_status(has_clob_midpoint, mid_velocity_5s.abs(), 0.01, 0.04),
                description: "Five-second normalized change rate of the Polymarket Up midpoint.",
            },
        ],
        validation: DashboardValidation {
            median_lead_time_ms: None,
            p75_lead_time_ms: None,
            precision: 0.0,
            recall: 0.0,
            degraded_confidence: true,
            reason: "Live collector is running; replay validation remains the acceptance source for forecast metrics.",
            horizons: vec![
                DashboardValidationHorizon {
                    horizon_ms: 1_000,
                    pr_auc: 0.0,
                },
                DashboardValidationHorizon {
                    horizon_ms: 5_000,
                    pr_auc: 0.0,
                },
                DashboardValidationHorizon {
                    horizon_ms: 30_000,
                    pr_auc: 0.0,
                },
            ],
        },
    })
}

fn dashboard_active_market_slug(latest_slug: &str, series: &str, now_s: i64) -> String {
    let Some(window_start_s) = market_window_start_from_slug(latest_slug) else {
        return latest_slug.to_string();
    };
    let window_end_s = window_start_s + live_collector::DEFAULT_WINDOW_STEP_SECONDS;
    let active_start_s = live_collector::target_window_start_seconds(
        now_s,
        live_collector::DEFAULT_WINDOW_STEP_SECONDS,
    );

    if now_s >= window_end_s && active_start_s > window_start_s {
        format!("{series}-{active_start_s}")
    } else {
        latest_slug.to_string()
    }
}

fn market_window_start_from_slug(slug: &str) -> Option<i64> {
    let suffix = slug.rsplit('-').next()?;
    if suffix.len() != 10 || !suffix.chars().all(|value| value.is_ascii_digit()) {
        return None;
    }

    suffix.parse().ok()
}

fn recent_market_ticks_from_ndjson(path: &Path, limit: usize) -> Vec<MarketTickRecord> {
    let Ok(file) = File::open(path) else {
        return Vec::new();
    };
    let mut ticks = VecDeque::with_capacity(limit);
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        if value.get("kind").and_then(serde_json::Value::as_str) != Some("market_tick") {
            continue;
        }
        let Ok(tick) = serde_json::from_value::<MarketTickRecord>(value["record"].clone()) else {
            continue;
        };
        if ticks.len() == limit {
            ticks.pop_front();
        }
        ticks.push_back(tick);
    }
    ticks.into_iter().collect()
}

fn point_velocity(points: &[DashboardPricePoint], horizon_ms: i64) -> f64 {
    let Some(latest) = points.last() else {
        return 0.0;
    };
    let target_ts = latest.timestamp_ms.saturating_sub(horizon_ms);
    let previous = points
        .iter()
        .rev()
        .find(|point| point.timestamp_ms <= target_ts)
        .or_else(|| points.first());
    let Some(previous) = previous else {
        return 0.0;
    };
    let elapsed_seconds = (latest.timestamp_ms - previous.timestamp_ms) as f64 / 1_000.0;
    if elapsed_seconds <= 0.0 {
        return 0.0;
    }
    (latest.p_mid - previous.p_mid) / elapsed_seconds
}

fn order_flow_imbalance(ticks: &[MarketTickRecord], latest_ts: i64, window_ms: i64) -> f64 {
    let start_ts = latest_ts.saturating_sub(window_ms);
    let mut signed_size = 0.0;
    let mut total_size = 0.0;

    for tick in ticks.iter().filter(|tick| {
        tick.meta.source == "clob"
            && tick.timestamp_ms >= start_ts
            && tick.timestamp_ms <= latest_ts
            && tick.size > 0.0
    }) {
        let sign = match (is_up_outcome(&tick.outcome), tick.side.as_str()) {
            (true, "BUY") | (false, "SELL") => 1.0,
            (true, "SELL") | (false, "BUY") => -1.0,
            _ => 0.0,
        };
        if sign == 0.0 {
            continue;
        }
        signed_size += sign * tick.size;
        total_size += tick.size;
    }

    if total_size == 0.0 {
        0.0
    } else {
        (signed_size / total_size).clamp(-1.0, 1.0)
    }
}

fn reference_velocity(ticks: &[MarketTickRecord], horizon_ms: i64) -> f64 {
    let reference_ticks = ticks
        .iter()
        .filter(|tick| tick.meta.source == "chainlink")
        .collect::<Vec<_>>();
    let Some(latest) = reference_ticks.last() else {
        return 0.0;
    };
    let target_ts = latest.timestamp_ms.saturating_sub(horizon_ms);
    let previous = reference_ticks
        .iter()
        .rev()
        .find(|tick| tick.timestamp_ms <= target_ts)
        .or_else(|| reference_ticks.first());
    let Some(previous) = previous else {
        return 0.0;
    };
    let elapsed_seconds = (latest.timestamp_ms - previous.timestamp_ms) as f64 / 1_000.0;
    if elapsed_seconds <= 0.0 {
        return 0.0;
    }
    (latest.price - previous.price) / elapsed_seconds
}

fn live_shift_score(
    fair_gap: f64,
    mid_velocity_1s: f64,
    order_flow_1s: f64,
    reference_velocity_1s: f64,
) -> f64 {
    (fair_gap.abs() * 2.0
        + mid_velocity_1s.abs() * 4.0
        + order_flow_1s.abs() * 0.4
        + (reference_velocity_1s.abs() / 100.0).min(0.3))
    .clamp(0.0, 1.0)
}

fn live_regime_description(
    latest_up: f64,
    mid_velocity_1s: f64,
    order_flow_1s: f64,
    shift_score: f64,
) -> (&'static str, String) {
    if shift_score >= 0.75 {
        let side = if latest_up >= 0.5 {
            "Up-side"
        } else {
            "Down-side"
        };
        return (
            "SHIFT_RISK",
            format!(
                "{side} shift risk: midpoint, short-horizon velocity, and signed flow are moving together; this is a live heuristic, not yet a validated forecast."
            ),
        );
    }

    if latest_up >= 0.56 || (mid_velocity_1s > 0.02 && order_flow_1s > 0.20) {
        return (
            "UP_PRESSURE",
            "Up-side pressure: the Up midpoint is above neutral or recent flow is leaning toward Up."
                .to_string(),
        );
    }

    if latest_up <= 0.44 || (mid_velocity_1s < -0.02 && order_flow_1s < -0.20) {
        return (
            "DOWN_PRESSURE",
            "Down-side pressure: the Up midpoint is below neutral or recent flow is leaning toward Down."
                .to_string(),
        );
    }

    (
        "BALANCED_LIVE",
        "Balanced live regime: midpoint and short-horizon flow are near neutral; continue watching for velocity and flow alignment.".to_string(),
    )
}

fn indicator_status(value: f64, elevated_threshold: f64, high_threshold: f64) -> &'static str {
    if value >= high_threshold {
        "high"
    } else if value >= elevated_threshold {
        "elevated"
    } else {
        "normal"
    }
}

fn live_indicator_status(
    has_clob_midpoint: bool,
    value: f64,
    elevated_threshold: f64,
    high_threshold: f64,
) -> &'static str {
    if has_clob_midpoint {
        indicator_status(value, elevated_threshold, high_threshold)
    } else {
        "waiting"
    }
}

fn shift_score_status(score: f64) -> &'static str {
    if score >= 0.75 {
        "high"
    } else if score >= 0.45 {
        "watch"
    } else {
        "normal"
    }
}

fn live_shift_score_status(has_clob_midpoint: bool, score: f64) -> &'static str {
    if has_clob_midpoint {
        shift_score_status(score)
    } else {
        "waiting"
    }
}

fn is_up_outcome(outcome: &str) -> bool {
    matches!(outcome, "UP" | "Up" | "YES" | "Yes")
}

fn is_midpoint_tick(side: &str) -> bool {
    matches!(side, "BBA" | "BOOK")
}

fn next_gemini_update_at_ms(generated_at_ms: Option<i64>, interval_seconds: i64) -> Option<i64> {
    generated_at_ms.map(|generated_at_ms| generated_at_ms + interval_seconds * 1_000)
}

fn sample_dashboard_snapshot(mode: &str) -> DashboardSnapshot {
    DashboardSnapshot {
        mode: mode.to_string(),
        market: DashboardMarket {
            slug: "btc-updown-5m-1768999700".to_string(),
            series: "btc-updown-5m".to_string(),
            title: "Bitcoin Up or Down - demo replay".to_string(),
        },
        regime: DashboardRegime {
            state: "EARLY_RISK",
            confidence: "Normal",
            updated_at_ms: 1_769_000_000_750,
            description:
                "Demo replay regime: Up-side pressure increased before the generated alert marker."
                    .to_string(),
        },
        price_points: vec![
            DashboardPricePoint {
                timestamp_ms: 1_769_000_000_000,
                p_mid: 0.50,
                p_fair: 0.49,
            },
            DashboardPricePoint {
                timestamp_ms: 1_769_000_000_750,
                p_mid: 0.54,
                p_fair: 0.49,
            },
            DashboardPricePoint {
                timestamp_ms: 1_769_000_001_000,
                p_mid: 0.62,
                p_fair: 0.51,
            },
        ],
        alerts: vec![DashboardAlert {
            timestamp_ms: 1_769_000_000_750,
            state: "EARLY_RISK",
            lead_time_ms: 250,
            score: 1.94,
        }],
        gemini_summary: DashboardGeminiSummary {
            enabled: true,
            generated_at_ms: Some(1_769_000_001_000),
            next_update_at_ms: next_gemini_update_at_ms(Some(1_769_000_001_000), 1_800),
            interval_seconds: Some(1_800),
            coverage: "last 30 minutes".to_string(),
            summary: "Cached demo summary: early risk increased because fair-gap velocity, order flow, and depth imbalance moved in the same direction.".to_string(),
        },
        similar_windows: vec![DashboardSimilarWindow {
            slug: "btc-updown-5m-1768999700".to_string(),
            window_ts_ms: 1_768_999_700_750,
            score: 0.98,
            fair_gap: 0.05,
            ofi_1s: 0.42,
            depth_imbalance: 0.31,
        }],
        regime_indicators: vec![
            DashboardRegimeIndicator {
                key: "fair_gap",
                label: "Fair gap",
                value: 0.05,
                unit: "pp",
                status: "elevated",
                description: "Demo fair probability gap used by the replay alert.",
            },
            DashboardRegimeIndicator {
                key: "mid_velocity_1s",
                label: "Mid velocity 1s",
                value: 0.08,
                unit: "pp/s",
                status: "high",
                description: "Demo one-second midpoint repricing velocity.",
            },
            DashboardRegimeIndicator {
                key: "order_flow_1s",
                label: "Order flow 1s",
                value: 0.42,
                unit: "",
                status: "elevated",
                description: "Demo signed flow proxy used by the replay alert.",
            },
            DashboardRegimeIndicator {
                key: "shift_score",
                label: "Shift score",
                value: 0.82,
                unit: "",
                status: "high",
                description: "Demo combined regime-shift heuristic score.",
            },
        ],
        validation: DashboardValidation {
            median_lead_time_ms: Some(250),
            p75_lead_time_ms: Some(250),
            precision: 1.0,
            recall: 0.333,
            degraded_confidence: true,
            reason: "5s and 30s horizons need more live evidence.",
            horizons: vec![
                DashboardValidationHorizon {
                    horizon_ms: 1_000,
                    pr_auc: 1.0,
                },
                DashboardValidationHorizon {
                    horizon_ms: 5_000,
                    pr_auc: 0.0,
                },
                DashboardValidationHorizon {
                    horizon_ms: 30_000,
                    pr_auc: 0.0,
                },
            ],
        },
    }
}

async fn get_current_regime(
    State(state): State<AppState>,
    Query(query): Query<AgentToolQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if let Some(store) = agent_tool_mongo_store(&state).await {
        match store.latest_regime_state(query.slug.as_deref()).await {
            Ok(Some(regime)) => {
                return Ok(Json(serde_json::json!({
                    "source": "mongodb",
                    "regime": regime,
                })));
            }
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(?error, "read current regime from MongoDB failed");
            }
        }
    }

    Ok(Json(serde_json::json!({
        "source": "sample",
        "regime": sample_agent_regime(query.slug.as_deref()),
    })))
}

async fn query_recent_alerts(
    State(state): State<AppState>,
    Query(query): Query<AgentToolQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let limit = bounded_limit(query.limit, 10, 50);
    if let Some(store) = agent_tool_mongo_store(&state).await {
        match store.recent_alerts(query.slug.as_deref(), limit).await {
            Ok(alerts) if !alerts.is_empty() => {
                return Ok(Json(serde_json::json!({
                    "source": "mongodb",
                    "alerts": alerts,
                })));
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(?error, "read recent alerts from MongoDB failed");
            }
        }
    }

    Ok(Json(serde_json::json!({
        "source": "sample",
        "alerts": sample_agent_alerts(query.slug.as_deref()),
    })))
}

async fn find_similar_windows(
    State(state): State<AppState>,
    Json(request): Json<FindSimilarWindowsRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if request.query_vector.len() != 5 {
        return Err((
            StatusCode::BAD_REQUEST,
            "query_vector must contain exactly 5 values".to_string(),
        ));
    }

    let limit = request.limit.unwrap_or(5).clamp(1, 25);
    if let Some(store) = agent_tool_mongo_store(&state).await {
        match store
            .find_similar_windows(&request.slug, &request.query_vector, limit)
            .await
        {
            Ok(windows) if !windows.is_empty() => {
                return Ok(Json(serde_json::json!({
                    "source": "mongodb",
                    "windows": windows,
                })));
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(?error, "read similar windows from MongoDB failed");
            }
        }
    }

    Ok(Json(serde_json::json!({
        "source": "sample",
        "windows": sample_similar_windows(&request.slug),
    })))
}

async fn get_backtest_metrics(
    State(state): State<AppState>,
    Query(query): Query<AgentToolQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let limit = bounded_limit(query.limit, 1, 10);
    if let Some(store) = agent_tool_mongo_store(&state).await {
        match store.recent_backtest_runs(limit).await {
            Ok(runs) if !runs.is_empty() => {
                return Ok(Json(serde_json::json!({
                    "source": "mongodb",
                    "runs": runs,
                })));
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(?error, "read backtest runs from MongoDB failed");
            }
        }
    }

    Ok(Json(serde_json::json!({
        "source": "sample",
        "runs": sample_backtest_runs(),
    })))
}

async fn generate_market_summary(
    State(state): State<AppState>,
    Query(_query): Query<AgentToolQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if let Some(store) = agent_tool_mongo_store(&state).await {
        match store.latest_agent_summary().await {
            Ok(Some(summary)) => {
                return Ok(Json(serde_json::json!({
                    "source": "mongodb",
                    "summary": summary,
                    "generated_now": false,
                })));
            }
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(?error, "read agent summary from MongoDB failed");
            }
        }
    }

    Ok(Json(serde_json::json!({
        "source": "sample",
        "summary": sample_agent_summary(),
        "generated_now": false,
    })))
}

async fn explain_now(State(state): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    let throttle = state.manual_explain.throttle;
    if !throttle.enabled {
        return (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "disabled",
                "reason": "gemini_disabled",
                "generated_now": false,
                "cooldown_seconds": throttle.manual_cooldown_seconds,
            })),
        );
    }

    let now_ms = unix_timestamp_ms();
    let reserve_result = reserve_manual_explain_call(&state.manual_explain, now_ms);
    if let Err(payload) = reserve_result {
        return payload;
    }

    match state.manual_explain.mode {
        ManualExplainMode::DryRun => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "generated",
                "source": "dry_run",
                "generated_now": true,
                "cooldown_seconds": throttle.manual_cooldown_seconds,
                "summary": sample_agent_summary(),
            })),
        ),
        ManualExplainMode::Generate => generate_manual_explain_response(&state, now_ms).await,
    }
}

fn reserve_manual_explain_call(
    runtime: &ManualExplainRuntime,
    now_ms: i64,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    let mut guard = runtime
        .state
        .lock()
        .expect("manual explain state lock is not poisoned");

    let calls_started_in_last_hour = runtime.call_budget.calls_started_in_last_hour(now_ms);
    if calls_started_in_last_hour >= runtime.throttle.max_calls_per_hour {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({
                "status": "rate_limited",
                "reason": "hourly_cap",
                "generated_now": false,
                "max_calls_per_hour": runtime.throttle.max_calls_per_hour,
            })),
        ));
    }

    let retry_after_seconds = runtime
        .throttle
        .manual_retry_after_seconds(now_ms, guard.last_started_at_ms)
        .unwrap_or(0);
    if retry_after_seconds > 0 {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({
                "status": "cooldown",
                "generated_now": false,
                "retry_after_seconds": retry_after_seconds,
                "cooldown_seconds": runtime.throttle.manual_cooldown_seconds,
            })),
        ));
    }

    if runtime.call_budget.reserve_manual_explain_call(
        &runtime.throttle,
        now_ms,
        guard.last_started_at_ms,
    ) {
        guard.last_started_at_ms = Some(now_ms);
        return Ok(());
    }

    Err((
        StatusCode::TOO_MANY_REQUESTS,
        Json(serde_json::json!({
            "status": "rate_limited",
            "reason": "hourly_cap",
            "generated_now": false,
            "max_calls_per_hour": runtime.throttle.max_calls_per_hour,
        })),
    ))
}

async fn generate_manual_explain_response(
    state: &AppState,
    now_ms: i64,
) -> (StatusCode, Json<serde_json::Value>) {
    let config = match gemini_summary::GeminiSummaryConfig::from_env() {
        Ok(config) => config,
        Err(error) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "status": "failed",
                    "reason": "invalid_gemini_config",
                    "generated_now": false,
                    "error": error,
                })),
            );
        }
    };

    let client = reqwest::Client::new();
    let state_for_prompt = gemini_summary::demo_summary_state(now_ms);
    let prompt = gemini_summary::build_summary_prompt(&state_for_prompt, 1);
    let summary = match gemini_summary::request_gemini_summary(&client, &config, &prompt).await {
        Ok(summary) => summary,
        Err(error) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "status": "failed",
                    "reason": "gemini_request_failed",
                    "generated_now": false,
                    "error": error.to_string(),
                })),
            );
        }
    };

    let bucket_seconds = (config.throttle.summary_interval_minutes * 60) as i64;
    let record = gemini_summary::summary_record(
        now_ms - (now_ms % (bucket_seconds * 1_000)),
        bucket_seconds,
        &config.model,
        &config.thinking_level,
        &summary,
        Vec::new(),
        Vec::new(),
        serde_json::json!({ "manual": true, "estimated": true }),
    );
    let fallback_path = PathBuf::from(
        std::env::var("GEMINI_SUMMARY_NDJSON_PATH")
            .unwrap_or_else(|_| "data/agent-summaries.ndjson".to_string()),
    );
    let store = agent_tool_mongo_store(state).await;
    if let Err(error) =
        gemini_summary::persist_agent_summary_or_fallback(store.as_ref(), &record, &fallback_path)
            .await
    {
        tracing::warn!(?error, "manual Gemini summary persistence failed");
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "generated",
            "source": "gemini",
            "generated_now": true,
            "cooldown_seconds": config.throttle.manual_cooldown_seconds,
            "summary": record,
        })),
    )
}

async fn agent_tool_mongo_store(state: &AppState) -> Option<mongo_store::MongoStore> {
    if !state.agent_tool_mongodb_enabled {
        return None;
    }

    let (Ok(uri), Ok(database_name)) = (std::env::var("MONGODB_URI"), std::env::var("MONGODB_DB"))
    else {
        return None;
    };

    let mut options = match mongodb::options::ClientOptions::parse(&uri).await {
        Ok(options) => options,
        Err(error) => {
            tracing::warn!(?error, "parse MongoDB URI for Agent tool failed");
            return None;
        }
    };
    options.server_selection_timeout = Some(agent_tool_mongodb_timeout_from_value(
        std::env::var("AGENT_TOOL_MONGODB_TIMEOUT_MS")
            .ok()
            .as_deref(),
    ));

    match mongodb::Client::with_options(options) {
        Ok(client) => Some(mongo_store::MongoStore::new(
            client.database(&database_name),
        )),
        Err(error) => {
            tracing::warn!(?error, "connect MongoDB for Agent tool failed");
            None
        }
    }
}

fn bounded_limit(limit: Option<i64>, default: i64, max: i64) -> i64 {
    limit.unwrap_or(default).clamp(1, max)
}

fn unix_timestamp_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn sample_agent_regime(slug: Option<&str>) -> serde_json::Value {
    serde_json::json!({
        "id": slug.unwrap_or("btc-updown-5m-1769000000"),
        "regime": "EARLY_RISK",
        "confidence": 0.82,
        "updated_at_ms": 1_769_000_000_750_i64,
        "previous_regime": "EQUILIBRIUM",
        "indicators": {
            "fair_gap": 0.05,
            "ofi_1s": 0.42,
            "depth_imbalance": 0.31,
            "stale_data_penalty": 0.0
        },
        "market_resolved": false
    })
}

fn sample_agent_alerts(slug: Option<&str>) -> Vec<serde_json::Value> {
    vec![serde_json::json!({
        "slug": slug.unwrap_or("btc-updown-5m-1769000000"),
        "created_at_ms": 1_769_000_000_750_i64,
        "severity": "warning",
        "state": "EARLY_RISK",
        "direction": "UP",
        "trigger": "fair_gap_velocity+order_flow",
        "message": "Fair probability gap and order-flow imbalance moved together.",
        "gemini_explained": false
    })]
}

fn sample_similar_windows(slug: &str) -> Vec<serde_json::Value> {
    vec![serde_json::json!({
        "slug": slug,
        "window_ts_ms": 1_769_000_000_750_i64,
        "window_ms": 1_000,
        "p_mid": 0.54,
        "p_fair": 0.49,
        "fair_gap": 0.05,
        "ofi_1s": 0.42,
        "depth_imbalance": 0.31,
        "spread": 0.03,
        "volume_acceleration": 2.1,
        "score": 0.98
    })]
}

fn sample_backtest_runs() -> Vec<serde_json::Value> {
    vec![serde_json::json!({
        "created_at_ms": 1_769_000_010_000_i64,
        "parameters": {
            "horizons_ms": [1000, 5000, 30000],
            "synchronous_tolerance_ms": 100
        },
        "data_range": {
            "markets": ["btc-updown-5m-1769000000"],
            "start_ms": 1_769_000_000_000_i64,
            "end_ms": 1_769_000_004_000_i64
        },
        "metrics": {
            "median_lead_time_ms": 250,
            "p75_lead_time_ms": 250,
            "precision": 1.0,
            "recall": 0.333,
            "horizon_pr_auc": [
                {"horizon_ms": 1000, "pr_auc": 1.0},
                {"horizon_ms": 5000, "pr_auc": 0.0},
                {"horizon_ms": 30000, "pr_auc": 0.0}
            ]
        },
        "ablation": []
    })]
}

fn sample_agent_summary() -> serde_json::Value {
    serde_json::json!({
        "bucket_start_ms": 1_769_000_000_000_i64,
        "bucket_seconds": 1_800,
        "model": "gemini-disabled-demo",
        "thinking_level": "LOW",
        "summary": "Cached demo summary: early risk increased because fair-gap velocity, order flow, and depth imbalance moved in the same direction.",
        "alert_ids": ["btc-updown-5m-1769000000:1769000000750"],
        "similar_window_ids": ["btc-updown-5m-1769000000:1769000000750"],
        "token_usage": {
            "input_tokens": 0,
            "output_tokens": 0
        }
    })
}

async fn validate_replay(
    Json(request): Json<ReplayValidationRequest>,
) -> Result<Json<ReplayValidationResponse>, (StatusCode, String)> {
    let alerts = replay_alerts(&request)?;
    let labels = generate_shift_labels(&request.price_points, &request.label_config);
    let report = validate_alerts_for_market(
        replay_market_slug(&request),
        &alerts,
        &labels,
        request.synchronous_tolerance_ms,
    );
    let ablation = replay_ablation(&request, &labels);

    Ok(Json(ReplayValidationResponse {
        alerts,
        labels,
        report,
        ablation,
    }))
}

fn replay_ablation(
    request: &ReplayValidationRequest,
    labels: &[ShiftLabel],
) -> Vec<AblationMetric> {
    let feature_windows = replay_feature_windows(request);
    let (Some(weights), Some(thresholds), Some(horizon_ms)) = (
        request.score_weights,
        request.score_thresholds,
        request.alert_horizon_ms,
    ) else {
        return Vec::new();
    };

    if feature_windows.is_empty() {
        return Vec::new();
    }

    ablation_report_from_feature_windows(
        &feature_windows,
        labels,
        &weights,
        &thresholds,
        horizon_ms,
        request.synchronous_tolerance_ms,
    )
}

fn replay_market_slug(request: &ReplayValidationRequest) -> &str {
    request
        .fair_probability_feature_windows
        .first()
        .map(|window| window.slug.as_str())
        .or_else(|| {
            request
                .feature_windows
                .first()
                .map(|window| window.slug.as_str())
        })
        .unwrap_or("replay")
}

fn replay_feature_windows(request: &ReplayValidationRequest) -> Vec<FeatureWindowRecord> {
    if !request.fair_probability_feature_windows.is_empty() {
        return request
            .fair_probability_feature_windows
            .iter()
            .map(build_feature_window_from_fair_probability_record)
            .collect();
    }

    request.feature_windows.clone()
}

fn replay_alerts(
    request: &ReplayValidationRequest,
) -> Result<Vec<AlertRecord>, (StatusCode, String)> {
    if !request.alerts.is_empty() {
        return Ok(request.alerts.clone());
    }

    let feature_windows = replay_feature_windows(request);
    if feature_windows.is_empty() {
        return Ok(Vec::new());
    }

    let Some(weights) = request.score_weights else {
        return Err((
            StatusCode::BAD_REQUEST,
            "score_weights are required when feature_windows are provided without alerts"
                .to_string(),
        ));
    };
    let Some(thresholds) = request.score_thresholds else {
        return Err((
            StatusCode::BAD_REQUEST,
            "score_thresholds are required when feature_windows are provided without alerts"
                .to_string(),
        ));
    };
    let Some(horizon_ms) = request.alert_horizon_ms else {
        return Err((
            StatusCode::BAD_REQUEST,
            "alert_horizon_ms is required when feature_windows are provided without alerts"
                .to_string(),
        ));
    };

    Ok(generate_alerts_from_feature_windows(
        &feature_windows,
        &weights,
        &thresholds,
        horizon_ms,
    ))
}
