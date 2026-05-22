use anyhow::{Context, bail};
use regime_core::{
    FeatureWindowRecord, ScoreThresholds, ScoreWeights, generate_alerts_from_feature_windows,
};
use serde::{Deserialize, Serialize};
use std::{env, fs, hint::black_box, path::PathBuf, time::Instant};

#[derive(Debug, Deserialize)]
struct ProbeInput {
    #[serde(default)]
    feature_windows: Vec<FeatureWindowRecord>,
    score_weights: Option<ScoreWeights>,
    score_thresholds: Option<ScoreThresholds>,
    alert_horizon_ms: Option<i64>,
}

#[derive(Debug, Eq, PartialEq)]
struct CliArgs {
    input_path: PathBuf,
    samples: usize,
    max_p95_ms: u128,
}

#[derive(Debug, Serialize)]
struct LatencyProbeReport {
    operation: &'static str,
    samples: usize,
    p50_processing_ns: u128,
    p95_processing_ns: u128,
    p99_processing_ns: u128,
    max_processing_ns: u128,
    p50_processing_us: u128,
    p95_processing_us: u128,
    p99_processing_us: u128,
    max_processing_us: u128,
    max_p95_ms: u128,
    p95_under_500ms: bool,
    passed: bool,
}

fn main() -> anyhow::Result<()> {
    let args = parse_args(env::args().skip(1))?;
    let input = read_probe_input(&args.input_path)?;
    let report = run_probe(&input, args.samples, args.max_p95_ms)?;

    println!("{}", serde_json::to_string(&report)?);

    if !report.passed {
        bail!(
            "p95 latency {}ns exceeds threshold {}ms",
            report.p95_processing_ns,
            report.max_p95_ms
        );
    }

    Ok(())
}

fn run_probe(
    input: &ProbeInput,
    samples: usize,
    max_p95_ms: u128,
) -> anyhow::Result<LatencyProbeReport> {
    if samples == 0 {
        bail!("--samples must be greater than 0");
    }
    if input.feature_windows.len() < 2 {
        bail!("at least two feature_windows are required for latency probing");
    }

    let Some(weights) = input.score_weights else {
        bail!("score_weights are required for latency probing");
    };
    let Some(thresholds) = input.score_thresholds else {
        bail!("score_thresholds are required for latency probing");
    };
    let Some(horizon_ms) = input.alert_horizon_ms else {
        bail!("alert_horizon_ms is required for latency probing");
    };

    let mut elapsed_ns = Vec::with_capacity(samples);
    for _ in 0..samples {
        let started = Instant::now();
        let alerts = generate_alerts_from_feature_windows(
            &input.feature_windows,
            &weights,
            &thresholds,
            horizon_ms,
        );
        black_box(alerts.len());
        elapsed_ns.push(started.elapsed().as_nanos());
    }

    let p50_processing_ns = percentile(&elapsed_ns, 0.50);
    let p95_processing_ns = percentile(&elapsed_ns, 0.95);
    let p99_processing_ns = percentile(&elapsed_ns, 0.99);
    let max_processing_ns = elapsed_ns.iter().copied().max().unwrap_or_default();
    let p50_processing_us = nanos_to_ceil_micros(p50_processing_ns);
    let p95_processing_us = nanos_to_ceil_micros(p95_processing_ns);
    let p99_processing_us = nanos_to_ceil_micros(p99_processing_ns);
    let max_processing_us = nanos_to_ceil_micros(max_processing_ns);
    let p95_under_500ms = p95_processing_ns <= 500_000_000;
    let passed = p95_processing_ns <= max_p95_ms * 1_000_000;

    Ok(LatencyProbeReport {
        operation: "generate_alerts_from_feature_windows",
        samples,
        p50_processing_ns,
        p95_processing_ns,
        p99_processing_ns,
        max_processing_ns,
        p50_processing_us,
        p95_processing_us,
        p99_processing_us,
        max_processing_us,
        max_p95_ms,
        p95_under_500ms,
        passed,
    })
}

fn parse_args(args: impl IntoIterator<Item = String>) -> anyhow::Result<CliArgs> {
    let mut args = args.into_iter();
    let mut input_path = None;
    let mut samples = 128_usize;
    let mut max_p95_ms = 500_u128;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--input" => {
                let Some(path) = args.next() else {
                    bail!("--input requires a file path");
                };
                input_path = Some(PathBuf::from(path));
            }
            "--samples" => {
                let Some(raw_samples) = args.next() else {
                    bail!("--samples requires a positive integer");
                };
                samples = raw_samples
                    .parse()
                    .with_context(|| format!("parse --samples value {raw_samples}"))?;
            }
            "--max-p95-ms" => {
                let Some(raw_threshold) = args.next() else {
                    bail!("--max-p95-ms requires a positive integer");
                };
                max_p95_ms = raw_threshold
                    .parse()
                    .with_context(|| format!("parse --max-p95-ms value {raw_threshold}"))?;
            }
            _ => bail!("unknown argument {arg}"),
        }
    }

    let Some(input_path) = input_path else {
        bail!("usage: latency-probe --input <replay.json> [--samples n] [--max-p95-ms threshold]");
    };

    Ok(CliArgs {
        input_path,
        samples,
        max_p95_ms,
    })
}

fn read_probe_input(path: &PathBuf) -> anyhow::Result<ProbeInput> {
    let raw =
        fs::read_to_string(path).with_context(|| format!("read probe input {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("parse probe input {}", path.display()))
}

fn percentile(values: &[u128], quantile: f64) -> u128 {
    if values.is_empty() {
        return 0;
    }

    let mut values = values.to_vec();
    values.sort_unstable();
    let index = ((values.len() as f64 * quantile).ceil() as usize).saturating_sub(1);
    values.get(index).copied().unwrap_or_default()
}

fn nanos_to_ceil_micros(value: u128) -> u128 {
    value.saturating_add(999) / 1_000
}
