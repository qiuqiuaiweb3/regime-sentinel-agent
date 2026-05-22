use mongodb::bson::Bson;

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
