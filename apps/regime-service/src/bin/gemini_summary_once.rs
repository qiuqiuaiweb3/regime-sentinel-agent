use anyhow::{Context, Result};
use mongodb::Client;
use regime_service::{
    gemini_summary::{
        GeminiSummaryConfig, build_summary_prompt, demo_summary_state,
        persist_agent_summary_or_fallback, request_gemini_summary, summary_record,
    },
    mongo_store::MongoStore,
};
use serde::Serialize;
use serde_json::json;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize)]
struct GeminiSummaryOnceOutput {
    model: String,
    provider: String,
    persisted: String,
    summary: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let config = GeminiSummaryConfig::from_env().map_err(anyhow::Error::msg)?;
    if !config.throttle.enabled {
        anyhow::bail!("GEMINI_ENABLED=true is required for gemini_summary_once");
    }

    let now_ms = unix_timestamp_ms();
    let state = demo_summary_state(now_ms);
    let prompt = build_summary_prompt(&state, 1);
    let client = reqwest::Client::new();
    let summary = request_gemini_summary(&client, &config, &prompt)
        .await
        .context("request Gemini summary")?;
    let bucket_seconds = (config.throttle.summary_interval_minutes * 60) as i64;
    let record = summary_record(
        now_ms,
        bucket_seconds,
        &config.model,
        &config.thinking_level,
        &summary,
        vec!["demo-alert-early-risk".to_string()],
        vec!["demo-window-high-volatility".to_string()],
        json!({ "estimated": true, "provider": format!("{:?}", config.provider) }),
    );

    let (store, persisted) = match (std::env::var("MONGODB_URI"), std::env::var("MONGODB_DB")) {
        (Ok(uri), Ok(database_name)) => {
            let mongo = Client::with_uri_str(&uri)
                .await
                .context("connect to MongoDB")?;
            (
                Some(MongoStore::new(mongo.database(&database_name))),
                "mongodb".to_string(),
            )
        }
        _ => (None, "ndjson_fallback".to_string()),
    };
    let fallback_path = std::env::var("GEMINI_SUMMARY_FALLBACK_PATH")
        .unwrap_or_else(|_| "gemini_summary.ndjson".to_string());
    persist_agent_summary_or_fallback(store.as_ref(), &record, Path::new(&fallback_path)).await?;

    println!(
        "{}",
        serde_json::to_string_pretty(&GeminiSummaryOnceOutput {
            model: config.model,
            provider: format!("{:?}", config.provider),
            persisted,
            summary,
        })?
    );
    Ok(())
}

fn unix_timestamp_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time is after Unix epoch")
        .as_millis() as i64
}
