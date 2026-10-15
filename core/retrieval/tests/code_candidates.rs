use engram_retrieval::{
    CodeRankedSegment, code_parent_order, code_scoring_pool, code_scoring_pool_with_parent_limit,
};

fn candidates() -> Vec<CodeRankedSegment> {
    [
        ("a1", "a", 0.9),
        ("a2", "a", 0.8),
        ("b1", "b", 0.7),
        ("c1", "c", 0.6),
        ("b2", "b", 0.5),
        ("a3", "a", 0.4),
    ]
    .into_iter()
    .map(|(segment, parent, score)| CodeRankedSegment {
        segment_id: segment.into(),
        parent_id: parent.into(),
        fusion_score: score,
    })
    .collect()
}

#[test]
fn pool_gives_parents_one_slot_before_round_robin_extra_segments() {
    let rows = candidates();
    let pool = code_scoring_pool(&rows, 5).unwrap();
    assert_eq!(pool.indices, [0, 2, 3, 1, 4]);
    assert_eq!(pool.omitted_segments, 1);
    assert_eq!(pool.omitted_parents, 0);
    let pool = code_scoring_pool(&rows, 2).unwrap();
    assert_eq!(pool.indices, [0, 2]);
    assert_eq!(pool.omitted_segments, 4);
    assert_eq!(pool.omitted_parents, 1);
}

#[test]
fn successful_scores_aggregate_maxima_and_preserve_rrf_parent_ties() {
    let rows = candidates();
    let pool = code_scoring_pool(&rows, 5).unwrap();
    let ranked = code_parent_order(&rows, Some((&pool, &[0.1, 0.9, 0.9, 0.8, 0.2]))).unwrap();
    assert_eq!(
        ranked
            .iter()
            .map(|r| r.parent_id.as_str())
            .collect::<Vec<_>>(),
        ["b", "c", "a"]
    );
    assert_eq!(ranked[2].segment_indices, [1, 0]);
    assert_eq!(ranked[2].score, 0.8);
    let pool = code_scoring_pool(&rows, 2).unwrap();
    assert_eq!(
        code_parent_order(&rows, Some((&pool, &[1.0, 1.0])))
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn none_or_failure_uses_all_rrf_parents_and_rejects_partial_invalid_scores() {
    let rows = candidates();
    let ranked = code_parent_order(&rows, None).unwrap();
    assert_eq!(
        ranked
            .iter()
            .map(|r| r.parent_id.as_str())
            .collect::<Vec<_>>(),
        ["a", "b", "c"]
    );
    assert_eq!(ranked[0].segment_indices, [0, 1, 5]);
    let pool = code_scoring_pool(&rows, 3).unwrap();
    assert!(code_parent_order(&rows, Some((&pool, &[0.1, 0.2]))).is_err());
    assert!(code_parent_order(&rows, Some((&pool, &[0.1, f32::NAN, 0.2]))).is_err());
    assert!(code_scoring_pool(&rows, 0).is_err());
}

#[test]
fn fifty_parent_frontier_leaves_room_for_extra_segment_views() {
    let mut rows: Vec<_> = (0..60)
        .map(|i| CodeRankedSegment {
            segment_id: format!("head-{i}"),
            parent_id: format!("parent-{i}"),
            fusion_score: 1.0 / (i + 1) as f32,
        })
        .collect();
    for i in 0..2 {
        rows.push(CodeRankedSegment {
            segment_id: format!("tail-{i}"),
            parent_id: format!("parent-{i}"),
            fusion_score: 0.001,
        });
    }
    let pool = code_scoring_pool_with_parent_limit(&rows, 50, 75).unwrap();
    assert_eq!(pool.indices.len(), 52);
    assert_eq!(
        &pool.indices[..50],
        code_scoring_pool(&rows, 50).unwrap().indices.as_slice()
    );
    assert!(pool.indices.contains(&60) && pool.indices.contains(&61));
    assert!(!pool.indices.iter().any(|&i| (50..60).contains(&i)));
    assert_eq!(pool.omitted_parents, 10);
    let mut scores = vec![0.0; pool.indices.len()];
    scores[pool.indices.iter().position(|&i| i == 60).unwrap()] = 1.0;
    let ranked = code_parent_order(&rows, Some((&pool, &scores))).unwrap();
    assert_eq!(ranked[0].parent_id, "parent-0");
    assert_eq!(ranked[0].segment_indices[0], 60);
    assert!(code_scoring_pool_with_parent_limit(&rows, 0, 75).is_err());
    assert!(code_scoring_pool_with_parent_limit(&rows, 50, 0).is_err());
    assert_eq!(
        code_scoring_pool_with_parent_limit(&rows, 50, 3)
            .unwrap()
            .indices
            .len(),
        3
    );
}

#[test]
fn extra_coverage_preserves_container_first_view_without_promoting_its_tail() {
    let rows = candidates();
    let pool =
        engram_retrieval::code_scoring_pool_with_coverage(&rows, 3, 6, |parent| parent == "b")
            .unwrap();
    assert_eq!(pool.indices, [0, 2, 3, 4]);
    assert_eq!(pool.omitted_parents, 0);
    assert_eq!(pool.omitted_segments, 2);
    let ranked = code_parent_order(&rows, Some((&pool, &[0.1, 0.2, 0.3, 0.9]))).unwrap();
    assert_eq!(ranked[0].parent_id, "b");
    assert_eq!(ranked[0].segment_indices, [4, 2]);
}
