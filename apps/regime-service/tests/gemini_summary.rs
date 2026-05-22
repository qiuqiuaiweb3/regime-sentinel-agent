use regime_core::RegimeStateRecord;
use regime_service::gemini_summary::{
    GeminiAuth, GeminiProvider, GeminiSummaryConfig, build_gemini_request, build_summary_prompt,
    demo_summary_state, parse_gemini_text, summary_record,
};
use serde_json::json;

#[test]
fn gemini_summary_config_defaults_to_disabled_without_api_key() {
    let config = GeminiSummaryConfig::from_env_values(
        None, None, None, None, None, None, None, None, None, None,
    )
    .expect("gemini config");

    assert!(!config.throttle.enabled);
    assert_eq!(config.model, "gemini-3-pro-preview");
    assert_eq!(config.provider, GeminiProvider::VertexAi);
    assert_eq!(config.throttle.summary_interval_minutes, 30);
}

#[test]
fn gemini_request_uses_vertex_generate_content_shape() {
    let config = GeminiSummaryConfig::from_env_values(
        Some("true"),
        Some("15"),
        Some("2"),
        None,
        Some("gemini-3-pro-preview"),
        None,
        Some("vertex"),
        Some("poly-market-analysis"),
        Some("global"),
        Some("test-token"),
    )
    .expect("gemini config");
    let prompt = "Summarize EARLY_RISK in one sentence.";

    let request = build_gemini_request(&config, prompt).expect("request");

    assert_eq!(
        request.url,
        "https://aiplatform.googleapis.com/v1/projects/poly-market-analysis/locations/global/publishers/google/models/gemini-3-pro-preview:generateContent"
    );
    assert_eq!(
        request.auth,
        Some(GeminiAuth::BearerToken("test-token".to_string()))
    );
    assert_eq!(request.body["contents"][0]["parts"][0]["text"], prompt);
    assert_eq!(
        request.body["generationConfig"]["thinkingConfig"]["thinkingLevel"],
        "LOW"
    );
    assert_eq!(request.body["generationConfig"]["maxOutputTokens"], 1024);
}

#[test]
fn vertex_request_can_defer_auth_to_metadata_token() {
    let config = GeminiSummaryConfig::from_env_values(
        Some("true"),
        Some("15"),
        Some("2"),
        None,
        Some("gemini-3-flash-preview"),
        None,
        Some("vertex"),
        Some("poly-market-analysis"),
        Some("global"),
        None,
    )
    .expect("gemini config");

    let request = build_gemini_request(&config, "hello").expect("request");

    assert_eq!(request.auth, None);
    assert!(request.url.contains("aiplatform.googleapis.com"));
}

#[test]
fn gemini_request_still_supports_developer_api_key_fallback() {
    let config = GeminiSummaryConfig::from_env_values(
        Some("true"),
        Some("15"),
        Some("2"),
        Some("test-key"),
        Some("gemini-3-pro-preview"),
        None,
        Some("developer_api"),
        None,
        None,
        None,
    )
    .expect("gemini config");
    let request = build_gemini_request(&config, "hello").expect("request");

    assert_eq!(
        request.url,
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-3-pro-preview:generateContent"
    );
    assert_eq!(
        request.auth,
        Some(GeminiAuth::ApiKey("test-key".to_string()))
    );
}

#[test]
fn parse_gemini_text_reads_first_text_part() {
    let text = parse_gemini_text(&json!({
        "candidates": [
            {
                "content": {
                    "parts": [
                        { "thought": true },
                        { "text": "Risk is rising before the full price move." }
                    ]
                }
            }
        ]
    }))
    .expect("gemini text");

    assert_eq!(text, "Risk is rising before the full price move.");
}

#[test]
fn parse_gemini_text_rejects_response_without_text() {
    let error = parse_gemini_text(&json!({
        "candidates": [
            { "content": { "parts": [ { "thought": true } ] } }
        ]
    }))
    .expect_err("missing text should fail");

    assert!(error.contains("text"));
    assert!(error.contains("candidates"));
}

#[test]
fn summary_prompt_and_record_keep_cost_control_metadata() {
    let state = RegimeStateRecord {
        id: "btc-updown-5m-1769000000".to_string(),
        regime: "EARLY_RISK".to_string(),
        confidence: 0.72,
        updated_at_ms: 1_769_000_000_750,
        previous_regime: "WATCH".to_string(),
        indicators: json!({ "fair_gap": 0.03, "ofi_1s": 0.42 }),
        market_resolved: false,
    };

    let prompt = build_summary_prompt(&state, 3);
    assert!(prompt.contains("EARLY_RISK"));
    assert!(prompt.contains("3 recent alerts"));

    let record = summary_record(
        1_769_000_000_000,
        1_800,
        "gemini-3-pro-preview",
        "low",
        "Risk is rising.",
        vec!["alert-1".to_string()],
        vec!["window-1".to_string()],
        json!({ "estimated": true }),
    );

    assert_eq!(record.bucket_seconds, 1_800);
    assert_eq!(record.model, "gemini-3-pro-preview");
    assert_eq!(record.thinking_level, "low");
    assert_eq!(record.alert_ids, vec!["alert-1"]);
}

#[test]
fn demo_summary_state_matches_early_risk_context() {
    let state = demo_summary_state(1_769_000_000_750);

    assert_eq!(state.regime, "EARLY_RISK");
    assert_eq!(state.previous_regime, "WATCH");
    assert_eq!(state.indicators["lead_time_ms"], json!(250));
    assert_eq!(state.updated_at_ms, 1_769_000_000_750);
}
