use regime_service::live_collector::{
    LiveCollectorConfig, append_ndjson_fallback, live_smoke_passed, market_ticks_from_message,
    midpoint_tick_from_midpoint_response, midpoint_ticks_from_midpoints_response,
    parse_gamma_event_market, persist_market_tick_or_fallback,
    reference_tick_from_chainlink_message, stale_data_penalty, stale_regime_state,
    summarize_live_smoke_ndjson, target_window_start_seconds,
};
use serde_json::Value;

#[test]
fn disabled_live_collector_does_not_require_market_env() {
    let config = LiveCollectorConfig::from_env_values(
        None,
        None,
        None,
        None,
        None,
        None,
        Some("/tmp/regime-fallback.ndjson"),
        None,
    )
    .expect("disabled collector config");

    assert!(!config.enabled);
}

#[test]
fn live_collector_config_builds_polymarket_market_subscription() {
    let config = LiveCollectorConfig::from_env_values(
        Some("true"),
        Some("btc-updown-5m-1769000000"),
        Some("btc-updown-5m"),
        Some("yes-token,no-token"),
        Some("UP,DOWN"),
        None,
        Some("/tmp/regime-fallback.ndjson"),
        Some("1500"),
    )
    .expect("collector config");

    let subscription = config.subscription_message();
    assert!(config.enabled);
    assert_eq!(config.stale_after_ms, 1500);
    assert_eq!(
        subscription,
        serde_json::json!({
            "assets_ids": ["yes-token", "no-token"],
            "type": "market",
            "custom_feature_enabled": true
        })
    );
}

#[test]
fn live_collector_auto_discovery_does_not_require_static_asset_ids() {
    let config = LiveCollectorConfig::from_env_values(
        Some("true"),
        Some("auto"),
        Some("btc-updown-5m"),
        None,
        None,
        None,
        Some("/tmp/regime-fallback.ndjson"),
        Some("1500"),
    )
    .expect("auto collector config");

    assert!(config.enabled);
    assert!(config.auto_discovery);
    assert_eq!(config.slug, "auto");
    assert_eq!(config.series, "btc-updown-5m");
    assert!(config.asset_ids.is_empty());
}

#[test]
fn target_window_start_seconds_uses_five_minute_grid() {
    assert_eq!(
        target_window_start_seconds(1_779_768_404, 300),
        1_779_768_300
    );
    assert_eq!(
        target_window_start_seconds(1_779_768_511, 300),
        1_779_768_300
    );
}

#[test]
fn gamma_event_market_parser_extracts_active_clob_tokens() {
    let market = parse_gamma_event_market(
        "btc-updown-5m-1779768300",
        "btc-updown-5m",
        &serde_json::json!({
            "markets": [
                {
                    "active": true,
                    "closed": false,
                    "clobTokenIds": "[\"yes-token\", \"no-token\"]",
                    "outcomes": "[\"Up\", \"Down\"]"
                }
            ]
        }),
        1_779_768_300,
        300,
    )
    .expect("parsed market");

    assert_eq!(market.slug, "btc-updown-5m-1779768300");
    assert_eq!(market.series, "btc-updown-5m");
    assert_eq!(market.asset_ids, vec!["yes-token", "no-token"]);
    assert_eq!(market.outcomes, vec!["UP", "DOWN"]);
    assert_eq!(market.window_start_s, 1_779_768_300);
    assert_eq!(market.window_end_s, 1_779_768_600);
}

#[test]
fn live_collector_config_builds_chainlink_reference_subscription() {
    let config = LiveCollectorConfig::from_env_values(
        Some("true"),
        Some("btc-updown-5m-1769000000"),
        Some("btc-updown-5m"),
        Some("yes-token,no-token"),
        Some("UP,DOWN"),
        None,
        Some("/tmp/regime-fallback.ndjson"),
        Some("1500"),
    )
    .expect("collector config");

    assert_eq!(
        config.reference_subscription_message(),
        serde_json::json!({
            "action": "subscribe",
            "subscriptions": [
                {
                    "topic": "crypto_prices_chainlink",
                    "type": "*",
                    "filters": "{\"symbol\":\"btc/usd\"}"
                }
            ]
        })
    );
}

#[test]
fn live_collector_config_derives_role_specific_fallback_paths() {
    let config = LiveCollectorConfig::from_env_values(
        Some("true"),
        Some("btc-updown-5m-1769000000"),
        Some("btc-updown-5m"),
        Some("yes-token,no-token"),
        Some("UP,DOWN"),
        None,
        Some("/tmp/regime-fallback.ndjson"),
        Some("1500"),
    )
    .expect("collector config");

    assert_eq!(
        config.ndjson_path_for_role("market"),
        std::path::PathBuf::from("/tmp/regime-fallback.market.ndjson")
    );
    assert_eq!(
        config.ndjson_path_for_role("reference"),
        std::path::PathBuf::from("/tmp/regime-fallback.reference.ndjson")
    );
    assert_eq!(
        config
            .clone()
            .with_ndjson_path(config.ndjson_path_for_role("market"))
            .ndjson_path,
        std::path::PathBuf::from("/tmp/regime-fallback.market.ndjson")
    );
}

#[test]
fn market_ticks_from_price_change_preserves_asset_outcome_and_receive_lag() {
    let config = LiveCollectorConfig::from_env_values(
        Some("true"),
        Some("btc-updown-5m-1769000000"),
        Some("btc-updown-5m"),
        Some("yes-token,no-token"),
        Some("UP,DOWN"),
        None,
        Some("/tmp/regime-fallback.ndjson"),
        Some("1500"),
    )
    .expect("collector config");

    let ticks = market_ticks_from_message(
        r#"{
          "event_type": "price_change",
          "timestamp": "1769000000750",
          "price_changes": [
            {
              "asset_id": "yes-token",
              "price": "0.54",
              "size": "200",
              "side": "BUY"
            }
          ]
        }"#,
        &config.market_meta(),
        1_769_000_000_900,
    )
    .expect("price change ticks");

    assert_eq!(ticks.len(), 1);
    assert_eq!(ticks[0].timestamp_ms, 1_769_000_000_750);
    assert_eq!(ticks[0].meta.slug, "btc-updown-5m-1769000000");
    assert_eq!(ticks[0].meta.series, "btc-updown-5m");
    assert_eq!(ticks[0].price, 0.54);
    assert_eq!(ticks[0].size, 200.0);
    assert_eq!(ticks[0].side, "BUY");
    assert_eq!(ticks[0].outcome, "UP");
    assert_eq!(ticks[0].receive_lag_ms, 150);
}

#[test]
fn market_ticks_ignore_plain_websocket_heartbeats() {
    let config = LiveCollectorConfig::from_env_values(
        Some("true"),
        Some("btc-updown-5m-1769000000"),
        Some("btc-updown-5m"),
        Some("yes-token,no-token"),
        Some("UP,DOWN"),
        None,
        Some("/tmp/regime-fallback.ndjson"),
        Some("1500"),
    )
    .expect("collector config");

    let ticks = market_ticks_from_message("PONG", &config.market_meta(), 1_769_000_000_900)
        .expect("heartbeat ignored");

    assert!(ticks.is_empty());
}

#[test]
fn chainlink_update_message_converts_to_reference_market_tick() {
    let config = LiveCollectorConfig::from_env_values(
        Some("true"),
        Some("btc-updown-5m-1769000000"),
        Some("btc-updown-5m"),
        Some("yes-token,no-token"),
        Some("UP,DOWN"),
        None,
        Some("/tmp/regime-fallback.ndjson"),
        Some("1500"),
    )
    .expect("collector config");

    let tick = reference_tick_from_chainlink_message(
        r#"{
          "topic": "crypto_prices_chainlink",
          "type": "update",
          "timestamp": 1500,
          "payload": {
            "symbol": "btc/usd",
            "timestamp": 1250,
            "value": 65000.25
          }
        }"#,
        &config,
        1_500,
    )
    .expect("reference tick")
    .expect("ticker record");

    assert_eq!(tick.timestamp_ms, 1_250);
    assert_eq!(tick.meta.slug, "btc-updown-5m-1769000000");
    assert_eq!(tick.meta.source, "chainlink");
    assert_eq!(tick.price, 65000.25);
    assert_eq!(tick.size, 0.0);
    assert_eq!(tick.side, "ORACLE");
    assert_eq!(tick.outcome, "btc/usd");
    assert_eq!(tick.receive_lag_ms, 250);
}

#[test]
fn chainlink_reference_ticks_ignore_plain_websocket_heartbeats() {
    let config = LiveCollectorConfig::from_env_values(
        Some("true"),
        Some("btc-updown-5m-1769000000"),
        Some("btc-updown-5m"),
        Some("yes-token,no-token"),
        Some("UP,DOWN"),
        None,
        Some("/tmp/regime-fallback.ndjson"),
        Some("1500"),
    )
    .expect("collector config");

    let tick =
        reference_tick_from_chainlink_message("PONG", &config, 1_500).expect("heartbeat ignored");

    assert!(tick.is_none());
}

#[test]
fn midpoint_response_converts_to_bba_ticks_for_current_market_assets() {
    let config = LiveCollectorConfig::from_env_values(
        Some("true"),
        Some("btc-updown-5m-1769000000"),
        Some("btc-updown-5m"),
        Some("yes-token,no-token"),
        Some("UP,DOWN"),
        None,
        Some("/tmp/regime-fallback.ndjson"),
        Some("1500"),
    )
    .expect("collector config");

    let ticks = midpoint_ticks_from_midpoints_response(
        &serde_json::json!({
            "yes-token": "0.515",
            "no-token": "0.485",
            "unrelated-token": "0.900"
        }),
        &config.market_meta(),
        1_769_000_001_250,
    )
    .expect("midpoint ticks");

    assert_eq!(ticks.len(), 2);
    assert_eq!(ticks[0].timestamp_ms, 1_769_000_001_250);
    assert_eq!(ticks[0].meta.source, "clob");
    assert_eq!(ticks[0].price, 0.515);
    assert_eq!(ticks[0].side, "BBA");
    assert_eq!(ticks[0].outcome, "UP");
    assert_eq!(ticks[0].receive_lag_ms, 0);
    assert_eq!(ticks[1].price, 0.485);
    assert_eq!(ticks[1].outcome, "DOWN");
}

#[test]
fn single_midpoint_response_converts_to_bba_tick_for_token_id() {
    let config = LiveCollectorConfig::from_env_values(
        Some("true"),
        Some("btc-updown-5m-1769000000"),
        Some("btc-updown-5m"),
        Some("yes-token,no-token"),
        Some("UP,DOWN"),
        None,
        Some("/tmp/regime-fallback.ndjson"),
        Some("1500"),
    )
    .expect("collector config");

    let tick = midpoint_tick_from_midpoint_response(
        &serde_json::json!({ "mid": "0.975" }),
        &config.market_meta(),
        "yes-token",
        1_769_000_001_250,
    )
    .expect("midpoint tick");

    assert_eq!(tick.timestamp_ms, 1_769_000_001_250);
    assert_eq!(tick.meta.slug, "btc-updown-5m-1769000000");
    assert_eq!(tick.meta.source, "clob");
    assert_eq!(tick.price, 0.975);
    assert_eq!(tick.side, "BBA");
    assert_eq!(tick.outcome, "UP");
}

#[test]
fn stale_data_penalty_flips_after_configured_threshold() {
    assert_eq!(stale_data_penalty(Some(1_000), 1_999, 1_000), 0.0);
    assert_eq!(stale_data_penalty(Some(1_000), 2_001, 1_000), 1.0);
    assert_eq!(stale_data_penalty(None, 2_001, 1_000), 1.0);
}

#[test]
fn stale_regime_state_marks_data_as_low_confidence() {
    let config = LiveCollectorConfig::from_env_values(
        Some("true"),
        Some("btc-updown-5m-1769000000"),
        Some("btc-updown-5m"),
        Some("yes-token,no-token"),
        Some("UP,DOWN"),
        None,
        Some("/tmp/regime-fallback.ndjson"),
        Some("1500"),
    )
    .expect("collector config");

    let state = stale_regime_state(&config, Some(1_000), 2_600).expect("stale state");

    assert_eq!(state.id, "btc-updown-5m-1769000000");
    assert_eq!(state.regime, "STALE_DATA");
    assert_eq!(state.confidence, 0.0);
    assert_eq!(state.indicators["stale_data_penalty"], 1.0);
}

#[test]
fn ndjson_fallback_appends_kind_and_serialized_record() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("fallback.ndjson");
    let record = serde_json::json!({
        "timestamp_ms": 1769000000750_i64,
        "price": 0.54
    });

    append_ndjson_fallback(&path, "market_tick", &record).expect("append fallback");

    let content = std::fs::read_to_string(path).expect("fallback file");
    let line: Value = serde_json::from_str(content.trim()).expect("fallback json");
    assert_eq!(line["kind"], "market_tick");
    assert_eq!(line["record"]["price"], 0.54);
}

#[tokio::test]
async fn market_tick_persistence_without_mongodb_writes_ndjson_fallback() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("ticks.ndjson");
    let config = LiveCollectorConfig::from_env_values(
        Some("true"),
        Some("btc-updown-5m-1769000000"),
        Some("btc-updown-5m"),
        Some("yes-token,no-token"),
        Some("UP,DOWN"),
        None,
        path.to_str(),
        Some("1500"),
    )
    .expect("collector config");
    let tick = market_ticks_from_message(
        r#"{
          "event_type": "last_trade_price",
          "asset_id": "yes-token",
          "price": "0.55",
          "size": "10",
          "side": "BUY",
          "timestamp": "1769000000750"
        }"#,
        &config.market_meta(),
        1_769_000_000_800,
    )
    .expect("trade tick")
    .remove(0);

    persist_market_tick_or_fallback(None, &tick, &config.ndjson_path)
        .await
        .expect("persist fallback");

    let content = std::fs::read_to_string(path).expect("fallback file");
    let line: Value = serde_json::from_str(content.trim()).expect("fallback json");
    assert_eq!(line["kind"], "market_tick");
    assert_eq!(line["record"]["outcome"], "UP");
}

#[test]
fn live_smoke_summary_counts_market_and_reference_ticks() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("live-smoke.ndjson");
    std::fs::write(
        &path,
        r#"{"kind":"market_tick","record":{"timestamp_ms":1769000000100,"meta":{"slug":"btc-updown-5m-1769000000","series":"btc-updown-5m","source":"clob"},"price":0.54,"size":10.0,"side":"BUY","outcome":"UP","receive_lag_ms":25}}
{"kind":"market_tick","record":{"timestamp_ms":1769000000200,"meta":{"slug":"btc-updown-5m-1769000000","series":"btc-updown-5m","source":"chainlink"},"price":65000.25,"size":0.0,"side":"ORACLE","outcome":"btc/usd","receive_lag_ms":15}}
{"kind":"regime_state","record":{"id":"btc-updown-5m-1769000000","regime":"STALE_DATA"}}
"#,
    )
    .expect("write smoke fixture");

    let report =
        summarize_live_smoke_ndjson("btc-updown-5m-1769000000", 30, &path).expect("summary");

    assert!(report.passed);
    assert_eq!(report.market_ticks, 1);
    assert_eq!(report.reference_ticks, 1);
    assert_eq!(report.stale_states, 1);
    assert_eq!(report.ndjson_bytes, std::fs::metadata(&path).unwrap().len());
    assert_eq!(report.first_tick_timestamp_ms, Some(1_769_000_000_100));
    assert_eq!(report.last_tick_timestamp_ms, Some(1_769_000_000_200));
    assert_eq!(report.outcomes, vec!["UP", "btc/usd"]);
}

#[test]
fn live_smoke_gate_requires_market_reference_and_required_outcomes() {
    let outcomes = vec!["UP".to_string(), "DOWN".to_string(), "btc/usd".to_string()];

    assert!(live_smoke_passed(
        1,
        1,
        &outcomes,
        &["UP", "DOWN", "btc/usd"]
    ));
    assert!(!live_smoke_passed(
        0,
        1,
        &outcomes,
        &["UP", "DOWN", "btc/usd"]
    ));
    assert!(!live_smoke_passed(
        1,
        0,
        &outcomes,
        &["UP", "DOWN", "btc/usd"]
    ));
    assert!(!live_smoke_passed(
        1,
        1,
        &["UP".to_string(), "btc/usd".to_string()],
        &["UP", "DOWN", "btc/usd"]
    ));
}
