//! Passage compatibility excludes query-only formatting and scorer settings.
use crate::EmbeddingProvider;
use engram_domain::EmbeddingSpace;
use engram_retrieval::CodeIndexConfig;
use engram_runtime::{CoreError, CoreResult};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeRepresentation {
    pub id: String,
    pub embedding_space: EmbeddingSpace,
    pub tokenizer_id: String,
    pub window: u32,
}

pub fn code_representation(
    model: &dyn EmbeddingProvider,
    config: &CodeIndexConfig,
) -> CoreResult<CodeRepresentation> {
    if !model.is_local() {
        return Err(CoreError::ProviderUnavailable {
            provider: "code representation requires a local embedding provider".into(),
        });
    }
    let tokens = model.tokenize_untruncated("", true)?;
    if tokens.tokenizer_id.is_empty()
        || config.body_token_target == 0
        || config
            .body_token_target
            .saturating_add(config.context_token_budget)
            .saturating_add(tokens.special_tokens)
            > tokens.window
    {
        return Err(CoreError::InvalidRequest { reason: "index.body_token_target / index.context_token_budget exceed the actual embedding tokenizer window with special tokens".into() });
    }
    let base = model.embedding_space();
    let identity = serde_json::to_vec(&(
        "ast-segments-v1",
        engram_code::SOURCE_COVERAGE_FINGERPRINT,
        "no-context-v1",
        "trim-passage-v1",
        &base.provider,
        &base.model,
        base.dimensions,
        &base.normalization,
        &tokens.tokenizer_id,
        config,
    ))
    .map_err(|e| CoreError::InvalidRequest {
        reason: e.to_string(),
    })?;
    let digest: String = Sha256::digest(identity)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let id = format!("code-passage-v1/{digest}");
    let embedding_space = EmbeddingSpace::new(
        base.provider,
        base.model,
        base.dimensions,
        &id,
        base.normalization,
    );
    Ok(CodeRepresentation {
        id,
        embedding_space,
        tokenizer_id: tokens.tokenizer_id,
        window: tokens.window,
    })
}
