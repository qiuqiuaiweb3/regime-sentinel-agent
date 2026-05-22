use regime_core::{
    CollectionKind, VectorSearchSpec, mongo_collection_names, mongo_index_specs,
    vector_search_specs,
};

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

#[test]
fn vector_search_specs_are_kept_separate_from_regular_indexes() {
    assert!(
        !mongo_index_specs()
            .iter()
            .any(|spec| spec.name == "feature_windows_feature_vector")
    );

    assert_eq!(
        vector_search_specs(),
        [VectorSearchSpec {
            collection: CollectionKind::FeatureWindows,
            name: "feature_windows_vector_search",
            path: "feature_vector",
            dimensions: 5,
            similarity: "cosine"
        }]
    );
}
