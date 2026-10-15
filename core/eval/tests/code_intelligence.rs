// STUB: AC10, AC11 — frozen parent-level retrieval metrics.
use engram_eval::code_intelligence::{CodeJudgment, ObservedCodeUnit, evaluate_code_query};

fn judgment(id: &str, grade: u8, ranges: &[(u64, u64)]) -> CodeJudgment {
    CodeJudgment {
        parent_id: id.into(),
        grade,
        required_ranges: ranges.to_vec(),
    }
}
fn observed(id: &str, ranges: &[(u64, u64)]) -> ObservedCodeUnit {
    ObservedCodeUnit {
        parent_id: id.into(),
        excerpts: ranges.to_vec(),
    }
}

#[test]
fn all_required_parents_count_and_overlap_does_not_inflate_range_coverage() {
    let labels = vec![judgment("a", 3, &[(10, 30)]), judgment("b", 3, &[(0, 10)])];
    let candidates = vec!["a".into(), "a".into(), "b".into()];
    let primary = vec![observed("a", &[(10, 22), (18, 25)])];
    let m = evaluate_code_query(&labels, &candidates, &primary, &[], true).unwrap();
    assert_eq!(m.candidate_recall_at_50, Some(1.0));
    assert_eq!(m.required_unit_recall_at_10, Some(0.5));
    assert_eq!(m.required_range_coverage, Some(0.5));
    assert!(!m.all_required_recovered);
    assert_eq!(m.mrr_at_10, 1.0);
    assert!((m.ndcg_at_10 - 0.6131471927654584).abs() < 1e-10);
}

#[test]
fn unvalidated_inspection_is_not_positive_evidence() {
    let m = evaluate_code_query(
        &[judgment("a", 3, &[(0, 5)])],
        &["a".into()],
        &[observed("a", &[(0, 5)])],
        &[],
        false,
    )
    .unwrap();
    assert_eq!(m.candidate_recall_at_50, Some(1.0));
    assert_eq!(m.required_unit_recall_at_10, Some(0.0));
    assert_eq!(m.required_range_coverage, Some(0.0));
    assert_eq!(m.ndcg_at_10, 0.0);
    assert_eq!(m.mrr_at_10, 0.0);
}

#[test]
fn negatives_have_no_artificial_positive_recall() {
    let m = evaluate_code_query(&[], &[], &[], &[], true).unwrap();
    assert_eq!(m.required_unit_recall_at_10, None);
    assert_eq!(m.candidate_recall_at_50, None);
    assert!(!m.all_required_recovered);
}

#[test]
fn unjudged_and_duplicate_parents_do_not_inflate_ndcg() {
    let m = evaluate_code_query(
        &[judgment("a", 3, &[(0, 5)])],
        &[],
        &[
            observed("unknown", &[]),
            observed("a", &[]),
            observed("a", &[]),
        ],
        &[],
        true,
    )
    .unwrap();
    assert_eq!(m.unjudged_primary_items, 1);
    assert_eq!(m.mrr_at_10, 0.5);
    assert!((m.ndcg_at_10 - 1.0 / 3.0_f64.log2()).abs() < 1e-10);
}

#[test]
fn malformed_labels_are_rejected() {
    assert!(evaluate_code_query(&[judgment("a", 4, &[])], &[], &[], &[], true).is_err());
    assert!(evaluate_code_query(&[judgment("a", 3, &[(5, 5)])], &[], &[], &[], true).is_err());
    assert!(
        evaluate_code_query(
            &[judgment("a", 3, &[]), judgment("a", 2, &[])],
            &[],
            &[],
            &[],
            true
        )
        .is_err()
    );
}

#[test]
fn candidate_budget_counts_distinct_parents_and_final_budget_counts_items() {
    let mut candidates = vec!["filler".into(); 100];
    candidates.extend((0..49).map(|i| format!("other-{i}")));
    candidates.push("required".into());
    let mut primary: Vec<_> = (0..10)
        .map(|i| observed(&format!("other-{i}"), &[]))
        .collect();
    primary.push(observed("required", &[(0, 5)]));
    let m = evaluate_code_query(
        &[judgment("required", 3, &[(0, 5)])],
        &candidates,
        &primary,
        &[],
        true,
    )
    .unwrap();
    assert_eq!(m.candidate_recall_at_50, Some(0.0));
    assert_eq!(m.required_unit_recall_at_10, Some(0.0));
    candidates.remove(100);
    let m = evaluate_code_query(
        &[judgment("required", 3, &[(0, 5)])],
        &candidates,
        &primary,
        &[],
        true,
    )
    .unwrap();
    assert_eq!(m.candidate_recall_at_50, Some(1.0));
}

#[test]
fn supporting_excerpts_cover_bytes_without_recovering_a_primary_parent() {
    let labels = [judgment("required", 3, &[(0, 10)])];
    let supporting = [observed("required", &[(0, 10)])];
    let m = evaluate_code_query(&labels, &[], &[], &supporting, true).unwrap();
    assert_eq!(m.required_unit_recall_at_10, Some(0.0));
    assert_eq!(m.required_range_coverage, Some(1.0));
    assert!(!m.all_required_recovered);
    let m = evaluate_code_query(&labels, &[], &[], &supporting, false).unwrap();
    assert_eq!(m.required_range_coverage, Some(0.0));
}
