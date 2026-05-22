use regime_core::{
    PricePoint, ShiftDirection, ShiftLabelConfig, compute_lead_time_ms, generate_shift_labels,
};

#[test]
fn generate_shift_labels_requires_move_to_persist() {
    let points = vec![
        PricePoint {
            timestamp_ms: 0,
            p_mid: 0.50,
        },
        PricePoint {
            timestamp_ms: 1_000,
            p_mid: 0.62,
        },
        PricePoint {
            timestamp_ms: 4_000,
            p_mid: 0.61,
        },
        PricePoint {
            timestamp_ms: 8_000,
            p_mid: 0.52,
        },
    ];

    let labels = generate_shift_labels(
        &points,
        &ShiftLabelConfig {
            horizons_ms: vec![1_000],
            min_move: 0.10,
            persist_ms: 3_000,
        },
    );

    assert_eq!(labels.len(), 1);
    assert_eq!(labels[0].baseline_time_ms, 0);
    assert_eq!(labels[0].onset_time_ms, 1_000);
    assert_eq!(labels[0].horizon_ms, 1_000);
    assert_eq!(labels[0].direction, ShiftDirection::Up);
    assert!((labels[0].magnitude - 0.12).abs() < 1e-9);
}

#[test]
fn generate_shift_labels_rejects_transient_moves_that_do_not_persist() {
    let points = vec![
        PricePoint {
            timestamp_ms: 0,
            p_mid: 0.50,
        },
        PricePoint {
            timestamp_ms: 1_000,
            p_mid: 0.62,
        },
        PricePoint {
            timestamp_ms: 4_000,
            p_mid: 0.54,
        },
    ];

    let labels = generate_shift_labels(
        &points,
        &ShiftLabelConfig {
            horizons_ms: vec![1_000],
            min_move: 0.10,
            persist_ms: 3_000,
        },
    );

    assert!(labels.is_empty());
}

#[test]
fn compute_lead_time_is_positive_when_alert_precedes_shift_onset() {
    assert_eq!(compute_lead_time_ms(750, 1_000), 250);
    assert_eq!(compute_lead_time_ms(1_000, 1_000), 0);
    assert_eq!(compute_lead_time_ms(1_250, 1_000), -250);
}
