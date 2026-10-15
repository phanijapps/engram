//! Fresh exact authority checks; an old score never follows changed bytes.
use crate::{EngramProvider, code_candidates::CodeCandidate};
use engram_domain::{Requester, Scope};
use engram_runtime::{CoreError, CoreResult};
pub(super) async fn current(
    provider: &EngramProvider,
    candidate: &CodeCandidate,
    scope: &Scope,
    requester: &Requester,
    representation: &str,
) -> CoreResult<bool> {
    let port = provider
        .code_segments()
        .ok_or_else(|| CoreError::ProviderUnavailable {
            provider: "code segment authority".into(),
        })?;
    let Some(resolved) = port
        .resolve_code_segment(&candidate.segment.id, scope, requester, representation)
        .await?
    else {
        return Ok(false);
    };
    let source = provider
        .require_knowledge()?
        .code_sources()
        .ok_or_else(|| CoreError::ProviderUnavailable {
            provider: "code source authority".into(),
        })?;
    let revision = source
        .indexed_code_revision(&candidate.segment.file, scope, requester)
        .await?;
    Ok(resolved.segment == candidate.segment
        && resolved.parent == candidate.parent
        && revision == candidate.indexed_revision)
}
pub(super) fn changed() -> CoreError {
    CoreError::Conflict {
        reason: "code source changed during evidence preparation".into(),
    }
}
