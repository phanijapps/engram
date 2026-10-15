use engram_retrieval::{CodeRankedSegment, code_parent_order, code_scoring_pool};

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
