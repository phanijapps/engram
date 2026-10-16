//! Admission and actual local input contracts before any inference.
use crate::{
    EngramProvider,
    code_configuration::{ResolvedCodeConfig, resolve_code_config},
    code_query::prepare_code_query,
};
use engram_domain::*;
use engram_retrieval::{
    CodeIntelligenceConfig, CodeScorerFailurePolicy, RerankScorer, RerankScorerIdentity,
};
use engram_runtime::{CoreError, CoreResult};
use std::sync::Arc;

pub(super) struct Admission {
    pub config: CodeIntelligenceConfig,
    pub resolved: ResolvedCodeConfig,
    pub requested: CodeEvidenceReranker,
    pub scorer: Option<Arc<dyn RerankScorer>>,
    pub identity: Option<RerankScorerIdentity>,
    pub state: CodeScorerState,
}
pub(super) fn unavailable() -> CoreError {
    CoreError::ProviderUnavailable {
        provider: "verified local code scorer".into(),
    }
}
pub(super) fn invalid(reason: &str) -> CoreError {
    CoreError::InvalidRequest {
        reason: reason.into(),
    }
}

pub(super) fn prepare(
    provider: &EngramProvider,
    query: &CodeEvidenceQuery,
) -> CoreResult<Admission> {
    query.validate().map_err(|e| invalid(&e.to_string()))?;
    let resolved = provider
        .code_configuration()
        .cloned()
        .map(Ok)
        .unwrap_or_else(|| resolve_code_config(None, None, None))?;
    let config = resolved.effective.for_query(query)?;
    if provider.code_segments().is_none()
        || provider
            .knowledge()
            .is_none_or(|k| k.document_heads().is_none() || k.code_sources().is_none())
    {
        return Err(CoreError::CapabilityUnsupported {
            capability: "code_evidence".into(),
            reason: "verified source and segment authority unavailable".into(),
        });
    }
    let requested = if config
        .fusion
        .rerank
        .as_ref()
        .is_some_and(|r| r.strategy == RerankStrategy::CrossEncoder)
    {
        CodeEvidenceReranker::CrossEncoder
    } else {
        CodeEvidenceReranker::None
    };
    // Both actual tokenizers admit the complete query before vector inference.
    let embedding = provider.require_embedding_provider()?;
    prepare_code_query(embedding.as_ref(), &query.query, config.query.instruction)?;
    let mut admission = Admission {
        config,
        resolved,
        requested,
        scorer: None,
        identity: None,
        state: CodeScorerState::Disabled,
    };
    if requested == CodeEvidenceReranker::None {
        return Ok(admission);
    }
    admission.state = CodeScorerState::Unavailable;
    let result = (|| {
        // Unknown/nonlocal inspectors receive neither query nor source text.
        let scorer = provider
            .code_scorer()
            .filter(|s| s.is_local() && s.supports_code_input())
            .ok_or_else(unavailable)?;
        let id = scorer
            .identity()
            .filter(|id| {
                id.model_id == admission.config.local_scorer.model_id
                    && !id.manifest_id.is_empty()
                    && !id.tokenizer_id.is_empty()
                    && !id.input_contract_id.is_empty()
                    && id.score_contract_id == "logits-column-0/raw-v1"
            })
            .ok_or_else(unavailable)?;
        if admission.config.local_scorer.pair_token_limit > id.window {
            return Err(invalid(
                "local_scorer.pair_token_limit exceeds actual scorer window",
            ));
        }
        let tokens = scorer.inspect_query(&query.query)?;
        if tokens.tokenizer_id != id.tokenizer_id
            || tokens.window != id.window
            || tokens.special_tokens != 0
            || tokens.offsets.iter().any(|r| {
                r.end < r.start
                    || r.end > query.query.len() as u64
                    || !query.query.is_char_boundary(r.start as usize)
                    || !query.query.is_char_boundary(r.end as usize)
            })
        {
            return Err(unavailable());
        }
        if tokens.total_tokens() > admission.config.local_scorer.query_token_limit as usize {
            return Err(invalid(
                "query exceeds actual local scorer query_token_limit",
            ));
        }
        Ok((scorer.clone(), id))
    })();
    match result {
        Ok((scorer, id)) => {
            admission.scorer = Some(scorer);
            admission.identity = Some(id);
            admission.state = CodeScorerState::Ready;
        }
        Err(error @ CoreError::InvalidRequest { .. }) => return Err(error),
        Err(error)
            if admission.config.local_scorer.on_failure == CodeScorerFailurePolicy::Error =>
        {
            return Err(error);
        }
        Err(_) => {}
    }
    Ok(admission)
}
