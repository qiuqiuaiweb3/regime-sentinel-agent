use mongodb::SearchIndexType;
use mongodb::bson::Bson;
use std::time::Duration;

#[test]
fn collection_create_models_include_time_series_ttl_for_market_ticks() {
    let models = regime_service::mongo_indexes::collection_create_models();
    let market_ticks = models
        .iter()
        .find(|model| model.collection_name == "market_ticks")
        .expect("market_ticks collection");

    let options = market_ticks.options.as_ref().expect("market_ticks options");
    let timeseries = options.timeseries.as_ref().expect("timeseries options");
    assert_eq!(timeseries.time_field, "timestamp");
    assert_eq!(timeseries.meta_field.as_deref(), Some("meta"));
    assert_eq!(
        options.expire_after_seconds,
        Some(Duration::from_secs(3_600))
    );

    let alerts = models
        .iter()
        .find(|model| model.collection_name == "alerts")
        .expect("alerts collection");
    assert!(alerts.options.is_none());
}

#[test]
fn regular_index_models_include_keys_options_and_collection_names() {
    let models = regime_service::mongo_indexes::regular_index_models();
    let market_ticks = models
        .iter()
        .find(|model| model.collection_name == "market_ticks")
        .expect("market_ticks index");

    assert_eq!(
        market_ticks.index.keys.get("meta.slug"),
        Some(&Bson::Int32(1))
    );
    assert_eq!(
        market_ticks.index.keys.get("timestamp"),
        Some(&Bson::Int32(1))
    );

    let options = market_ticks.index.options.as_ref().expect("index options");
    assert_eq!(options.name.as_deref(), Some("market_ticks_slug_timestamp"));
    assert_eq!(options.expire_after, None);
    assert_eq!(options.unique, None);

    let feature_windows = models
        .iter()
        .find(|model| model.collection_name == "feature_windows")
        .expect("feature_windows index");
    assert_eq!(
        feature_windows
            .index
            .options
            .as_ref()
            .and_then(|o| o.unique),
        Some(true)
    );
}

#[test]
fn regular_index_models_do_not_include_vector_search_index() {
    let models = regime_service::mongo_indexes::regular_index_models();

    assert!(!models.iter().any(|model| {
        model
            .index
            .options
            .as_ref()
            .and_then(|options| options.name.as_deref())
            == Some("feature_windows_vector_search")
    }));
}

#[test]
fn vector_search_index_models_use_atlas_search_model_shape() {
    let models = regime_service::mongo_indexes::vector_search_index_models();
    let model = models
        .iter()
        .find(|model| model.collection_name == "feature_windows")
        .expect("feature_windows vector search index");

    assert_eq!(
        model.index.name.as_deref(),
        Some("feature_windows_vector_search")
    );
    assert!(matches!(
        model.index.index_type,
        Some(SearchIndexType::VectorSearch)
    ));

    let fields = model
        .index
        .definition
        .get_array("fields")
        .expect("vector fields");
    let field = fields[0].as_document().expect("vector field document");
    assert_eq!(field.get_str("type"), Ok("vector"));
    assert_eq!(field.get_str("path"), Ok("feature_vector"));
    assert_eq!(field.get_i32("numDimensions"), Ok(5));
    assert_eq!(field.get_str("similarity"), Ok("cosine"));
}
