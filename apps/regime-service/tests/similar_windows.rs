use mongodb::bson::Bson;

#[test]
fn similar_windows_pipeline_uses_vector_search_with_slug_filter() {
    let pipeline = regime_service::similar_windows::similar_windows_pipeline(
        "btc-updown-5m",
        &[0.03, 0.42, 0.31, 0.03, 2.1],
        3,
    );

    assert_eq!(pipeline.len(), 2);

    let vector_search = pipeline[0]
        .get_document("$vectorSearch")
        .expect("vector search stage");
    assert_eq!(
        vector_search.get_str("index"),
        Ok("feature_windows_vector_search")
    );
    assert_eq!(vector_search.get_str("path"), Ok("feature_vector"));
    assert_eq!(vector_search.get_i32("limit"), Ok(3));
    assert_eq!(vector_search.get_i32("numCandidates"), Ok(60));
    assert_eq!(
        vector_search
            .get_document("filter")
            .and_then(|filter| filter.get_str("slug")),
        Ok("btc-updown-5m")
    );

    let query_vector = vector_search
        .get_array("queryVector")
        .expect("query vector");
    assert_eq!(query_vector[0], Bson::Double(0.03));
    assert_eq!(query_vector[4], Bson::Double(2.1));

    let project = pipeline[1].get_document("$project").expect("project stage");
    assert_eq!(project.get_i32("_id"), Ok(0));
    assert_eq!(project.get_i32("slug"), Ok(1));
    assert_eq!(project.get_i32("window_ts"), Ok(1));
    assert_eq!(
        project
            .get_document("score")
            .and_then(|score| score.get_str("$meta")),
        Ok("vectorSearchScore")
    );
}
