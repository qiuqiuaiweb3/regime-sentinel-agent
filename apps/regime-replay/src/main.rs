use anyhow::{Context, bail};
use regime_core::{
    AblationMetric, AlertRecord, FairProbabilityFeatureWindowRecord, FeatureWindowRecord,
    PricePoint, ScoreThresholds, ScoreWeights, ShiftLabel, ShiftLabelConfig, ValidationReport,
    ablation_report_from_feature_windows, build_feature_window_from_fair_probability_record,
    generate_alerts_from_feature_windows, generate_shift_labels, validate_alerts_for_market,
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
    #[serde(default)]
    fair_probability_feature_windows: Vec<FairProbabilityFeatureWindowRecord>,
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
    ablation: Vec<AblationMetric>,
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
    let report = validate_alerts_for_market(
        replay_market_slug(&input),
        &alerts,
        &labels,
        input.synchronous_tolerance_ms,
    );
    let ablation = replay_ablation(&input, &labels)?;
    let output = ReplayOutput {
        alerts,
        labels,
        report,
        ablation,
    };

    match args.format {
        OutputFormat::Json => println!("{}", serde_json::to_string(&output)?),
        OutputFormat::Csv => print!("{}", validation_csv(&output.report)),
    }

    Ok(())
}

fn replay_ablation(
    input: &ReplayInput,
    labels: &[ShiftLabel],
) -> anyhow::Result<Vec<AblationMetric>> {
    let feature_windows = replay_feature_windows(input);
    if feature_windows.is_empty() {
        return Ok(Vec::new());
    }

    let Some(weights) = input.score_weights else {
        return Ok(Vec::new());
    };
    let Some(thresholds) = input.score_thresholds else {
        return Ok(Vec::new());
    };
    let Some(horizon_ms) = input.alert_horizon_ms else {
        return Ok(Vec::new());
    };

    Ok(ablation_report_from_feature_windows(
        &feature_windows,
        labels,
        &weights,
        &thresholds,
        horizon_ms,
        input.synchronous_tolerance_ms,
    ))
}

fn replay_market_slug(input: &ReplayInput) -> &str {
    input
        .fair_probability_feature_windows
        .first()
        .map(|window| window.slug.as_str())
        .or_else(|| {
            input
                .feature_windows
                .first()
                .map(|window| window.slug.as_str())
        })
        .unwrap_or("replay")
}

fn replay_feature_windows(input: &ReplayInput) -> Vec<FeatureWindowRecord> {
    if !input.fair_probability_feature_windows.is_empty() {
        return input
            .fair_probability_feature_windows
            .iter()
            .map(build_feature_window_from_fair_probability_record)
            .collect();
    }

    input.feature_windows.clone()
}

fn replay_alerts(input: &ReplayInput) -> anyhow::Result<Vec<AlertRecord>> {
    if !input.alerts.is_empty() {
        return Ok(input.alerts.clone());
    }

    let feature_windows = replay_feature_windows(input);
    if feature_windows.is_empty() {
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
        &feature_windows,
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
