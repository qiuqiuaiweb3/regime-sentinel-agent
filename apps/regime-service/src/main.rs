use anyhow::Context;
use mongodb::Client;
use regime_service::{
    gemini_summary::{GeminiSummaryConfig, run_gemini_summary_scheduler},
    gemini_throttle::GeminiCallBudget,
    live_collector::{LiveCollectorConfig, run_live_collector, run_reference_price_collector},
    mongo_store::MongoStore,
};
use std::{env, net::SocketAddr, path::PathBuf};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr: SocketAddr = format!("{host}:{port}").parse()?;
    let collector_config = LiveCollectorConfig::from_env().map_err(anyhow::Error::msg)?;
    let gemini_config = GeminiSummaryConfig::from_env().map_err(anyhow::Error::msg)?;
    let gemini_call_budget = GeminiCallBudget::new();

    if collector_config.enabled {
        let collector_store = mongo_store_from_env("live collector").await?;
        let market_config = collector_config
            .clone()
            .with_ndjson_path(collector_config.ndjson_path_for_role("market"));
        let market_store = collector_store.clone();
        tokio::spawn(async move {
            if let Err(error) = run_live_collector(market_config, market_store).await {
                tracing::error!(?error, "live collector stopped");
            }
        });
        let reference_config = collector_config
            .clone()
            .with_ndjson_path(collector_config.ndjson_path_for_role("reference"));
        tokio::spawn(async move {
            if let Err(error) =
                run_reference_price_collector(reference_config, collector_store).await
            {
                tracing::error!(?error, "reference price collector stopped");
            }
        });
    }
    if gemini_config.throttle.enabled {
        let gemini_store = mongo_store_from_env("Gemini summary scheduler").await?;
        let fallback_path = PathBuf::from(
            env::var("GEMINI_SUMMARY_NDJSON_PATH")
                .unwrap_or_else(|_| "data/agent-summaries.ndjson".to_string()),
        );
        let scheduler_gemini_call_budget = gemini_call_budget.clone();
        tokio::spawn(async move {
            if let Err(error) = run_gemini_summary_scheduler(
                gemini_config,
                gemini_store,
                fallback_path,
                scheduler_gemini_call_budget,
            )
            .await
            {
                tracing::error!(?error, "Gemini summary scheduler stopped");
            }
        });
    }

    let listener = tokio::net::TcpListener::bind(addr).await?;

    tracing::info!(%addr, "starting regime-service");
    axum::serve(
        listener,
        regime_service::build_router_with_gemini_call_budget(gemini_call_budget),
    )
    .await?;
    Ok(())
}

async fn mongo_store_from_env(component: &str) -> anyhow::Result<Option<MongoStore>> {
    match (env::var("MONGODB_URI"), env::var("MONGODB_DB")) {
        (Ok(uri), Ok(database_name)) => {
            let client = Client::with_uri_str(&uri)
                .await
                .with_context(|| format!("connect MongoDB for {component}"))?;
            Ok(Some(MongoStore::new(client.database(&database_name))))
        }
        _ => {
            tracing::warn!(%component, "MongoDB env missing; using NDJSON fallback");
            Ok(None)
        }
    }
}
