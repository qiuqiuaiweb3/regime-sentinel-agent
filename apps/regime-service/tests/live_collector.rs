use regime_service::live_collector::{
    LiveCollectorConfig, append_ndjson_fallback, market_ticks_from_message,
    persist_market_tick_or_fallback, reference_tick_from_coinbase_message, stale_data_penalty,
    stale_regime_state, summarize_live_smoke_ndjson,
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
fn live_collector_config_builds_coinbase_reference_subscription() {
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
            "type": "subscribe",
            "product_ids": ["BTC-USD"],
            "channels": ["ticker", "heartbeat"]
        })
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
fn coinbase_ticker_message_converts_to_reference_market_tick() {
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

    let tick = reference_tick_from_coinbase_message(
        r#"{
          "type": "ticker",
          "product_id": "BTC-USD",
          "price": "65000.25",
          "time": "1970-01-01T00:00:01.250Z"
        }"#,
        &config,
        1_500,
    )
    .expect("reference tick")
    .expect("ticker record");

    assert_eq!(tick.timestamp_ms, 1_250);
    assert_eq!(tick.meta.slug, "btc-updown-5m-1769000000");
    assert_eq!(tick.meta.source, "coinbase");
    assert_eq!(tick.price, 65000.25);
    assert_eq!(tick.size, 0.0);
    assert_eq!(tick.side, "TICKER");
    assert_eq!(tick.outcome, "BTC-USD");
    assert_eq!(tick.receive_lag_ms, 250);
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
{"kind":"market_tick","record":{"timestamp_ms":1769000000200,"meta":{"slug":"btc-updown-5m-1769000000","series":"btc-updown-5m","source":"coinbase"},"price":65000.25,"size":0.0,"side":"TICKER","outcome":"BTC-USD","receive_lag_ms":15}}
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
    assert_eq!(report.first_tick_timestamp_ms, Some(1_769_000_000_100));
    assert_eq!(report.last_tick_timestamp_ms, Some(1_769_000_000_200));
    assert_eq!(report.outcomes, vec!["BTC-USD", "UP"]);
}
