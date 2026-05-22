use assert_cmd::Command;
use predicates::str::contains;
use std::path::PathBuf;

#[test]
fn latency_probe_reports_hot_path_p95_under_gate() {
    let input_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../demo/replay/latency-probe-window.json");

    Command::cargo_bin("latency-probe")
        .expect("latency-probe binary")
        .arg("--input")
        .arg(input_path)
        .arg("--samples")
        .arg("128")
        .assert()
        .success()
        .stdout(contains(r#""samples":128"#))
        .stdout(contains(r#""p95_under_500ms":true"#));
}
