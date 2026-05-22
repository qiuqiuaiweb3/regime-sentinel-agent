use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
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
