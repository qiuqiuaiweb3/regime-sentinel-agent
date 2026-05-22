#[test]
fn demo_seed_prepares_acceptance_records_for_each_collection() {
    let records = regime_service::demo_seed::demo_seed_records_at(1_769_000_000_750, "run-1");

    assert_eq!(
        records.collection_names(),
        [
            "market_ticks",
            "feature_windows",
            "regime_states",
            "alerts",
            "agent_summaries",
            "backtest_runs",
        ]
    );
    assert_eq!(records.market_tick.meta.slug, "btc-updown-5m-demo");
    assert_eq!(records.feature_window.slug, "btc-updown-5m-demo");
    assert_eq!(records.regime_state.regime, "EARLY_RISK");
    assert_eq!(records.alert.created_at_ms, 1_769_000_000_750);
    assert_eq!(records.agent_summary.bucket_seconds, 1_800);
    assert_eq!(records.agent_summary.bucket_start_ms, 1_769_000_000_750);
    assert_eq!(
        records.backtest_run.metrics["median_lead_time_ms"],
        serde_json::json!(250.0)
    );
}

#[test]
fn demo_seed_count_queries_cover_each_seeded_collection() {
    let queries = regime_service::demo_seed::demo_seed_count_queries("run-1");
    let collection_names = queries
        .iter()
        .map(|query| query.collection_name)
        .collect::<Vec<_>>();

    assert_eq!(
        collection_names,
        [
            "market_ticks",
            "feature_windows",
            "regime_states",
            "alerts",
            "agent_summaries",
            "backtest_runs",
        ]
    );
    assert_eq!(queries[0].filter.get_str("demo_run_id"), Ok("run-1"));
    assert_eq!(
        queries[2].filter.get_str("indicators.demo_run_id"),
        Ok("run-1")
    );
}

#[test]
fn demo_seed_count_validation_rejects_missing_collections() {
    let counts = vec![
        regime_service::demo_seed::DemoSeedCount {
            collection_name: "market_ticks",
            count: 1,
        },
        regime_service::demo_seed::DemoSeedCount {
            collection_name: "feature_windows",
            count: 0,
        },
    ];

    let error = regime_service::demo_seed::validate_demo_seed_counts(&counts)
        .expect_err("zero count should fail verification");

    assert!(error.contains("feature_windows"));
}
