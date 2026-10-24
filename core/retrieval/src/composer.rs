//! Storage-neutral context composition.
//!
//! This module owns the final fan-in step after memory, knowledge, vector,
//! graph, hierarchy, or belief sources have already produced policy-checked
//! candidates. It applies fusion once, enforces final context limits once, and
//! preserves omissions plus degraded-source reports without depending on any
//! concrete adapter.

use engram_domain::*;
use engram_runtime::CoreResult;

use crate::RetrievalFusion;

/// Inputs required to compose a final retrieval response.
///
/// Candidate producers are expected to enforce their own scope and policy rules
/// before filling this structure. The composer only owns shared ranking, final
/// budget enforcement, omitted-result merging, and source-failure preservation.
pub struct RetrievalCompositionInput<'a> {
    pub request: &'a RetrievalRequest,
    pub fusion: &'a dyn RetrievalFusion,
    pub reranker: Option<&'a dyn crate::RetrievalReranker>,
    pub candidates: Vec<RetrievalResult>,
    pub omitted: Vec<OmittedResult>,
    pub source_failures: Vec<RetrievalSourceFailure>,
    pub created_at: Timestamp,
}

/// Applies shared fusion and context-budget rules to produce `ContextPayload`.
///
/// The composer deliberately disables request-level fusion truncation so all
/// candidates can be ranked together before the final context limit records
/// budget omissions. This keeps optional source failures and policy omissions
/// visible to callers while staying independent of storage implementation.
pub fn compose_context(input: RetrievalCompositionInput<'_>) -> CoreResult<ContextPayload> {
    let max_items = effective_max_items(input.request);
    let mut fusion_request = input.request.clone();
    fusion_request.limit = None;
    let fused_results = input.fusion.fuse(&fusion_request, input.candidates)?;

    // Optional cross-encoder rerank between fusion and budget.
    let ranked_results = match input.reranker {
        Some(reranker) => reranker.rerank(input.request, fused_results)?,
        None => fused_results,
    };

    let mut items = Vec::new();
    let mut omitted = input.omitted;
    // Shared budget enforcement (MCCL-audit F3): token and byte budgets are
    // honored AFTER ranking alongside the item budget, so weak candidates
    // drop before strong ones regardless of storage order. An item that
    // alone exceeds a budget is OMITTED (BudgetExceeded) — never silently
    // truncated to fit. `max_bytes` counts exact serialized UTF-8 content
    // bytes; `max_tokens` uses the deterministic documented estimator
    // (`estimate_context_tokens`), an approximation, not a model tokenizer.
    let max_tokens = input
        .request
        .budget
        .as_ref()
        .and_then(|b| b.max_tokens)
        .map(|t| t as u64);
    let max_bytes = input
        .request
        .budget
        .as_ref()
        .and_then(|b| b.max_bytes)
        .map(|b| b as u64);
    let mut used_tokens = 0u64;
    let mut used_bytes = 0u64;
    for (index, result) in ranked_results.into_iter().enumerate() {
        if index >= max_items {
            omitted.push(omitted_fused_result(&result, OmittedReason::BudgetExceeded));
            continue;
        }
        let item_tokens = crate::tokens::estimate_context_tokens(&result.content) as u64;
        let item_bytes = result.content.len() as u64;
        let tokens_fit = max_tokens
            .map(|max| used_tokens + item_tokens <= max)
            .unwrap_or(true);
        let bytes_fit = max_bytes
            .map(|max| used_bytes + item_bytes <= max)
            .unwrap_or(true);
        if !tokens_fit || !bytes_fit {
            omitted.push(omitted_fused_result(&result, OmittedReason::BudgetExceeded));
            continue;
        }
        used_tokens += item_tokens;
        used_bytes += item_bytes;
        items.push(result);
    }

    Ok(ContextPayload {
        items,
        budget: input.request.budget.clone(),
        omitted,
        source_failures: input.source_failures,
        created_at: input.created_at,
    })
}

fn effective_max_items(request: &RetrievalRequest) -> usize {
    let limit = request.limit.unwrap_or(u32::MAX);
    let budget_limit = request
        .budget
        .as_ref()
        .and_then(|budget| budget.max_items)
        .unwrap_or(u32::MAX);
    limit.min(budget_limit) as usize
}

fn omitted_fused_result(result: &RetrievalResult, reason: OmittedReason) -> OmittedResult {
    OmittedResult {
        target_type: result.target_type.clone(),
        target_id: result.target_id.clone(),
        reason,
    }
}
