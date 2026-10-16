//! Exact local model input assembly and coverage accounting by byte offsets.
use crate::{EmbeddingProvider, code_representation::CodeRepresentation};
use engram_domain::{CodeContextFact, CodeTokenCoverage};
use engram_retrieval::CodeIndexConfig;
use engram_runtime::{CoreError, CoreResult};

pub(super) fn build_input(
    body: &str,
    facts: &[CodeContextFact],
    model: &dyn EmbeddingProvider,
    config: &CodeIndexConfig,
    representation: &CodeRepresentation,
) -> CoreResult<Option<(String, CodeTokenCoverage)>> {
    let context = engram_knowledge::code_context_text(facts);
    let prefix = if context.is_empty() {
        String::new()
    } else {
        format!("{context}\n\n")
    };
    let input = format!("{prefix}{}", body.trim());
    let alone = model.tokenize_untruncated(&prefix, false)?;
    let actual = model.tokenize_untruncated(&input, true)?;
    if input.trim().is_empty() || actual.offsets.is_empty() {
        return Err(CoreError::ValidationFailed {
            reason: "blank code ranges require grounded nonempty context before inference".into(),
        });
    }
    let context_tokens = actual
        .offsets
        .iter()
        .filter(|r| r.end <= prefix.len() as u64)
        .count() as u32;
    let code_tokens = actual.offsets.len() as u32 - context_tokens;
    if actual.tokenizer_id != representation.tokenizer_id
        || alone.tokenizer_id != representation.tokenizer_id
        || actual.window != representation.window
        || actual
            .offsets
            .iter()
            .any(|r| r.end < r.start || r.end > input.len() as u64)
    {
        return Err(CoreError::ValidationFailed {
            reason: "code/context tokenizer contract changed".into(),
        });
    }
    if alone.total_tokens() > config.context_token_budget as usize
        || context_tokens > config.context_token_budget
        || code_tokens > config.body_token_target
        || actual.total_tokens() > representation.window as usize
    {
        return Ok(None);
    }
    let coverage = CodeTokenCoverage {
        code_tokens,
        context_tokens,
        query_tokens: 0,
        special_tokens: actual.special_tokens,
        total_tokens: actual.total_tokens() as u32,
        truncated: false,
    };
    if !coverage.is_valid() {
        return Err(CoreError::ValidationFailed {
            reason: "code input token coverage is invalid".into(),
        });
    }
    Ok(Some((input, coverage)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use engram_domain::{CodeByteRange, CodeContextKind, EmbeddingSpace};
    use engram_retrieval::TextTokenization;
    struct Scalar;
    impl EmbeddingProvider for Scalar {
        fn is_local(&self) -> bool {
            true
        }
        fn provider_id(&self) -> &str {
            "local-test"
        }
        fn model_id(&self) -> &str {
            "scalar"
        }
        fn dimensions(&self) -> u32 {
            3
        }
        fn embedding_space(&self) -> EmbeddingSpace {
            EmbeddingSpace::new("local-test", "scalar", 3, "passage", None::<String>)
        }
        fn tokenize_untruncated(&self, text: &str, specials: bool) -> CoreResult<TextTokenization> {
            Ok(TextTokenization {
                offsets: text
                    .char_indices()
                    .map(|(start, ch)| CodeByteRange {
                        start: start as u64,
                        end: (start + ch.len_utf8()) as u64,
                    })
                    .collect(),
                special_tokens: if specials { 2 } else { 0 },
                window: 32,
                tokenizer_id: "scalar-v1".into(),
            })
        }
        fn embed_query(&self, _: &str) -> CoreResult<Vec<f32>> {
            unreachable!("input assembly does no inference")
        }
        fn embed_passage(&self, _: &str) -> CoreResult<Vec<f32>> {
            unreachable!("input assembly does no inference")
        }
    }
    #[test]
    fn whole_fact_budgets_and_actual_combined_special_tokens_are_enforced() {
        let config = CodeIndexConfig {
            body_token_target: 8,
            context_token_budget: 16,
        };
        let rep = crate::code_representation::code_representation(&Scalar, &config).unwrap();
        let fact = CodeContextFact {
            kind: CodeContextKind::Path,
            value: "x".into(),
            sources: vec![],
        };
        let (text, coverage) = build_input("ππππππππ", &[fact.clone()], &Scalar, &config, &rep)
            .unwrap()
            .unwrap();
        assert_eq!(text, "path: x\n\nππππππππ");
        assert_eq!(
            (
                coverage.code_tokens,
                coverage.context_tokens,
                coverage.special_tokens,
                coverage.total_tokens
            ),
            (8, 9, 2, 19)
        );
        let long = CodeContextFact {
            value: "long/path/that/cannot/fit".into(),
            ..fact
        };
        assert!(
            build_input("body", &[long], &Scalar, &config, &rep)
                .unwrap()
                .is_none()
        );
        assert!(
            build_input("123456789", &[], &Scalar, &config, &rep)
                .unwrap()
                .is_none()
        );
        let (_, plain) = build_input("body", &[], &Scalar, &config, &rep)
            .unwrap()
            .unwrap();
        assert_eq!((plain.context_tokens, plain.total_tokens), (0, 6));
        assert!(build_input(" \n", &[], &Scalar, &config, &rep).is_err());
        let (_, contextual_blank) = build_input(
            " \n",
            &[CodeContextFact {
                kind: CodeContextKind::Path,
                value: "x".into(),
                sources: vec![],
            }],
            &Scalar,
            &config,
            &rep,
        )
        .unwrap()
        .unwrap();
        assert_eq!(contextual_blank.code_tokens, 0);
        assert!(contextual_blank.context_tokens > 0);
    }
}
