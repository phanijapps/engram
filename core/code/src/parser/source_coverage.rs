//! Pure UTF-8 partitioning and explicit source-slice coverage.
use super::chunking::ChunkCandidate;
use engram_domain::{CodeByteRange, CodeChunkRole, CodeParseQuality};
use engram_runtime::{CoreError, CoreResult};

#[derive(Debug, Clone)]
pub struct CoveredCodeCandidate {
    pub candidate: ChunkCandidate,
    pub role: CodeChunkRole,
    pub ranges: Vec<CodeByteRange>,
    pub declaration_range: Option<CodeByteRange>,
    pub parse_quality: CodeParseQuality,
    pub complete: bool,
    pub unsupported_features: Vec<String>,
}

/// Distinguishes no grammar attempt from a parser/setup failure. Neither
/// outcome prevents the ingest adapter from preserving an exact snapshot.
pub fn source_parse_outcome(
    attempt: Option<CoreResult<tree_sitter::Tree>>,
) -> (Option<tree_sitter::Tree>, CodeParseQuality) {
    match attempt {
        None => (None, CodeParseQuality::Fallback),
        Some(Err(_)) => (None, CodeParseQuality::Failed),
        Some(Ok(tree)) => {
            let quality = if tree.root_node().has_error() {
                CodeParseQuality::Recovered
            } else {
                CodeParseQuality::Clean
            };
            (Some(tree), quality)
        }
    }
}

pub fn snapshot_ranges(text: &str, max_bytes: usize) -> CoreResult<Vec<CodeByteRange>> {
    if max_bytes == 0 {
        return Err(invalid("snapshot bound must be positive"));
    }
    if text.is_empty() {
        return Ok(vec![CodeByteRange { start: 0, end: 0 }]);
    }
    let mut result = vec![];
    let mut start = 0;
    while start < text.len() {
        let mut end = start.saturating_add(max_bytes).min(text.len());
        while end > start && !text.is_char_boundary(end) {
            end -= 1;
        }
        if end == start {
            return Err(invalid("snapshot bound cannot contain a UTF-8 scalar"));
        }
        result.push(CodeByteRange {
            start: start as u64,
            end: end as u64,
        });
        start = end;
    }
    Ok(result)
}

/// Complement of coverage, merging overlaps but never treating a clipped span
/// as a full declaration. Source byte validity is checked before slicing.
pub fn source_residual_ranges(
    byte_length: u64,
    covered: &[CodeByteRange],
) -> CoreResult<Vec<CodeByteRange>> {
    let mut sorted = covered.to_vec();
    sorted.sort_by_key(|r| (r.start, r.end));
    let mut cursor = 0;
    let mut result = vec![];
    for range in sorted {
        if range.end < range.start || range.end > byte_length {
            return Err(invalid("coverage exceeds source bytes"));
        }
        if range.start > cursor {
            result.push(CodeByteRange {
                start: cursor,
                end: range.start,
            });
        }
        cursor = cursor.max(range.end);
    }
    if cursor < byte_length {
        result.push(CodeByteRange {
            start: cursor,
            end: byte_length,
        });
    }
    Ok(result)
}

fn invalid(reason: &str) -> CoreError {
    CoreError::InvalidRequest {
        reason: reason.into(),
    }
}
