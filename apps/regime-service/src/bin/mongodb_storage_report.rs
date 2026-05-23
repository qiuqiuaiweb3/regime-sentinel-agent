use anyhow::Context;
use mongodb::{
    Client,
    bson::{Bson, Document, doc},
};
use serde_json::json;

const COLLECTIONS: [&str; 6] = [
    "market_ticks",
    "feature_windows",
    "regime_states",
    "alerts",
    "agent_summaries",
    "backtest_runs",
];

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    let uri = std::env::var("MONGODB_URI").context("MONGODB_URI is required")?;
    let database_name = std::env::var("MONGODB_DB").context("MONGODB_DB is required")?;
    let client = Client::with_uri_str(&uri)
        .await
        .context("connect MongoDB")?;
    let db = client.database(&database_name);

    let db_stats = db.run_command(doc! { "dbStats": 1, "scale": 1 }).await?;
    let mut collection_stats = Vec::new();
    for collection in COLLECTIONS {
        let stats = db
            .run_command(doc! { "collStats": collection, "scale": 1 })
            .await
            .with_context(|| format!("read collStats for {collection}"))?;
        collection_stats.push(collection_summary(collection, &stats));
    }

    let report = json!({
        "database": database_name,
        "db": {
            "collections": i64_field(&db_stats, "collections"),
            "objects": i64_field(&db_stats, "objects"),
            "dataSize": i64_field(&db_stats, "dataSize"),
            "storageSize": i64_field(&db_stats, "storageSize"),
            "indexSize": i64_field(&db_stats, "indexSize"),
            "totalSize": i64_field(&db_stats, "totalSize"),
        },
        "collections": collection_stats,
    });

    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn collection_summary(collection: &str, stats: &Document) -> serde_json::Value {
    json!({
        "name": collection,
        "count": i64_field(stats, "count"),
        "size": i64_field(stats, "size"),
        "avgObjSize": i64_field(stats, "avgObjSize"),
        "storageSize": i64_field(stats, "storageSize"),
        "totalIndexSize": i64_field(stats, "totalIndexSize"),
        "totalSize": i64_field(stats, "totalSize"),
        "nindexes": i64_field(stats, "nindexes"),
    })
}

fn i64_field(document: &Document, key: &str) -> Option<i64> {
    match document.get(key) {
        Some(Bson::Int32(value)) => Some(i64::from(*value)),
        Some(Bson::Int64(value)) => Some(*value),
        Some(Bson::Double(value)) => Some(*value as i64),
        _ => None,
    }
}
