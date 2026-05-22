use anyhow::{Context, Result};
use mongodb::Client;
use regime_service::mongo_bootstrap::bootstrap_mongodb;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let uri = std::env::var("MONGODB_URI").context("MONGODB_URI is required")?;
    let database_name = std::env::var("MONGODB_DB").context("MONGODB_DB is required")?;

    let client = Client::with_uri_str(&uri)
        .await
        .context("connect to MongoDB")?;
    let summary = bootstrap_mongodb(&client.database(&database_name))
        .await
        .context("bootstrap MongoDB")?;

    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}
