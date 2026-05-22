use axum::{
    Json, Router,
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    routing::{get, post},
};
use futures_util::stream;
use regime_core::{
    AlertRecord, FeatureWindowRecord, PricePoint, ScoreThresholds, ScoreWeights, ShiftLabel,
    ShiftLabelConfig, ValidationReport, generate_alerts_from_feature_windows,
    generate_shift_labels, validate_alerts,
};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::time::Duration;
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

    pub fn agent_summary_insert(summary: &AgentSummaryRecord) -> MongoInsertDocument {
        MongoInsertDocument {
            collection_name: "agent_summaries",
            document: agent_summary_document(summary),
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
    use mongodb::{Database, bson::Document};
    use regime_core::{
        AgentSummaryRecord, AlertEventRecord, BacktestRunRecord, FeatureWindowRecord,
        MarketTickRecord, RegimeStateRecord,
    };

    use crate::mongo_documents::{
        agent_summary_insert, alert_insert, backtest_run_insert, feature_window_insert,
        market_tick_insert, regime_state_upsert,
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

pub mod live_collector {
    use anyhow::Context;
    use chrono::DateTime;
    use futures_util::{SinkExt, StreamExt};
    use regime_core::{MarketTickMeta, MarketTickRecord, RegimeStateRecord};
    use serde::Serialize;
    use serde_json::{Value, json};
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    use tokio_tungstenite::{connect_async, tungstenite::Message};

    pub const DEFAULT_MARKET_WS_URL: &str = "wss://ws-subscriptions-clob.polymarket.com/ws/market";
    pub const DEFAULT_REFERENCE_WS_URL: &str = "wss://ws-feed.exchange.coinbase.com";
    pub const DEFAULT_REFERENCE_PRODUCT_ID: &str = "BTC-USD";
    pub const DEFAULT_STALE_AFTER_MS: i64 = 1_500;

    #[derive(Debug, Clone, PartialEq)]
    pub struct LiveCollectorConfig {
        pub enabled: bool,
        pub slug: String,
        pub series: String,
        pub asset_ids: Vec<String>,
        pub outcomes: Vec<String>,
        pub market_ws_url: String,
        pub reference_ws_url: String,
        pub reference_product_id: String,
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
                    slug: "disabled".to_string(),
                    series: "disabled".to_string(),
                    asset_ids: Vec::new(),
                    outcomes: Vec::new(),
                    market_ws_url: market_ws_url.unwrap_or(DEFAULT_MARKET_WS_URL).to_string(),
                    reference_ws_url: DEFAULT_REFERENCE_WS_URL.to_string(),
                    reference_product_id: DEFAULT_REFERENCE_PRODUCT_ID.to_string(),
                    ndjson_path: PathBuf::from(ndjson_path.unwrap_or("data/live-fallback.ndjson")),
                    stale_after_ms,
                });
            }

            let slug = required_string(slug, "LIVE_MARKET_SLUG")?;
            let series = required_string(series, "LIVE_MARKET_SERIES")?;
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
                slug: slug.to_string(),
                series: series.to_string(),
                asset_ids,
                outcomes,
                market_ws_url: market_ws_url.unwrap_or(DEFAULT_MARKET_WS_URL).to_string(),
                reference_ws_url: DEFAULT_REFERENCE_WS_URL.to_string(),
                reference_product_id: DEFAULT_REFERENCE_PRODUCT_ID.to_string(),
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
            config.reference_product_id = std::env::var("REFERENCE_PRICE_PRODUCT_ID")
                .unwrap_or_else(|_| DEFAULT_REFERENCE_PRODUCT_ID.to_string());
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
                "type": "subscribe",
                "product_ids": [self.reference_product_id],
                "channels": ["ticker", "heartbeat"]
            })
        }
    }

    pub fn market_ticks_from_message(
        payload: &str,
        meta: &LiveMarketMeta,
        received_at_ms: i64,
    ) -> Result<Vec<MarketTickRecord>, String> {
        let value: Value = serde_json::from_str(payload)
            .map_err(|error| format!("invalid websocket JSON: {error}"))?;
        market_ticks_from_value(&value, meta, received_at_ms)
    }

    pub fn reference_tick_from_coinbase_message(
        payload: &str,
        config: &LiveCollectorConfig,
        received_at_ms: i64,
    ) -> Result<Option<MarketTickRecord>, String> {
        let value: Value = serde_json::from_str(payload)
            .map_err(|error| format!("invalid reference JSON: {error}"))?;
        if value.get("type").and_then(Value::as_str) != Some("ticker") {
            return Ok(None);
        }
        let product_id = string_field(&value, "product_id")?;
        if product_id != config.reference_product_id {
            return Ok(None);
        }
        let timestamp_ms = value
            .get("time")
            .and_then(Value::as_str)
            .and_then(parse_rfc3339_ms)
            .unwrap_or(received_at_ms);

        Ok(Some(MarketTickRecord {
            timestamp_ms,
            meta: MarketTickMeta {
                slug: config.slug.clone(),
                series: config.series.clone(),
                source: "coinbase".to_string(),
            },
            price: f64_field(&value, "price")?,
            size: 0.0,
            side: "TICKER".to_string(),
            outcome: product_id.to_string(),
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
        let Some(store) = store else {
            return append_ndjson_fallback(fallback_path, "market_tick", tick);
        };

        if let Err(error) = store.insert_market_tick(tick).await {
            tracing::warn!(
                ?error,
                "market tick MongoDB write failed; using NDJSON fallback"
            );
            append_ndjson_fallback(fallback_path, "market_tick", tick)?;
        }

        Ok(())
    }

    pub async fn persist_regime_state_or_fallback(
        store: Option<&crate::mongo_store::MongoStore>,
        state: &RegimeStateRecord,
        fallback_path: &Path,
    ) -> anyhow::Result<()> {
        let Some(store) = store else {
            return append_ndjson_fallback(fallback_path, "regime_state", state);
        };

        if let Err(error) = store.upsert_regime_state(state).await {
            tracing::warn!(
                ?error,
                "regime state MongoDB write failed; using NDJSON fallback"
            );
            append_ndjson_fallback(fallback_path, "regime_state", state)?;
        }

        Ok(())
    }

    pub async fn run_live_collector(
        config: LiveCollectorConfig,
        store: Option<crate::mongo_store::MongoStore>,
    ) -> anyhow::Result<()> {
        if !config.enabled {
            return Ok(());
        }

        let meta = config.market_meta();
        let mut last_event_timestamp_ms = None;

        loop {
            let ws_stream = match connect_async(&config.market_ws_url).await {
                Ok((ws_stream, _)) => ws_stream,
                Err(error) => {
                    tracing::warn!(?error, "connect Polymarket market websocket failed");
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    continue;
                }
            };

            let (mut write, mut read) = ws_stream.split();
            write
                .send(Message::Text(
                    config.subscription_message().to_string().into(),
                ))
                .await
                .context("send Polymarket market subscription")?;
            let mut heartbeat = tokio::time::interval(Duration::from_secs(10));

            loop {
                tokio::select! {
                    _ = heartbeat.tick() => {
                        write
                            .send(Message::Text("PING".into()))
                            .await
                            .context("send Polymarket market heartbeat")?;
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

    pub async fn run_reference_price_collector(
        config: LiveCollectorConfig,
        store: Option<crate::mongo_store::MongoStore>,
    ) -> anyhow::Result<()> {
        if !config.enabled {
            return Ok(());
        }

        loop {
            let ws_stream = match connect_async(&config.reference_ws_url).await {
                Ok((ws_stream, _)) => ws_stream,
                Err(error) => {
                    tracing::warn!(?error, "connect reference price websocket failed");
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    continue;
                }
            };

            let (mut write, mut read) = ws_stream.split();
            write
                .send(Message::Text(
                    config.reference_subscription_message().to_string().into(),
                ))
                .await
                .context("send reference price subscription")?;

            while let Some(message) = read.next().await {
                match message {
                    Ok(Message::Text(text)) => {
                        handle_reference_message(
                            text.as_ref(),
                            &config,
                            store.as_ref(),
                            &config.ndjson_path,
                        )
                        .await;
                    }
                    Ok(Message::Binary(bytes)) => {
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
                    Ok(Message::Ping(payload)) => {
                        write
                            .send(Message::Pong(payload))
                            .await
                            .context("send reference websocket pong")?;
                    }
                    Ok(Message::Close(close)) => {
                        tracing::warn!(?close, "reference price websocket closed");
                        break;
                    }
                    Ok(_) => {}
                    Err(error) => {
                        tracing::warn!(?error, "reference price websocket read failed");
                        break;
                    }
                }
            }

            tokio::time::sleep(Duration::from_secs(3)).await;
        }
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

    async fn handle_market_message(
        payload: &str,
        meta: &LiveMarketMeta,
        store: Option<&crate::mongo_store::MongoStore>,
        fallback_path: &Path,
        last_event_timestamp_ms: &mut Option<i64>,
    ) {
        match market_ticks_from_message(payload, meta, unix_timestamp_ms()) {
            Ok(ticks) => {
                for tick in ticks {
                    *last_event_timestamp_ms = Some(tick.timestamp_ms);
                    if let Err(error) =
                        persist_market_tick_or_fallback(store, &tick, fallback_path).await
                    {
                        tracing::warn!(?error, "persist market tick failed");
                    }
                }
            }
            Err(error) => {
                tracing::warn!(%error, "parse Polymarket market websocket message failed");
            }
        }
    }

    async fn handle_reference_message(
        payload: &str,
        config: &LiveCollectorConfig,
        store: Option<&crate::mongo_store::MongoStore>,
        fallback_path: &Path,
    ) {
        match reference_tick_from_coinbase_message(payload, config, unix_timestamp_ms()) {
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

    fn parse_rfc3339_ms(value: &str) -> Option<i64> {
        DateTime::parse_from_rfc3339(value)
            .map(|timestamp| timestamp.timestamp_millis())
            .ok()
    }

    fn string_field<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
        value
            .get(field)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("missing string field {field}"))
    }

    fn f64_field(value: &Value, field: &str) -> Result<f64, String> {
        match value.get(field) {
            Some(Value::String(raw)) => raw
                .parse::<f64>()
                .map_err(|_| format!("invalid float field {field}")),
            Some(Value::Number(raw)) => raw
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
}

#[derive(Debug, Serialize)]
pub struct DashboardSnapshot {
    regime: DashboardRegime,
    price_points: Vec<DashboardPricePoint>,
    alerts: Vec<DashboardAlert>,
    gemini_summary: DashboardGeminiSummary,
}

#[derive(Debug, Serialize)]
pub struct DashboardRegime {
    state: &'static str,
    confidence: &'static str,
    updated_at_ms: i64,
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
    summary: &'static str,
}

pub fn build_router() -> Router {
    let static_dir =
        PathBuf::from(std::env::var("REGIME_STATIC_DIR").unwrap_or_else(|_| "build".to_string()));

    if static_dir.join("index.html").exists() {
        return build_router_with_static_dir(static_dir);
    }

    build_api_router()
}

pub fn build_router_with_static_dir(static_dir: impl AsRef<Path>) -> Router {
    let static_dir = static_dir.as_ref().to_path_buf();
    let index_file = static_dir.join("index.html");

    build_api_router()
        .fallback_service(ServeDir::new(static_dir).fallback(ServeFile::new(index_file)))
}

fn build_api_router() -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/dashboard/snapshot", get(dashboard_snapshot))
        .route("/api/dashboard/events", get(dashboard_events))
        .route("/api/replay/validate", post(validate_replay))
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "regime-service",
    })
}

async fn dashboard_snapshot() -> Json<DashboardSnapshot> {
    Json(sample_dashboard_snapshot())
}

async fn dashboard_events() -> Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>> {
    let stream = stream::unfold(
        tokio::time::interval(Duration::from_secs(1)),
        |mut interval| async {
            interval.tick().await;
            let data = serde_json::to_string(&sample_dashboard_snapshot())
                .expect("dashboard snapshot serializes");
            Some((Ok(Event::default().event("snapshot").data(data)), interval))
        },
    );

    Sse::new(stream).keep_alive(KeepAlive::default())
}

fn sample_dashboard_snapshot() -> DashboardSnapshot {
    DashboardSnapshot {
        regime: DashboardRegime {
            state: "EARLY_RISK",
            confidence: "Normal",
            updated_at_ms: 1_769_000_000_750,
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
            enabled: false,
            generated_at_ms: None,
            summary: "Gemini summaries are disabled by default.",
        },
    }
}

async fn validate_replay(
    Json(request): Json<ReplayValidationRequest>,
) -> Result<Json<ReplayValidationResponse>, (StatusCode, String)> {
    let alerts = replay_alerts(&request)?;
    let labels = generate_shift_labels(&request.price_points, &request.label_config);
    let report = validate_alerts(&alerts, &labels, request.synchronous_tolerance_ms);

    Ok(Json(ReplayValidationResponse {
        alerts,
        labels,
        report,
    }))
}

fn replay_alerts(
    request: &ReplayValidationRequest,
) -> Result<Vec<AlertRecord>, (StatusCode, String)> {
    if !request.alerts.is_empty() {
        return Ok(request.alerts.clone());
    }

    if request.feature_windows.is_empty() {
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
        &request.feature_windows,
        &weights,
        &thresholds,
        horizon_ms,
    ))
}
