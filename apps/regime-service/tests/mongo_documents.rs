use mongodb::bson::Bson;
use regime_core::{
    AgentSummaryRecord, AlertEventRecord, BacktestRunRecord, FeatureWindowMetrics, MarketTickMeta,
    MarketTickRecord, RegimeStateRecord, build_feature_window,
};

#[test]
fn feature_window_document_matches_mongodb_schema_fields() {
    let window = build_feature_window(
        "btc-updown-5m",
        FeatureWindowMetrics {
            window_ts_ms: 1_769_000_000_000,
            window_ms: 1_000,
            p_mid: 0.52,
            p_fair: 0.49,
            ofi_1s: 0.42,
            depth_imbalance: 0.31,
            spread: 0.03,
            volume_acceleration: 2.1,
        },
    );

    let document = regime_service::mongo_documents::feature_window_document(&window);

    assert_eq!(document.get_str("slug"), Ok("btc-updown-5m"));
    assert_eq!(
        document
            .get_datetime("window_ts")
            .map(|value| value.timestamp_millis()),
        Ok(1_769_000_000_000)
    );
    assert!(!document.contains_key("window_ts_ms"));
    assert_eq!(document.get_i32("window_ms"), Ok(1_000));
    assert!((document.get_f64("fair_gap").unwrap() - 0.03).abs() < 1e-12);

    let feature_vector = document
        .get_array("feature_vector")
        .expect("feature_vector");
    assert_eq!(feature_vector.len(), 5);
    assert_eq!(feature_vector[1], Bson::Double(0.42));
    assert_eq!(feature_vector[4], Bson::Double(2.1));
}

#[test]
fn feature_window_insert_targets_feature_windows_collection() {
    let window = build_feature_window(
        "btc-updown-5m",
        FeatureWindowMetrics {
            window_ts_ms: 1_769_000_000_000,
            window_ms: 1_000,
            p_mid: 0.52,
            p_fair: 0.49,
            ofi_1s: 0.42,
            depth_imbalance: 0.31,
            spread: 0.03,
            volume_acceleration: 2.1,
        },
    );

    let insert = regime_service::mongo_documents::feature_window_insert(&window);

    assert_eq!(insert.collection_name, "feature_windows");
    assert_eq!(insert.document.get_str("slug"), Ok("btc-updown-5m"));
}

#[test]
fn market_tick_insert_matches_time_series_schema() {
    let tick = MarketTickRecord {
        timestamp_ms: 1_769_000_000_123,
        meta: MarketTickMeta {
            slug: "btc-updown-5m".to_string(),
            series: "btc-updown-5m".to_string(),
            source: "clob".to_string(),
        },
        price: 0.52,
        size: 100.0,
        side: "BUY".to_string(),
        outcome: "UP".to_string(),
        receive_lag_ms: 120,
    };

    let insert = regime_service::mongo_documents::market_tick_insert(&tick);

    assert_eq!(insert.collection_name, "market_ticks");
    assert_eq!(
        insert
            .document
            .get_datetime("timestamp")
            .map(|value| value.timestamp_millis()),
        Ok(1_769_000_000_123)
    );
    assert_eq!(
        insert
            .document
            .get_document("meta")
            .and_then(|meta| meta.get_str("slug")),
        Ok("btc-updown-5m")
    );
    assert_eq!(insert.document.get_f64("price"), Ok(0.52));
    assert_eq!(insert.document.get_f64("size"), Ok(100.0));
    assert_eq!(insert.document.get_str("side"), Ok("BUY"));
    assert_eq!(insert.document.get_i64("receive_lag_ms"), Ok(120));
}

#[test]
fn regime_state_upsert_matches_latest_state_schema() {
    let state = RegimeStateRecord {
        id: "btc-updown-5m".to_string(),
        regime: "EARLY_SHIFT_RISK".to_string(),
        confidence: 0.71,
        updated_at_ms: 1_769_000_000_456,
        previous_regime: "EQUILIBRIUM".to_string(),
        indicators: serde_json::json!({"ofi_1s": 0.42}),
        market_resolved: false,
    };

    let upsert = regime_service::mongo_documents::regime_state_upsert(&state);

    assert_eq!(upsert.collection_name, "regime_states");
    assert!(upsert.upsert);
    assert_eq!(upsert.filter.get_str("_id"), Ok("btc-updown-5m"));

    let set = upsert.update.get_document("$set").expect("$set document");
    assert_eq!(set.get_str("regime"), Ok("EARLY_SHIFT_RISK"));
    assert_eq!(set.get_f64("confidence"), Ok(0.71));
    assert_eq!(
        set.get_datetime("updated_at")
            .map(|value| value.timestamp_millis()),
        Ok(1_769_000_000_456)
    );
    assert_eq!(
        set.get_document("indicators")
            .and_then(|indicators| indicators.get_f64("ofi_1s")),
        Ok(0.42)
    );
    assert_eq!(set.get_bool("market_resolved"), Ok(false));
}

#[test]
fn alert_insert_matches_alert_schema() {
    let alert = AlertEventRecord {
        slug: "btc-updown-5m".to_string(),
        created_at_ms: 1_769_000_000_789,
        severity: "HIGH".to_string(),
        state: "EARLY_RISK".to_string(),
        direction: "UP".to_string(),
        trigger: "fair_gap_velocity+ofi_1s".to_string(),
        message: "Up-side pressure rising before price fully reprices".to_string(),
        gemini_explained: false,
    };

    let insert = regime_service::mongo_documents::alert_insert(&alert);

    assert_eq!(insert.collection_name, "alerts");
    assert_eq!(insert.document.get_str("slug"), Ok("btc-updown-5m"));
    assert_eq!(
        insert
            .document
            .get_datetime("created_at")
            .map(|value| value.timestamp_millis()),
        Ok(1_769_000_000_789)
    );
    assert_eq!(insert.document.get_str("severity"), Ok("HIGH"));
    assert_eq!(insert.document.get_str("state"), Ok("EARLY_RISK"));
    assert_eq!(insert.document.get_bool("gemini_explained"), Ok(false));
}

#[test]
fn agent_summary_insert_matches_summary_schema() {
    let summary = AgentSummaryRecord {
        bucket_start_ms: 1_769_000_000_000,
        bucket_seconds: 1_800,
        model: "gemini-3-flash-preview".to_string(),
        thinking_level: "LOW".to_string(),
        summary: "Risk rose but no confirmed shift.".to_string(),
        alert_ids: vec!["alert-1".to_string()],
        similar_window_ids: vec!["window-1".to_string()],
        token_usage: serde_json::json!({"input": 100, "output": 40}),
    };

    let insert = regime_service::mongo_documents::agent_summary_insert(&summary);

    assert_eq!(insert.collection_name, "agent_summaries");
    assert_eq!(
        insert
            .document
            .get_datetime("bucket_start")
            .map(|value| value.timestamp_millis()),
        Ok(1_769_000_000_000)
    );
    assert_eq!(insert.document.get_i32("bucket_seconds"), Ok(1_800));
    assert_eq!(
        insert.document.get_str("model"),
        Ok("gemini-3-flash-preview")
    );
    assert_eq!(
        insert
            .document
            .get_document("token_usage")
            .and_then(|usage| usage.get_i64("output")),
        Ok(40)
    );
}

#[test]
fn agent_summary_bucket_upsert_overwrites_the_scheduled_bucket() {
    let summary = AgentSummaryRecord {
        bucket_start_ms: 1_769_000_000_000,
        bucket_seconds: 1_800,
        model: "gemini-3-flash-preview".to_string(),
        thinking_level: "LOW".to_string(),
        summary: "Fresh scheduled summary replaces the previous response.".to_string(),
        alert_ids: Vec::new(),
        similar_window_ids: Vec::new(),
        token_usage: serde_json::json!({"estimated": true}),
    };

    let update = regime_service::mongo_documents::agent_summary_bucket_upsert(&summary);

    assert_eq!(update.collection_name, "agent_summaries");
    assert_eq!(
        update
            .filter
            .get_datetime("bucket_start")
            .map(|value| value.timestamp_millis()),
        Ok(1_769_000_000_000)
    );
    assert_eq!(
        update
            .update
            .get_document("$set")
            .unwrap()
            .get_str("summary"),
        Ok("Fresh scheduled summary replaces the previous response.")
    );
    assert!(update.upsert);
}

#[test]
fn market_data_retention_filters_delete_related_slug_data() {
    let slugs = vec!["btc-updown-5m-1".to_string(), "btc-updown-5m-2".to_string()];

    let deletes = regime_service::mongo_documents::market_data_retention_deletes(&slugs);

    let collections = deletes
        .iter()
        .map(|delete| delete.collection_name)
        .collect::<Vec<_>>();
    assert_eq!(
        collections,
        vec!["market_ticks", "feature_windows", "alerts", "regime_states"]
    );
    assert_eq!(
        deletes[0].filter,
        mongodb::bson::doc! { "meta.slug": { "$in": &slugs } }
    );
    assert_eq!(
        deletes[3].filter,
        mongodb::bson::doc! { "_id": { "$in": &slugs } }
    );
}

#[test]
fn backtest_run_insert_matches_validation_schema() {
    let run = BacktestRunRecord {
        created_at_ms: 1_769_000_000_999,
        parameters: serde_json::json!({"watch": 0.5, "early_risk": 1.0}),
        data_range: serde_json::json!({"start_ms": 1, "end_ms": 2}),
        metrics: serde_json::json!({"early": 2, "false_alerts": 1}),
        ablation: serde_json::json!({"without_ofi_1s": {"early": 1}}),
    };

    let insert = regime_service::mongo_documents::backtest_run_insert(&run);

    assert_eq!(insert.collection_name, "backtest_runs");
    assert_eq!(
        insert
            .document
            .get_datetime("created_at")
            .map(|value| value.timestamp_millis()),
        Ok(1_769_000_000_999)
    );
    assert_eq!(
        insert
            .document
            .get_document("metrics")
            .and_then(|metrics| metrics.get_i64("early")),
        Ok(2)
    );
}
