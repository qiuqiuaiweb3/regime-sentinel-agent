use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use std::fs;
use tower::ServiceExt;

#[tokio::test]
async fn health_endpoint_reports_service_status() {
    let response = regime_service::build_router()
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .expect("health request"),
        )
        .await
        .expect("health response");

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("health body");
    let payload: Value = serde_json::from_slice(&body).expect("health json");
    assert_eq!(payload["status"], "ok");
    assert_eq!(payload["service"], "regime-service");
}

#[tokio::test]
async fn replay_validation_endpoint_returns_lead_time_report() {
    let request_body = json!({
        "price_points": [
            {"timestamp_ms": 0, "p_mid": 0.50},
            {"timestamp_ms": 1000, "p_mid": 0.62},
            {"timestamp_ms": 4000, "p_mid": 0.61}
        ],
        "alerts": [
            {
                "timestamp_ms": 750,
                "state": "EarlyRisk",
                "confidence": "Normal",
                "horizon_ms": 1000,
                "score": 1.25
            }
        ],
        "label_config": {
            "horizons_ms": [1000],
            "min_move": 0.10,
            "persist_ms": 3000
        },
        "synchronous_tolerance_ms": 100
    });

    let response = regime_service::build_router()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/replay/validate")
                .header("content-type", "application/json")
                .body(Body::from(request_body.to_string()))
                .expect("validation request"),
        )
        .await
        .expect("validation response");

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("validation body");
    let payload: Value = serde_json::from_slice(&body).expect("validation json");

    assert_eq!(payload["report"]["summary"]["early"], 1);
    assert_eq!(payload["report"]["results"][0]["lead_time_ms"], 250);
    assert_eq!(payload["labels"][0]["onset_time_ms"], 1000);
}

#[tokio::test]
async fn replay_validation_endpoint_generates_alerts_from_feature_windows() {
    let request_body = json!({
        "price_points": [
            {"timestamp_ms": 0, "p_mid": 0.50},
            {"timestamp_ms": 1000, "p_mid": 0.62},
            {"timestamp_ms": 4000, "p_mid": 0.61}
        ],
        "feature_windows": [
            {
                "slug": "btc-updown-5m",
                "window_ts_ms": 0,
                "window_ms": 1000,
                "p_mid": 0.50,
                "p_fair": 0.49,
                "fair_gap": 0.01,
                "ofi_1s": 0.01,
                "depth_imbalance": 0.01,
                "spread": 0.02,
                "volume_acceleration": 0.01,
                "feature_vector": [0.01, 0.01, 0.01, 0.02, 0.01]
            },
            {
                "slug": "btc-updown-5m",
                "window_ts_ms": 750,
                "window_ms": 1000,
                "p_mid": 0.54,
                "p_fair": 0.49,
                "fair_gap": 0.05,
                "ofi_1s": 0.42,
                "depth_imbalance": 0.31,
                "spread": 0.03,
                "volume_acceleration": 2.1,
                "feature_vector": [0.05, 0.42, 0.31, 0.03, 2.1]
            }
        ],
        "score_weights": {
            "fair_gap_velocity": 4.0,
            "depth_imbalance": 1.0,
            "ofi_1s": 1.0,
            "volume_acceleration": 0.5,
            "stale_data_penalty": 1.0
        },
        "score_thresholds": {
            "watch": 0.5,
            "early_risk": 1.0,
            "shift_detected_move": 0.10
        },
        "alert_horizon_ms": 1000,
        "label_config": {
            "horizons_ms": [1000],
            "min_move": 0.10,
            "persist_ms": 3000
        },
        "synchronous_tolerance_ms": 100
    });

    let response = regime_service::build_router()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/replay/validate")
                .header("content-type", "application/json")
                .body(Body::from(request_body.to_string()))
                .expect("validation request"),
        )
        .await
        .expect("validation response");

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("validation body");
    let payload: Value = serde_json::from_slice(&body).expect("validation json");

    assert_eq!(payload["alerts"][0]["timestamp_ms"], 750);
    assert_eq!(payload["report"]["summary"]["early"], 1);
    assert_eq!(payload["report"]["results"][0]["lead_time_ms"], 250);
    assert_eq!(payload["ablation"][0]["variant"], "baseline");
}

#[tokio::test]
async fn static_frontend_routes_fallback_to_index_without_hiding_api_routes() {
    let temp_dir = tempfile::tempdir().expect("static temp dir");
    fs::write(
        temp_dir.path().join("index.html"),
        "<!doctype html><title>Regime Sentinel Agent</title>",
    )
    .expect("write index");

    let app = regime_service::build_router_with_static_dir(temp_dir.path());

    let frontend_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/replay/btc-updown-5m")
                .body(Body::empty())
                .expect("frontend request"),
        )
        .await
        .expect("frontend response");

    assert_eq!(frontend_response.status(), StatusCode::OK);
    let frontend_body = to_bytes(frontend_response.into_body(), usize::MAX)
        .await
        .expect("frontend body");
    assert!(String::from_utf8_lossy(&frontend_body).contains("Regime Sentinel Agent"));

    let health_response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .expect("health request"),
        )
        .await
        .expect("health response");

    assert_eq!(health_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn dashboard_snapshot_endpoint_returns_replay_ready_payload() {
    let response = regime_service::build_router()
        .oneshot(
            Request::builder()
                .uri("/api/dashboard/snapshot")
                .body(Body::empty())
                .expect("snapshot request"),
        )
        .await
        .expect("snapshot response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("snapshot body");
    let payload: Value = serde_json::from_slice(&body).expect("snapshot json");

    assert_eq!(payload["mode"], "live");
    assert_eq!(payload["regime"]["state"], "EARLY_RISK");
    assert_eq!(payload["regime"]["confidence"], "Normal");
    assert_eq!(payload["price_points"][0]["p_mid"], 0.50);
    assert_eq!(payload["alerts"][0]["lead_time_ms"], 250);
    assert_eq!(payload["gemini_summary"]["enabled"], true);
    assert_eq!(payload["gemini_summary"]["coverage"], "last 30 minutes");
    assert_eq!(payload["similar_windows"][0]["score"], 0.98);
    assert_eq!(payload["validation"]["degraded_confidence"], true);
    assert_eq!(payload["validation"]["horizons"][0]["horizon_ms"], 1000);
}

#[tokio::test]
async fn dashboard_snapshot_endpoint_accepts_replay_mode() {
    let response = regime_service::build_router()
        .oneshot(
            Request::builder()
                .uri("/api/dashboard/snapshot?mode=replay")
                .body(Body::empty())
                .expect("snapshot request"),
        )
        .await
        .expect("snapshot response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("snapshot body");
    let payload: Value = serde_json::from_slice(&body).expect("snapshot json");

    assert_eq!(payload["mode"], "replay");
}

#[tokio::test]
async fn dashboard_events_endpoint_exposes_sse_stream() {
    let response = regime_service::build_router()
        .oneshot(
            Request::builder()
                .uri("/api/dashboard/events")
                .body(Body::empty())
                .expect("events request"),
        )
        .await
        .expect("events response");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok()),
        Some("text/event-stream")
    );
}

#[tokio::test]
async fn agent_tool_endpoints_return_demo_safe_payloads_without_mongodb() {
    let app = regime_service::build_router_with_agent_tool_mongodb(false);

    let current_regime = get_json(
        app.clone(),
        "/api/agent/current-regime?slug=btc-updown-5m-1769000000",
    )
    .await;
    assert_eq!(current_regime["source"], "sample");
    assert_eq!(current_regime["regime"]["regime"], "EARLY_RISK");

    let recent_alerts = get_json(
        app.clone(),
        "/api/agent/recent-alerts?slug=btc-updown-5m-1769000000&limit=3",
    )
    .await;
    assert_eq!(recent_alerts["source"], "sample");
    assert_eq!(recent_alerts["alerts"][0]["state"], "EARLY_RISK");

    let backtest_metrics = get_json(app.clone(), "/api/agent/backtest-metrics?limit=1").await;
    assert_eq!(backtest_metrics["source"], "sample");
    assert_eq!(
        backtest_metrics["runs"][0]["metrics"]["median_lead_time_ms"],
        250
    );

    let market_summary = get_json(
        app.clone(),
        "/api/agent/market-summary?slug=btc-updown-5m-1769000000",
    )
    .await;
    assert_eq!(market_summary["source"], "sample");
    assert_eq!(market_summary["summary"]["model"], "gemini-disabled-demo");

    let similar_windows_body = json!({
        "slug": "btc-updown-5m-1769000000",
        "query_vector": [0.05, 0.42, 0.31, 0.03, 2.1],
        "limit": 3
    });
    let similar_windows = post_json(app, "/api/agent/similar-windows", similar_windows_body).await;
    assert_eq!(similar_windows["source"], "sample");
    assert_eq!(
        similar_windows["windows"][0]["slug"],
        "btc-updown-5m-1769000000"
    );
}

#[tokio::test]
async fn openapi_spec_exposes_agent_builder_read_tools() {
    let response = regime_service::build_router()
        .oneshot(
            Request::builder()
                .uri("/api/openapi.json")
                .body(Body::empty())
                .expect("openapi request"),
        )
        .await
        .expect("openapi response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("openapi body");
    let payload: Value = serde_json::from_slice(&body).expect("openapi json");

    assert_eq!(payload["openapi"], "3.0.3");
    assert_eq!(
        payload["servers"][0]["url"],
        "https://regime-sentinel-agent-998092298764.asia-northeast1.run.app"
    );
    assert_eq!(
        payload["paths"]["/api/dashboard/snapshot"]["get"]["operationId"],
        "getDashboardSnapshot"
    );
    assert_eq!(
        payload["paths"]["/api/dashboard/snapshot"]["get"]["parameters"][0]["name"],
        "mode"
    );
    assert_eq!(
        payload["paths"]["/api/dashboard/snapshot"]["get"]["responses"]["200"]["content"]["application/json"]
            ["schema"]["$ref"],
        "#/components/schemas/DashboardSnapshot"
    );
    assert_eq!(
        payload["paths"]["/api/replay/validate"]["post"]["operationId"],
        "validateReplay"
    );
    assert_eq!(
        payload["paths"]["/api/agent/current-regime"]["get"]["operationId"],
        "getCurrentRegime"
    );
    assert_eq!(
        payload["paths"]["/api/agent/recent-alerts"]["get"]["operationId"],
        "queryRecentAlerts"
    );
    assert_eq!(
        payload["paths"]["/api/agent/similar-windows"]["post"]["operationId"],
        "findSimilarWindows"
    );
    assert_eq!(
        payload["paths"]["/api/agent/backtest-metrics"]["get"]["operationId"],
        "getBacktestMetrics"
    );
    assert_eq!(
        payload["paths"]["/api/agent/market-summary"]["get"]["operationId"],
        "generateMarketSummary"
    );
    assert_eq!(
        payload["paths"]["/api/replay/validate"]["post"]["requestBody"]["content"]["application/json"]
            ["schema"]["$ref"],
        "#/components/schemas/ReplayValidationRequest"
    );
    assert_eq!(
        payload["paths"]["/api/agent/similar-windows"]["post"]["requestBody"]["content"]["application/json"]
            ["schema"]["$ref"],
        "#/components/schemas/FindSimilarWindowsRequest"
    );
    assert_eq!(
        payload["components"]["schemas"]["ReplayValidationRequest"]["required"][0],
        "price_points"
    );
    assert_eq!(
        payload["components"]["schemas"]["DashboardSnapshot"]["required"][0],
        "mode"
    );
    assert_eq!(
        payload["components"]["schemas"]["DashboardGeminiSummary"]["properties"]["coverage"]["type"],
        "string"
    );
}

async fn get_json(app: axum::Router, uri: &str) -> Value {
    let response = app
        .oneshot(
            Request::builder()
                .uri(uri)
                .body(Body::empty())
                .expect("get request"),
        )
        .await
        .expect("get response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("get body");
    serde_json::from_slice(&body).expect("get json")
}

async fn post_json(app: axum::Router, uri: &str, body: Value) -> Value {
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .expect("post request"),
        )
        .await
        .expect("post response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("post body");
    serde_json::from_slice(&body).expect("post json")
}
