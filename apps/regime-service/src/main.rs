use anyhow::Context;
use mongodb::Client;
use regime_service::{
    live_collector::{LiveCollectorConfig, run_live_collector, run_reference_price_collector},
    mongo_store::MongoStore,
};
use std::{env, net::SocketAddr};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr: SocketAddr = format!("{host}:{port}").parse()?;
    let collector_config = LiveCollectorConfig::from_env().map_err(anyhow::Error::msg)?;

    if collector_config.enabled {
        let collector_store = match (env::var("MONGODB_URI"), env::var("MONGODB_DB")) {
            (Ok(uri), Ok(database_name)) => {
                let client = Client::with_uri_str(&uri)
                    .await
                    .context("connect MongoDB for live collector")?;
                Some(MongoStore::new(client.database(&database_name)))
            }
            _ => {
                tracing::warn!("live collector enabled without MongoDB env; using NDJSON fallback");
                None
            }
        };
        let market_config = collector_config.clone();
        let market_store = collector_store.clone();
        tokio::spawn(async move {
            if let Err(error) = run_live_collector(market_config, market_store).await {
                tracing::error!(?error, "live collector stopped");
            }
        });
        tokio::spawn(async move {
            if let Err(error) =
                run_reference_price_collector(collector_config, collector_store).await
            {
                tracing::error!(?error, "reference price collector stopped");
            }
        });
    }

    let listener = tokio::net::TcpListener::bind(addr).await?;

    tracing::info!(%addr, "starting regime-service");
    axum::serve(listener, regime_service::build_router()).await?;
    Ok(())
}
