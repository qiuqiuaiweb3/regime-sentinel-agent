use axum::{
    Json, Router,
    routing::{get, post},
};
use regime_core::{
    AlertRecord, PricePoint, ShiftLabel, ShiftLabelConfig, ValidationReport, generate_shift_labels,
    validate_alerts,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
}

#[derive(Debug, Deserialize)]
pub struct ReplayValidationRequest {
    price_points: Vec<PricePoint>,
    alerts: Vec<AlertRecord>,
    label_config: ShiftLabelConfig,
    synchronous_tolerance_ms: i64,
}

#[derive(Debug, Serialize)]
pub struct ReplayValidationResponse {
    labels: Vec<ShiftLabel>,
    report: ValidationReport,
}

pub fn build_router() -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/replay/validate", post(validate_replay))
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "regime-service",
    })
}

async fn validate_replay(
    Json(request): Json<ReplayValidationRequest>,
) -> Json<ReplayValidationResponse> {
    let labels = generate_shift_labels(&request.price_points, &request.label_config);
    let report = validate_alerts(&request.alerts, &labels, request.synchronous_tolerance_ms);

    Json(ReplayValidationResponse { labels, report })
}
