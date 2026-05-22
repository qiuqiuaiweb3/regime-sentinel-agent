use anyhow::{Context, bail};
use regime_core::{
    AlertRecord, FeatureWindowRecord, PricePoint, ScoreThresholds, ScoreWeights, ShiftLabel,
    ShiftLabelConfig, ValidationReport, generate_alerts_from_feature_windows,
    generate_shift_labels, validate_alerts,
};
use serde::{Deserialize, Serialize};
use std::{env, fs, path::PathBuf};

#[derive(Debug, Deserialize)]
struct ReplayInput {
    price_points: Vec<PricePoint>,
    #[serde(default)]
    alerts: Vec<AlertRecord>,
    #[serde(default)]
    feature_windows: Vec<FeatureWindowRecord>,
    score_weights: Option<ScoreWeights>,
    score_thresholds: Option<ScoreThresholds>,
    alert_horizon_ms: Option<i64>,
    label_config: ShiftLabelConfig,
    synchronous_tolerance_ms: i64,
}

#[derive(Debug, Serialize)]
struct ReplayOutput {
    alerts: Vec<AlertRecord>,
    labels: Vec<ShiftLabel>,
    report: ValidationReport,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum OutputFormat {
    Json,
    Csv,
}

#[derive(Debug, Eq, PartialEq)]
struct CliArgs {
    input_path: PathBuf,
    format: OutputFormat,
}

fn main() -> anyhow::Result<()> {
    let args = parse_args(env::args().skip(1))?;
    let input = read_replay_input(&args.input_path)?;
    let alerts = replay_alerts(&input)?;
    let labels = generate_shift_labels(&input.price_points, &input.label_config);
    let report = validate_alerts(&alerts, &labels, input.synchronous_tolerance_ms);
    let output = ReplayOutput {
        alerts,
        labels,
        report,
    };

    match args.format {
        OutputFormat::Json => println!("{}", serde_json::to_string(&output)?),
        OutputFormat::Csv => print!("{}", validation_csv(&output.report)),
    }

    Ok(())
}

fn replay_alerts(input: &ReplayInput) -> anyhow::Result<Vec<AlertRecord>> {
    if !input.alerts.is_empty() {
        return Ok(input.alerts.clone());
    }

    if input.feature_windows.is_empty() {
        return Ok(Vec::new());
    }

    let Some(weights) = input.score_weights else {
        bail!("score_weights are required when feature_windows are provided without alerts");
    };
    let Some(thresholds) = input.score_thresholds else {
        bail!("score_thresholds are required when feature_windows are provided without alerts");
    };
    let Some(horizon_ms) = input.alert_horizon_ms else {
        bail!("alert_horizon_ms is required when feature_windows are provided without alerts");
    };

    Ok(generate_alerts_from_feature_windows(
        &input.feature_windows,
        &weights,
        &thresholds,
        horizon_ms,
    ))
}

fn parse_args(args: impl IntoIterator<Item = String>) -> anyhow::Result<CliArgs> {
    let mut args = args.into_iter();
    let mut input_path = None;
    let mut format = OutputFormat::Json;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--input" => {
                let Some(path) = args.next() else {
                    bail!("--input requires a file path");
                };
                input_path = Some(PathBuf::from(path));
            }
            "--format" => {
                let Some(raw_format) = args.next() else {
                    bail!("--format requires json or csv");
                };
                format = match raw_format.as_str() {
                    "json" => OutputFormat::Json,
                    "csv" => OutputFormat::Csv,
                    _ => bail!("--format must be json or csv"),
                };
            }
            _ => bail!("unknown argument {arg}"),
        }
    }

    let Some(input_path) = input_path else {
        bail!("usage: regime-replay --input <replay.json> [--format json|csv]");
    };

    Ok(CliArgs { input_path, format })
}

fn read_replay_input(path: &PathBuf) -> anyhow::Result<ReplayInput> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("read replay input {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("parse replay input {}", path.display()))
}

fn validation_csv(report: &ValidationReport) -> String {
    let mut csv =
        String::from("alert_time_ms,shift_onset_time_ms,lead_time_ms,horizon_ms,timing\n");

    for result in &report.results {
        let shift_onset = result
            .shift_onset_time_ms
            .map(|value| value.to_string())
            .unwrap_or_default();
        let lead_time = result
            .lead_time_ms
            .map(|value| value.to_string())
            .unwrap_or_default();

        csv.push_str(&format!(
            "{},{},{},{},{:?}\n",
            result.alert_time_ms, shift_onset, lead_time, result.horizon_ms, result.timing
        ));
    }

    csv
}
