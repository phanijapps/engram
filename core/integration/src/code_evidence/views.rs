//! Lazy exact-range scorer continuations. No model silently truncates a body.
use crate::code_candidates::CodeCandidate;
use engram_domain::*;
use engram_retrieval::{CodeLocalScorerConfig, RerankScorer};
use engram_runtime::{CoreError, CoreResult};
use sha2::{Digest, Sha256};

pub(super) const CONTRACT: &str = "code-score-views-v1/untrimmed-source/native-separator/whole-facts/lazy-original-pool-first/utf8-token-cuts-64k";
const MAX_BODY_BYTES: usize = 65536;
pub(super) struct View {
    pub id: String,
    pub original: usize,
    pub ranges: Vec<CodeByteRange>,
    pub context: Vec<CodeContextFact>,
    pub text: String,
    pub coverage: CodeTokenCoverage,
}
pub(super) struct Cursor {
    original: usize,
    remaining: Vec<CodeByteRange>,
}
fn invalid() -> CoreError {
    CoreError::ValidationFailed {
        reason: "scorer continuation cannot map a bounded complete input".into(),
    }
}
fn boundary(text: &str, mut end: usize) -> usize {
    end = end.min(text.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    end
}
fn prefix(facts: &[CodeContextFact]) -> String {
    let text = engram_knowledge::code_context_text(facts);
    if text.is_empty() {
        text
    } else {
        format!("{text}\n\n")
    }
}
fn inspect(
    scorer: &dyn RerankScorer,
    query: &str,
    body: &str,
    context: &str,
    config: &CodeLocalScorerConfig,
) -> CoreResult<Option<(String, CodeTokenCoverage)>> {
    let text = format!("{context}{body}");
    let coverage = scorer.inspect_pair(query, &text, context.len())?;
    if !coverage.is_valid()
        || coverage.truncated
        || coverage.query_tokens > config.query_token_limit
    {
        return Err(invalid());
    }
    Ok((coverage.total_tokens <= config.pair_token_limit).then_some((text, coverage)))
}
impl Cursor {
    pub fn new(original: usize, candidate: &CodeCandidate) -> Self {
        Self {
            original,
            remaining: candidate.segment.ranges.clone(),
        }
    }
    pub fn has_remaining(&self) -> bool {
        !self.remaining.is_empty()
    }
    pub fn next(
        &mut self,
        candidate: &CodeCandidate,
        scorer: &dyn RerankScorer,
        query: &str,
        config: &CodeLocalScorerConfig,
    ) -> CoreResult<Option<View>> {
        if self.remaining.is_empty() {
            return Ok(None);
        }
        let mut facts = candidate.segment.context.clone();
        let mut context = prefix(&facts);
        // Keep whole facts only. Reserve room for at least one actual scalar.
        let first = engram_knowledge::code_segment_text(&candidate.parent, &[self.remaining[0]])?;
        let scalar_end = first.chars().next().map(char::len_utf8).unwrap_or(0);
        while inspect(scorer, query, &first[..scalar_end], &context, config)?.is_none() {
            if facts.pop().is_none() {
                return Err(invalid());
            }
            context = prefix(&facts);
        }
        let whole = engram_knowledge::code_segment_text(&candidate.parent, &self.remaining)?;
        let mut ranges = self.remaining.clone();
        let (text, coverage) = if whole.len() <= MAX_BODY_BYTES {
            inspect(scorer, query, &whole, &context, config)?
        } else {
            None
        }
        .map(Ok)
        .unwrap_or_else(|| {
            let max = boundary(&first, MAX_BODY_BYTES);
            let tokens = scorer.inspect_document(&first[..max])?;
            let identity = scorer.identity().ok_or_else(invalid)?;
            if tokens.tokenizer_id != identity.tokenizer_id
                || tokens.window != identity.window
                || tokens.special_tokens != 0
                || tokens
                    .offsets
                    .iter()
                    .any(|r| r.end < r.start || r.end > max as u64)
            {
                return Err(invalid());
            }
            let empty = scorer.inspect_pair(query, &context, context.len())?;
            if !empty.is_valid() || empty.truncated || empty.total_tokens >= config.pair_token_limit
            {
                return Err(invalid());
            }
            let room = (config.pair_token_limit - empty.total_tokens) as usize;
            let mut end = tokens
                .offsets
                .get(room.saturating_sub(1))
                .map(|r| r.end as usize)
                .unwrap_or(max);
            end = boundary(&first, end.min(max)).max(scalar_end);
            loop {
                if let Some(pair) = inspect(scorer, query, &first[..end], &context, config)? {
                    ranges = vec![CodeByteRange {
                        start: self.remaining[0].start,
                        end: self.remaining[0].start + end as u64,
                    }];
                    return Ok(pair);
                }
                if end <= scalar_end {
                    return Err(invalid());
                }
                end = boundary(&first, end / 2).max(scalar_end);
            }
        })?;
        if ranges == self.remaining {
            self.remaining.clear();
        } else {
            let end = ranges[0].end;
            if end == self.remaining[0].end {
                self.remaining.remove(0);
            } else {
                self.remaining[0].start = end;
            }
        }
        let bytes = serde_json::to_vec(&(CONTRACT, &candidate.segment.id, &ranges, &facts))
            .map_err(|_| invalid())?;
        let hash: String = Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        Ok(Some(View {
            id: format!("code-score-view-v1/{hash}"),
            original: self.original,
            ranges,
            context: facts,
            text,
            coverage,
        }))
    }
}
