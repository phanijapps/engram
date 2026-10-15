//! Formatting identity is independent of passage storage and never double prefixes.
use engram_domain::{CodeByteRange, CodeIntelligenceProfile, EmbeddingSpace};
use engram_integration::{
    EmbeddingProvider, code_query::prepare_code_query, code_representation::code_representation,
};
use engram_retrieval::{CodeIntelligenceConfig, CodeQueryInstruction, TextTokenization};
use engram_runtime::CoreResult;
use std::sync::atomic::{AtomicUsize, Ordering};
struct Scalar(AtomicUsize);
impl EmbeddingProvider for Scalar {
    fn is_local(&self) -> bool {
        true
    }
    fn provider_id(&self) -> &str {
        "test-local"
    }
    fn model_id(&self) -> &str {
        "scalar"
    }
    fn dimensions(&self) -> u32 {
        3
    }
    fn embedding_space(&self) -> EmbeddingSpace {
        EmbeddingSpace::new("test-local", "scalar", 3, "passage", None::<String>)
    }
    fn tokenize_untruncated(&self, text: &str, specials: bool) -> CoreResult<TextTokenization> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(TextTokenization {
            offsets: text
                .char_indices()
                .map(|(start, ch)| CodeByteRange {
                    start: start as u64,
                    end: (start + ch.len_utf8()) as u64,
                })
                .collect(),
            special_tokens: if specials { 2 } else { 0 },
            window: 512,
            tokenizer_id: "scalar-v1".into(),
        })
    }
    fn embed_query(&self, _: &str) -> CoreResult<Vec<f32>> {
        panic!("preflight must not infer")
    }
    fn embed_passage(&self, _: &str) -> CoreResult<Vec<f32>> {
        panic!("preflight must not infer")
    }
}
#[test]
fn three_formats_have_distinct_query_identity_and_identical_passage_identity() {
    let model = Scalar(AtomicUsize::new(0));
    let mut config = CodeIntelligenceConfig::for_profile(CodeIntelligenceProfile::BalancedV1);
    let passage = code_representation(&model, &config.index).unwrap();
    let mut ids = std::collections::HashSet::new();
    for (instruction, expected) in [
        (CodeQueryInstruction::LegacyV1, "query: find writes"),
        (CodeQueryInstruction::None, "find writes"),
        (
            CodeQueryInstruction::BgeRetrievalV1,
            "Represent this sentence for searching relevant passages: find writes",
        ),
    ] {
        config.query.instruction = instruction;
        let query = prepare_code_query(&model, "  find writes  ", instruction).unwrap();
        assert_eq!(query.text, expected);
        assert_eq!(
            query.coverage.total_tokens as usize,
            expected.chars().count() + 2
        );
        assert!(ids.insert(query.representation_id.clone()));
        assert_eq!(
            query.representation_id,
            prepare_code_query(&model, "another question", instruction)
                .unwrap()
                .representation_id
        );
        assert_eq!(passage, code_representation(&model, &config.index).unwrap());
        assert!(
            model.embed_prepared_code_query(&query).is_err(),
            "unknown preparation contract fails closed"
        );
    }
}
#[test]
fn byte_limit_precedes_tokenization_and_specials_instructions_count_toward_window() {
    let model = Scalar(AtomicUsize::new(0));
    assert!(prepare_code_query(&model, &"🦀".repeat(4097), CodeQueryInstruction::None).is_err());
    assert_eq!(model.0.load(Ordering::SeqCst), 0);
    assert!(prepare_code_query(&model, &"x".repeat(510), CodeQueryInstruction::None).is_ok());
    assert!(prepare_code_query(&model, &"x".repeat(511), CodeQueryInstruction::None).is_err());
    assert!(prepare_code_query(&model, &"x".repeat(510), CodeQueryInstruction::LegacyV1).is_err());
}
