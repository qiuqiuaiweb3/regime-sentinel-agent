use anyhow::{Context, Result};
use mongodb::Client;
use regime_service::demo_seed::{generate_demo_seed_run_id, write_demo_seed};
use serde::Serialize;

const DEMO_SEED_RUN_FILE: &str = ".regime-demo-seed.json";

#[derive(Serialize)]
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
    let run_id = std::env::var("DEMO_SEED_RUN_ID").unwrap_or_else(|_| generate_demo_seed_run_id());
    let summary = write_demo_seed(&client.database(&database_name), &run_id)
        .await
        .context("write demo seed")?;

    std::fs::write(
        DEMO_SEED_RUN_FILE,
        serde_json::to_string_pretty(&DemoSeedRunFile { run_id })?,
    )
    .context("write demo seed run metadata")?;

    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}
