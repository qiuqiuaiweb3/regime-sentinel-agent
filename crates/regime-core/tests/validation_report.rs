use regime_core::{
    AlertConfidence, AlertRecord, AlertState, DetectionTiming, ShiftDirection, ShiftLabel,
    validate_alerts, validate_alerts_for_market,
};

fn label(onset_time_ms: i64, horizon_ms: i64) -> ShiftLabel {
    ShiftLabel {
        baseline_time_ms: onset_time_ms - horizon_ms,
        onset_time_ms,
        horizon_ms,
        direction: ShiftDirection::Up,
        magnitude: 0.12,
    }
}

fn alert(timestamp_ms: i64, horizon_ms: i64) -> AlertRecord {
    AlertRecord {
        timestamp_ms,
        state: AlertState::EarlyRisk,
        confidence: AlertConfidence::Normal,
        horizon_ms,
        score: 1.25,
    }
}

#[test]
fn validate_alerts_classifies_early_sync_late_and_false_alerts() {
    let labels = vec![
        label(1_000, 1_000),
        label(5_000, 5_000),
        label(30_000, 30_000),
    ];
    let alerts = vec![
        alert(750, 1_000),
        alert(5_050, 5_000),
        alert(30_500, 30_000),
        alert(1_200, 2_000),
    ];

    let report = validate_alerts(&alerts, &labels, 100);

    assert_eq!(report.results[0].timing, DetectionTiming::Early);
    assert_eq!(report.results[0].lead_time_ms, Some(250));
    assert_eq!(report.results[1].timing, DetectionTiming::Synchronous);
    assert_eq!(report.results[1].lead_time_ms, Some(-50));
    assert_eq!(report.results[2].timing, DetectionTiming::Late);
    assert_eq!(report.results[2].lead_time_ms, Some(-500));
    assert_eq!(report.results[3].timing, DetectionTiming::FalseAlert);
    assert_eq!(report.results[3].lead_time_ms, None);

    assert_eq!(report.summary.total_alerts, 4);
    assert_eq!(report.summary.early, 1);
    assert_eq!(report.summary.synchronous, 1);
    assert_eq!(report.summary.late, 1);
    assert_eq!(report.summary.false_alerts, 1);
}

#[test]
fn validate_alerts_reports_required_effectiveness_metrics() {
    let labels = vec![
        label(1_000, 1_000),
        label(5_000, 5_000),
        label(30_000, 30_000),
    ];
    let alerts = vec![
        alert(750, 1_000),
        alert(5_050, 5_000),
        alert(30_500, 30_000),
        alert(1_200, 2_000),
    ];

    let report = validate_alerts_for_market("btc-updown-5m-demo", &alerts, &labels, 100);

    assert_eq!(report.metrics.median_lead_time_ms, Some(-50.0));
    assert_eq!(report.metrics.p75_lead_time_ms, Some(250.0));
    assert_eq!(report.metrics.precision, 0.75);
    assert_eq!(report.metrics.recall, 1.0);
    assert_eq!(
        report.metrics.false_alerts_per_market[0].market_slug,
        "btc-updown-5m-demo"
    );
    assert_eq!(report.metrics.false_alerts_per_market[0].false_alerts, 1);
    assert_eq!(report.metrics.horizon_pr_auc.len(), 3);
    assert!(
        report
            .metrics
            .horizon_pr_auc
            .iter()
            .all(|metric| metric.pr_auc == 1.0)
    );
}
