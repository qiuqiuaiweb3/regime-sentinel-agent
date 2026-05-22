use regime_core::{CollectionKind, mongo_collection_names, mongo_index_specs};

#[test]
fn mongo_collection_names_match_plan() {
    assert_eq!(
        mongo_collection_names(),
        [
            "market_ticks",
            "feature_windows",
            "regime_states",
            "alerts",
            "agent_summaries",
            "backtest_runs"
        ]
    );
}

#[test]
fn mongo_index_specs_cover_hot_path_and_validation_collections() {
    let specs = mongo_index_specs();

    assert!(specs.iter().any(|spec| {
        spec.collection == CollectionKind::MarketTicks
            && spec.name == "market_ticks_slug_timestamp"
            && spec.fields == ["meta.slug", "timestamp"]
            && spec.ttl_seconds == Some(604_800)
    }));
    assert!(specs.iter().any(|spec| {
        spec.collection == CollectionKind::FeatureWindows
            && spec.name == "feature_windows_slug_window_ts"
            && spec.fields == ["slug", "window_ts"]
            && spec.unique
    }));
    assert!(specs.iter().any(|spec| {
        spec.collection == CollectionKind::Alerts
            && spec.name == "alerts_slug_created_at"
            && spec.fields == ["slug", "created_at"]
    }));
    assert!(specs.iter().any(|spec| {
        spec.collection == CollectionKind::BacktestRuns
            && spec.name == "backtest_runs_created_at"
            && spec.fields == ["created_at"]
    }));
}
