use anyhow::Context;
use mongodb::Client;
use regime_core::{MarketTickRecord, RegimeStateRecord};
use regime_service::mongo_store::{MarketRetentionReport, MongoStore};
use serde_json::Value;
use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::PathBuf,
};

#[derive(Debug, Default, serde::Serialize)]
struct IngestReport {
    files: usize,
    market_ticks: usize,
    regime_states: usize,
    skipped_lines: usize,
    retention: Option<MarketRetentionReport>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    let paths = std::env::args()
        .skip(1)
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    if paths.is_empty() {
        anyhow::bail!("at least one NDJSON path is required");
    }

    let uri = std::env::var("MONGODB_URI").context("MONGODB_URI is required")?;
    let database_name = std::env::var("MONGODB_DB").context("MONGODB_DB is required")?;
    let client = Client::with_uri_str(&uri)
        .await
        .context("connect MongoDB")?;
    let store = MongoStore::new(client.database(&database_name));

    let mut report = IngestReport {
        files: paths.len(),
        ..IngestReport::default()
    };
    for path in paths {
        ingest_path(&store, &path, &mut report).await?;
    }
    let retained_markets = retained_markets_from_env()?;
    report.retention = Some(store.prune_old_market_data(retained_markets).await?);

    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn retained_markets_from_env() -> anyhow::Result<usize> {
    std::env::var("ROLLING_MARKET_LIMIT")
        .unwrap_or_else(|_| "12".to_string())
        .parse::<usize>()
        .context("ROLLING_MARKET_LIMIT must be an unsigned integer")
}

async fn ingest_path(
    store: &MongoStore,
    path: &PathBuf,
    report: &mut IngestReport,
) -> anyhow::Result<()> {
    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    for line in BufReader::new(file).lines() {
        let line = line.with_context(|| format!("read {}", path.display()))?;
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(&line)
            .with_context(|| format!("parse NDJSON in {}", path.display()))?;
        match value.get("kind").and_then(Value::as_str) {
            Some("market_tick") => {
                let tick: MarketTickRecord = serde_json::from_value(value["record"].clone())
                    .context("decode market_tick record")?;
                store.insert_market_tick(&tick).await?;
                report.market_ticks += 1;
            }
            Some("regime_state") => {
                let state: RegimeStateRecord = serde_json::from_value(value["record"].clone())
                    .context("decode regime_state record")?;
                store.upsert_regime_state(&state).await?;
                report.regime_states += 1;
            }
            _ => {
                report.skipped_lines += 1;
            }
        }
    }
    Ok(())
}
