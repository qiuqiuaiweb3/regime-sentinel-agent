use anyhow::{Context, Result};
use mongodb::Client;
use regime_service::demo_seed::{count_demo_seed, validate_demo_seed_counts};
use serde::Deserialize;

const DEMO_SEED_RUN_FILE: &str = ".regime-demo-seed.json";

#[derive(Deserialize)]
struct DemoSeedRunFile {
    run_id: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let uri = std::env::var("MONGODB_URI").context("MONGODB_URI is required")?;
    let database_name = std::env::var("MONGODB_DB").context("MONGODB_DB is required")?;

    let client = Client::with_uri_str(&uri)
        .await
        .context("connect to MongoDB")?;
    let run_id = demo_seed_run_id().context("resolve demo seed run id")?;
    let counts = count_demo_seed(&client.database(&database_name), &run_id)
        .await
        .context("count demo seed documents")?;
    validate_demo_seed_counts(&counts).map_err(anyhow::Error::msg)?;

    println!("{}", serde_json::to_string_pretty(&counts)?);
    Ok(())
}

fn demo_seed_run_id() -> Result<String> {
    if let Ok(run_id) = std::env::var("DEMO_SEED_RUN_ID") {
        return Ok(run_id);
    }

    let raw = std::fs::read_to_string(DEMO_SEED_RUN_FILE)
        .with_context(|| format!("read {DEMO_SEED_RUN_FILE}"))?;
    let run_file: DemoSeedRunFile =
        serde_json::from_str(&raw).with_context(|| format!("parse {DEMO_SEED_RUN_FILE}"))?;
    Ok(run_file.run_id)
}
