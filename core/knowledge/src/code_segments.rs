//! Knowledge-owned derived representation lifecycle and canonical resolution.
use async_trait::async_trait;
use engram_domain::*;
use engram_runtime::{CoreError, CoreResult};

/// Storage-issued mutation fence, independent of the canonical source attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeSegmentLease {
    pub inventory: CodeSegmentInventory,
    pub revision: u64,
}
pub struct CodeSegmentSet {
    pub lease: CodeSegmentLease,
    pub segments: Vec<CodeEmbeddingSegment>,
}
pub struct ResolvedCodeSegment {
    pub segment: CodeEmbeddingSegment,
    pub parent: KnowledgeChunk,
}

#[async_trait]
pub trait CodeSegmentRepository: Send + Sync {
    /// Monotonic for both source authority and derived representation changes.
    async fn code_index_revision(&self) -> CoreResult<u64>;
    async fn code_segment_inventory(
        &self,
        file: &CodeFileKey,
        representation: &str,
    ) -> CoreResult<Option<CodeSegmentLease>>;
    async fn begin_code_segments(
        &self,
        inventory: CodeSegmentInventory,
        expected: Option<&CodeSegmentLease>,
    ) -> CoreResult<CodeSegmentLease>;
    async fn write_code_segments(
        &self,
        lease: &CodeSegmentLease,
        segments: Vec<CodeEmbeddingSegment>,
    ) -> CoreResult<()>;
    async fn activate_code_segments(&self, lease: &CodeSegmentLease) -> CoreResult<()>;
    async fn fail_code_segments(&self, lease: &CodeSegmentLease) -> CoreResult<()>;
    /// Authorized operator preparation can resume a partial updating inventory.
    /// Query consumers use `resolve_code_segment`, which requires activation.
    async fn load_current_code_segments(
        &self,
        file: &CodeFileKey,
        representation: &str,
        requester: &Requester,
    ) -> CoreResult<Option<CodeSegmentSet>>;
    async fn resolve_code_segment(
        &self,
        id: &str,
        scope: &Scope,
        requester: &Requester,
        representation: &str,
    ) -> CoreResult<Option<ResolvedCodeSegment>>;
    /// Bounded maintenance recheck for one updating/active representation target.
    async fn prepare_code_segment(
        &self,
        id: &str,
        scope: &Scope,
        requester: &Requester,
        representation: &str,
    ) -> CoreResult<Option<ResolvedCodeSegment>>;
}

/// Derives exact segment text from authorized canonical bytes, never snapshot
/// or cached vector text. Synthetic parent separators are not source coverage.
pub fn code_segment_text(parent: &KnowledgeChunk, ranges: &[CodeByteRange]) -> CoreResult<String> {
    let invalid = || CoreError::ValidationFailed {
        reason: "segment ranges do not map to canonical parent UTF-8 bytes".into(),
    };
    let code = parent.code.as_ref().ok_or_else(invalid)?;
    let mut mapped = Vec::new();
    let mut offset = 0usize;
    for range in &code.ranges {
        let length = usize::try_from(range.end.checked_sub(range.start).ok_or_else(invalid)?)
            .map_err(|_| invalid())?;
        let end = offset.checked_add(length).ok_or_else(invalid)?;
        parent.text.get(offset..end).ok_or_else(invalid)?;
        mapped.push((*range, offset));
        offset = end
            .checked_add(code.slice_separator.len())
            .ok_or_else(invalid)?;
    }
    if mapped.is_empty()
        || offset.saturating_sub(code.slice_separator.len()) != parent.text.len()
        || ranges.is_empty()
    {
        return Err(invalid());
    }
    let mut result = Vec::new();
    let mut previous = 0;
    for range in ranges {
        if range.start < previous || range.end < range.start {
            return Err(invalid());
        }
        let (outer, offset) = mapped
            .iter()
            .find(|(outer, _)| range.start >= outer.start && range.end <= outer.end)
            .ok_or_else(invalid)?;
        let start = offset + usize::try_from(range.start - outer.start).map_err(|_| invalid())?;
        let end = offset + usize::try_from(range.end - outer.start).map_err(|_| invalid())?;
        result.push(parent.text.get(start..end).ok_or_else(invalid)?);
        previous = range.end;
    }
    Ok(result.join(&code.slice_separator))
}
