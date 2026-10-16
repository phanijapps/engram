use engram_retrieval::RerankScorer;
use engram_runtime::CoreResult;
struct Legacy;
impl RerankScorer for Legacy {
    fn score(&self, query: &str, document: &str) -> CoreResult<f32> {
        Ok(if document.contains(query) { 1.0 } else { 0.0 })
    }
}
#[test]
fn legacy_scorers_keep_batch_order_and_fail_closed_for_code_input_admission() {
    let scorer = Legacy;
    assert!(!scorer.is_local());
    assert!(scorer.identity().is_none());
    assert!(scorer.inspect_query("question").is_err());
    assert!(scorer.inspect_pair("question", "document", 0).is_err());
    let docs = vec!["blue".into(), "red apple".into(), "green".into()];
    assert_eq!(
        scorer.score_batch("apple", &docs, 2).unwrap(),
        [0.0, 1.0, 0.0]
    );
    assert!(scorer.score_batch("apple", &docs, 0).is_err());
}

struct Nonfinite;
impl RerankScorer for Nonfinite {
    fn score(&self, _: &str, _: &str) -> CoreResult<f32> {
        Ok(f32::INFINITY)
    }
}
#[test]
fn default_batch_rejects_nonfinite_legacy_provider_results() {
    assert!(
        Nonfinite
            .score_batch("question", &["document".into()], 1)
            .is_err()
    );
}
