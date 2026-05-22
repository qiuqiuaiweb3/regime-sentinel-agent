use mongodb::bson::Bson;
use regime_core::{FeatureWindowMetrics, build_feature_window};

#[test]
fn feature_window_document_matches_mongodb_schema_fields() {
    let window = build_feature_window(
        "btc-updown-5m",
        FeatureWindowMetrics {
            window_ts_ms: 1_769_000_000_000,
            window_ms: 1_000,
            p_mid: 0.52,
            p_fair: 0.49,
            ofi_1s: 0.42,
            depth_imbalance: 0.31,
            spread: 0.03,
            volume_acceleration: 2.1,
        },
    );

    let document = regime_service::mongo_documents::feature_window_document(&window);

    assert_eq!(document.get_str("slug"), Ok("btc-updown-5m"));
    assert_eq!(
        document
            .get_datetime("window_ts")
            .map(|value| value.timestamp_millis()),
        Ok(1_769_000_000_000)
    );
    assert!(!document.contains_key("window_ts_ms"));
    assert_eq!(document.get_i32("window_ms"), Ok(1_000));
    assert!((document.get_f64("fair_gap").unwrap() - 0.03).abs() < 1e-12);

    let feature_vector = document
        .get_array("feature_vector")
        .expect("feature_vector");
    assert_eq!(feature_vector.len(), 5);
    assert_eq!(feature_vector[1], Bson::Double(0.42));
    assert_eq!(feature_vector[4], Bson::Double(2.1));
}
