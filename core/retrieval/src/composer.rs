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

    let mut items = Vec::new();
    let mut omitted = input.omitted;
    for (index, result) in fused_results.into_iter().enumerate() {
        if index >= max_items {
            omitted.push(omitted_fused_result(&result, OmittedReason::BudgetExceeded));
            continue;
        }
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
