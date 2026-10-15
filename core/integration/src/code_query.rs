//! Query formatting has an independent identity and never changes passage IDs.
use crate::EmbeddingProvider;
use engram_domain::CodeTokenCoverage;
use engram_retrieval::CodeQueryInstruction;
use engram_runtime::{CoreError, CoreResult};
use sha2::{Digest, Sha256};

pub struct PreparedCodeQuery {
    pub text: String,
    pub representation_id: String,
    pub tokenizer_id: String,
    pub window: u32,
    pub coverage: CodeTokenCoverage,
}

pub fn prepare_code_query(
    model: &dyn EmbeddingProvider,
    query: &str,
    instruction: CodeQueryInstruction,
) -> CoreResult<PreparedCodeQuery> {
    if query.len() > 16_384 || query.trim().is_empty() {
        return Err(CoreError::InvalidRequest {
            reason: "code query must be nonempty and at most 16384 UTF-8 bytes".into(),
        });
    }
    if !model.is_local() {
        return Err(CoreError::ProviderUnavailable {
            provider: "local code query model".into(),
        });
    }
    let text = format!("{}{}", instruction.prefix(), query.trim());
    let tokens = model.tokenize_untruncated(&text, true)?;
    if tokens.tokenizer_id.is_empty()
        || tokens.total_tokens() > tokens.window as usize
        || tokens.offsets.len() > 2048
        || tokens.special_tokens > 64
    {
        return Err(CoreError::InvalidRequest { reason:"complete code query including instruction/special tokens exceeds the embedding window".into() });
    }
    let identity = serde_json::to_vec(&(
        "code-query-v1",
        instruction,
        &tokens.tokenizer_id,
        tokens.window,
        model.embedding_space(),
    ))
    .map_err(|e| CoreError::InvalidRequest {
        reason: e.to_string(),
    })?;
    let digest: String = Sha256::digest(identity)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let total_tokens = tokens.total_tokens() as u32;
    Ok(PreparedCodeQuery {
        text,
        tokenizer_id: tokens.tokenizer_id,
        window: tokens.window,
        representation_id: format!("code-query-v1/{digest}"),
        coverage: CodeTokenCoverage {
            code_tokens: 0,
            context_tokens: 0,
            query_tokens: tokens.offsets.len() as u32,
            special_tokens: tokens.special_tokens,
            total_tokens,
            truncated: false,
        },
    })
}
