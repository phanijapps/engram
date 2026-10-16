//! Pure source-range partitioning using externally supplied exact token offsets.
use engram_domain::{CodeByteRange, CodeEmbeddingSegment};
use engram_runtime::{CoreError, CoreResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenSegmentPlan {
    pub ranges: Vec<CodeByteRange>,
    pub code_tokens: u32,
    pub continuation: bool,
}

fn invalid(reason: &str) -> CoreError {
    CoreError::ValidationFailed {
        reason: reason.into(),
    }
}
fn slice(source: &str, r: CodeByteRange) -> CoreResult<&str> {
    let start =
        usize::try_from(r.start).map_err(|_| invalid("source range exceeds address space"))?;
    let end = usize::try_from(r.end).map_err(|_| invalid("source range exceeds address space"))?;
    source
        .get(start..end)
        .ok_or_else(|| invalid("source range is not a complete UTF-8 slice"))
}

/// Grammar-independent injection point: other parsers can supply the same
/// statement/block cut offsets without changing segmentation or persistence.
pub fn structural_boundaries(tree: &tree_sitter::Tree) -> Vec<u64> {
    let mut result = Vec::new();
    let mut cursor = tree.walk();
    loop {
        let node = cursor.node();
        let kind = node.kind();
        if node.is_named()
            && (kind.ends_with("_statement")
                || kind.ends_with("_declaration")
                || kind.ends_with("_definition")
                || kind.ends_with("_item")
                || matches!(kind, "block" | "statement_block" | "compound_statement"))
        {
            result.extend([node.start_byte() as u64, node.end_byte() as u64]);
        }
        if cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                result.sort_unstable();
                result.dedup();
                return result;
            }
        }
    }
}

/// Preserves every canonical source byte. Counts are recomputed after cuts;
/// token offsets alone never stand in for tokenizing the actual model input.
pub fn segment_source_ranges<F>(
    source: &str,
    ranges: &[CodeByteRange],
    separator: &str,
    boundaries: &[u64],
    budget: u32,
    tokenize: F,
) -> CoreResult<Vec<TokenSegmentPlan>>
where
    F: FnMut(&str) -> CoreResult<Vec<CodeByteRange>>,
{
    partition(
        source, ranges, separator, boundaries, budget, false, tokenize,
    )
}

/// Like `segment_source_ranges`, but permits zero-token shell gaps when range
/// bounds prevent attaching them to code. The consumer MUST supply grounded,
/// nonempty context before inference; these plans alone are not model inputs.
pub fn segment_source_ranges_with_context<F>(
    source: &str,
    ranges: &[CodeByteRange],
    separator: &str,
    boundaries: &[u64],
    budget: u32,
    tokenize: F,
) -> CoreResult<Vec<TokenSegmentPlan>>
where
    F: FnMut(&str) -> CoreResult<Vec<CodeByteRange>>,
{
    partition(
        source, ranges, separator, boundaries, budget, true, tokenize,
    )
}

fn partition<F>(
    source: &str,
    ranges: &[CodeByteRange],
    separator: &str,
    boundaries: &[u64],
    budget: u32,
    allow_context: bool,
    mut tokenize: F,
) -> CoreResult<Vec<TokenSegmentPlan>>
where
    F: FnMut(&str) -> CoreResult<Vec<CodeByteRange>>,
{
    if budget == 0 || ranges.is_empty() {
        return Err(invalid(
            "segmentation requires ranges and a positive token budget",
        ));
    }
    let mut previous_end = 0;
    let mut parts = Vec::new();
    for r in ranges {
        if r.start < previous_end {
            return Err(invalid("canonical ranges overlap or are unordered"));
        }
        parts.push(slice(source, *r)?);
        previous_end = r.end;
    }
    let joined = parts.join(separator);
    let total = tokenize(&joined)?.len();
    if total <= budget as usize && ranges.len() <= CodeEmbeddingSegment::MAX_RANGES {
        return Ok(vec![TokenSegmentPlan {
            ranges: ranges.to_vec(),
            code_tokens: total as u32,
            continuation: false,
        }]);
    }
    let mut result = Vec::new();
    for range in ranges {
        let mut start = range.start;
        let mut carrying_terminal = false;
        while start < range.end {
            let remaining = slice(
                source,
                CodeByteRange {
                    start,
                    end: range.end,
                },
            )?;
            let offsets = tokenize(remaining)?;
            if offsets.len() <= budget as usize {
                result.push(TokenSegmentPlan {
                    ranges: vec![CodeByteRange {
                        start,
                        end: range.end,
                    }],
                    code_tokens: offsets.len() as u32,
                    continuation: carrying_terminal,
                });
                break;
            }
            if offsets
                .iter()
                .any(|r| r.end < r.start || r.end > remaining.len() as u64)
            {
                return Err(invalid("token offsets exceed their input"));
            }
            let mut end = start + offsets[budget as usize].start;
            while end > start && !source.is_char_boundary(end as usize) {
                end -= 1;
            }
            // Prefix retokenization can change a boundary token. Back off at
            // actual token boundaries until the prefix itself fits.
            while end > start
                && tokenize(slice(source, CodeByteRange { start, end })?)?.len() > budget as usize
            {
                end = offsets
                    .iter()
                    .map(|r| start + r.start)
                    .filter(|v| *v < end && *v > start && source.is_char_boundary(*v as usize))
                    .max()
                    .unwrap_or(start);
            }
            if end <= start {
                return Err(invalid(
                    "token budget cannot cover one complete UTF-8 terminal scalar",
                ));
            }
            let structural = boundaries
                .iter()
                .copied()
                .filter(|v| *v > start && *v <= end && source.is_char_boundary(*v as usize))
                .max();
            let (end, terminal_cut) = if let Some(cut) = structural {
                if tokenize(slice(source, CodeByteRange { start, end: cut })?)?.len()
                    <= budget as usize
                {
                    (cut, false)
                } else {
                    (end, true)
                }
            } else {
                (end, true)
            };
            let count = tokenize(slice(source, CodeByteRange { start, end })?)?.len();
            result.push(TokenSegmentPlan {
                ranges: vec![CodeByteRange { start, end }],
                code_tokens: count as u32,
                continuation: carrying_terminal || terminal_cut,
            });
            carrying_terminal = terminal_cut;
            start = end;
        }
    }
    attach_blank_segments(
        source,
        ranges,
        separator,
        budget,
        allow_context,
        result,
        &mut tokenize,
    )
}

/// Blank shell gaps have source bytes but no independent model meaning. Keep
/// them in an adjacent segment, retokenizing the exact joined input after merge.
fn attach_blank_segments<F>(
    source: &str,
    canonical: &[CodeByteRange],
    separator: &str,
    budget: u32,
    allow_context: bool,
    plans: Vec<TokenSegmentPlan>,
    tokenize: &mut F,
) -> CoreResult<Vec<TokenSegmentPlan>>
where
    F: FnMut(&str) -> CoreResult<Vec<CodeByteRange>>,
{
    let text = |ranges: &[CodeByteRange]| -> CoreResult<String> {
        Ok(ranges
            .iter()
            .map(|r| slice(source, *r))
            .collect::<CoreResult<Vec<_>>>()?
            .join(separator))
    };
    let merge = |left: &TokenSegmentPlan,
                 right: &TokenSegmentPlan,
                 tokenize: &mut F|
     -> CoreResult<Option<TokenSegmentPlan>> {
        let mut ranges = left.ranges.clone();
        for range in &right.ranges {
            if let Some(last) = ranges.last_mut()
                && last.end == range.start
                && canonical
                    .iter()
                    .any(|outer| outer.start <= last.start && outer.end >= range.end)
            {
                last.end = range.end;
                continue;
            }
            ranges.push(*range);
        }
        let count = tokenize(&text(&ranges)?)?.len();
        Ok(
            (count <= budget as usize && ranges.len() <= CodeEmbeddingSegment::MAX_RANGES)
                .then_some(TokenSegmentPlan {
                    ranges,
                    code_tokens: count as u32,
                    continuation: left.continuation || right.continuation,
                }),
        )
    };
    let mut queue: std::collections::VecDeque<_> = plans.into();
    let mut result: Vec<TokenSegmentPlan> = Vec::new();
    while let Some(plan) = queue.pop_front() {
        if text(&plan.ranges)?.trim().is_empty() {
            if let Some(previous) = result.last_mut()
                && let Some(joined) = merge(previous, &plan, tokenize)?
            {
                *previous = joined;
                continue;
            }
            if let Some(next) = queue.front_mut()
                && let Some(joined) = merge(&plan, next, tokenize)?
            {
                *next = joined;
                continue;
            }
            if !allow_context {
                return Err(invalid(
                    "blank source gap cannot fit an adjacent model segment",
                ));
            }
        }
        result.push(plan);
    }
    Ok(result)
}
