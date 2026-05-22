use anyhow::{Context, bail};
use regime_core::{
    AlertRecord, PricePoint, ShiftLabel, ShiftLabelConfig, ValidationReport, generate_shift_labels,
    validate_alerts,
};
use serde::{Deserialize, Serialize};
use std::{env, fs, path::PathBuf};

#[derive(Debug, Deserialize)]
struct ReplayInput {
    price_points: Vec<PricePoint>,
    alerts: Vec<AlertRecord>,
    label_config: ShiftLabelConfig,
    synchronous_tolerance_ms: i64,
}

#[derive(Debug, Serialize)]
struct ReplayOutput {
    labels: Vec<ShiftLabel>,
    report: ValidationReport,
}

fn main() -> anyhow::Result<()> {
    let input_path = parse_input_path(env::args().skip(1))?;
    let input = read_replay_input(&input_path)?;
    let labels = generate_shift_labels(&input.price_points, &input.label_config);
    let report = validate_alerts(&input.alerts, &labels, input.synchronous_tolerance_ms);
    let output = ReplayOutput { labels, report };

    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}

fn parse_input_path(args: impl IntoIterator<Item = String>) -> anyhow::Result<PathBuf> {
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg == "--input" {
            let Some(path) = args.next() else {
                bail!("--input requires a file path");
            };
            return Ok(PathBuf::from(path));
        }
    }

    bail!("usage: regime-replay --input <replay.json>")
}

fn read_replay_input(path: &PathBuf) -> anyhow::Result<ReplayInput> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("read replay input {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("parse replay input {}", path.display()))
}
