#[test]
fn bootstrap_plan_skips_existing_collections_but_keeps_index_targets() {
    let plan = regime_service::mongo_bootstrap::mongo_bootstrap_plan([
        "market_ticks".to_string(),
        "alerts".to_string(),
    ]);

    assert_eq!(
        plan.collections_to_create,
        vec![
            "feature_windows",
            "regime_states",
            "agent_summaries",
            "backtest_runs"
        ]
    );

    assert_eq!(plan.regular_indexes_to_create.len(), 6);
    assert!(plan.regular_indexes_to_create.iter().any(|target| {
        target.collection_name == "market_ticks"
            && target.index_name == "market_ticks_slug_timestamp"
    }));

    assert_eq!(plan.vector_search_indexes_to_create.len(), 1);
    assert_eq!(
        plan.vector_search_indexes_to_create[0].collection_name,
        "feature_windows"
    );
    assert_eq!(
        plan.vector_search_indexes_to_create[0].index_name,
        "feature_windows_vector_search"
    );
}

#[test]
fn bootstrap_summary_counts_planned_resources() {
    let plan = regime_service::mongo_bootstrap::mongo_bootstrap_plan(Vec::new());
    let summary = regime_service::mongo_bootstrap::MongoBootstrapSummary::from_plan(&plan);

    assert_eq!(summary.collections_created, 6);
    assert_eq!(summary.regular_indexes_requested, 6);
    assert_eq!(summary.vector_search_indexes_requested, 1);
}
