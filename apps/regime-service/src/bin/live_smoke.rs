use anyhow::{Context, bail};
use regime_service::live_collector::{
    LiveCollectorConfig, LiveSmokeReport, run_live_collector, run_reference_price_collector,
    summarize_live_smoke_ndjson,
};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let config = LiveCollectorConfig::from_env().map_err(anyhow::Error::msg)?;
    if !config.enabled {
        bail!("LIVE_COLLECTOR_ENABLED must be true for live_smoke");
    }

    let duration_seconds = std::env::var("LIVE_SMOKE_DURATION_SECONDS")
        .unwrap_or_else(|_| "45".to_string())
        .parse::<u64>()
        .context("LIVE_SMOKE_DURATION_SECONDS must be an integer")?;
    if duration_seconds == 0 {
        bail!("LIVE_SMOKE_DURATION_SECONDS must be greater than 0");
    }

    let market_path = path_with_extension_suffix(&config.ndjson_path, "market");
    let reference_path = path_with_extension_suffix(&config.ndjson_path, "reference");
    remove_if_exists(&market_path).context("clear previous market live smoke NDJSON")?;
    remove_if_exists(&reference_path).context("clear previous reference live smoke NDJSON")?;

    let mut market_config = config.clone();
    market_config.ndjson_path = market_path.clone();
    let mut reference_config = config.clone();
    reference_config.ndjson_path = reference_path.clone();
    let market_task = tokio::spawn(async move { run_live_collector(market_config, None).await });
    let reference_task =
        tokio::spawn(async move { run_reference_price_collector(reference_config, None).await });

    tokio::time::sleep(Duration::from_secs(duration_seconds)).await;
    market_task.abort();
    reference_task.abort();
    await_aborted_task(market_task).await?;
    await_aborted_task(reference_task).await?;

    let market_report = summarize_live_smoke_ndjson(&config.slug, duration_seconds, &market_path)?;
    let reference_report =
        summarize_live_smoke_ndjson(&config.slug, duration_seconds, &reference_path)?;
    let report = combine_reports(
        &config.slug,
        duration_seconds,
        &market_path,
        &reference_path,
        &market_report,
        &reference_report,
    );
    println!("{}", serde_json::to_string_pretty(&report)?);

    if !report.passed {
        bail!("live smoke did not observe both CLOB and Coinbase ticks");
    }

    Ok(())
}

fn path_with_extension_suffix(path: &Path, suffix: &str) -> PathBuf {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("ndjson");
    path.with_extension(format!("{suffix}.{extension}"))
}

fn remove_if_exists(path: &Path) -> anyhow::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("remove {}", path.display())),
    }
}

fn combine_reports(
    slug: &str,
    duration_seconds: u64,
    market_path: &Path,
    reference_path: &Path,
    market_report: &LiveSmokeReport,
    reference_report: &LiveSmokeReport,
) -> LiveSmokeReport {
    let mut outcomes = BTreeSet::new();
    outcomes.extend(market_report.outcomes.iter().cloned());
    outcomes.extend(reference_report.outcomes.iter().cloned());
    let market_ticks = market_report.market_ticks + reference_report.market_ticks;
    let reference_ticks = market_report.reference_ticks + reference_report.reference_ticks;
    let stale_states = market_report.stale_states + reference_report.stale_states;

    LiveSmokeReport {
        slug: slug.to_string(),
        duration_seconds,
        ndjson_path: format!("{};{}", market_path.display(), reference_path.display()),
        market_ticks,
        reference_ticks,
        stale_states,
        outcomes: outcomes.into_iter().collect(),
        first_tick_timestamp_ms: min_optional(
            market_report.first_tick_timestamp_ms,
            reference_report.first_tick_timestamp_ms,
        ),
        last_tick_timestamp_ms: max_optional(
            market_report.last_tick_timestamp_ms,
            reference_report.last_tick_timestamp_ms,
        ),
        passed: market_ticks > 0 && reference_ticks > 0,
    }
}

fn min_optional(left: Option<i64>, right: Option<i64>) -> Option<i64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn max_optional(left: Option<i64>, right: Option<i64>) -> Option<i64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

async fn await_aborted_task(
    task: tokio::task::JoinHandle<anyhow::Result<()>>,
) -> anyhow::Result<()> {
    match task.await {
        Ok(result) => result,
        Err(error) if error.is_cancelled() => Ok(()),
        Err(error) => Err(error).context("join live smoke task"),
    }
}
