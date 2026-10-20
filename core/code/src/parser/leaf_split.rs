//! Oversized-leaf chunk splitting (fetch-content coverage guard).
//!
//! A declaration chunk whose text exceeds [`LEAF_SPLIT_MAX_BYTES`] — a large
//! TS `interface`, a giant Rust `enum`, a file-level fallback over a big
//! file — collapses an entire large file into ONE chunk. `fetch_content`
//! stitches what exists, so the file comes back as a single blob and the
//! vector lane embeds only that one chunk.
//!
//! This module is the generic safety net: after the declaration walk, every
//! oversized chunk is re-sliced into contiguous line windows taken from the
//! ORIGINAL source text, so window spans align exactly with file lines (the
//! declaration chunk's own text may carry a prepended doc comment that would
//! misalign naive text splitting). Windows keep the chunk's anchor, kind,
//! and path; only the span and text narrow. The invariant it enforces:
//! **no large file is ever represented by a single chunk**, regardless of
//! what grammar quirk produced the oversized leaf.

use engram_domain::SourceLocation;

use crate::parser::chunking::ChunkCandidate;

/// A chunk larger than this (bytes of text) is split into line windows.
/// 8 KiB comfortably covers typical functions/interfaces; whole-file leaves
/// (fallbacks, unmapped containers) blow past it and get windowed.
pub const LEAF_SPLIT_MAX_BYTES: usize = 8 * 1024;
/// Window line cap — keeps windows human-sized even for dense (wide-line)
/// sources that stay under the byte cap.
pub const LEAF_SPLIT_MAX_LINES: usize = 120;

/// Splits every oversized chunk into line windows sliced from `source`.
///
/// Chunks at or under [`LEAF_SPLIT_MAX_BYTES`], chunks without a usable line
/// span, and spans that resolve to no source lines pass through unchanged.
/// At least one line is always taken per window, so a single oversized LINE
/// (e.g. minified output) becomes its own window rather than looping.
pub fn split_oversized(chunks: Vec<ChunkCandidate>, source: &str) -> Vec<ChunkCandidate> {
    let source_lines: Vec<&str> = source.lines().collect();
    chunks
        .into_iter()
        .flat_map(|chunk| split_one(chunk, &source_lines))
        .collect()
}

/// Splits one chunk if oversized; otherwise returns it unchanged.
fn split_one(chunk: ChunkCandidate, source_lines: &[&str]) -> Vec<ChunkCandidate> {
    if chunk.text.len() <= LEAF_SPLIT_MAX_BYTES {
        return vec![chunk];
    }
    let Some(location) = chunk.location.as_ref() else {
        return vec![chunk];
    };
    let (Some(start), Some(end)) = (location.start_line, location.end_line) else {
        return vec![chunk];
    };
    // 1-based inclusive span → 0-based half-open slice, clamped to the file.
    let from = (start.saturating_sub(1) as usize).min(source_lines.len());
    let to = (end as usize).min(source_lines.len());
    if to <= from {
        return vec![chunk];
    }

    let mut windows: Vec<ChunkCandidate> = Vec::new();
    let mut window_start = from;
    while window_start < to {
        let mut bytes = 0usize;
        let mut lines = 0usize;
        let mut window_end = window_start;
        while window_end < to {
            let line_len = source_lines[window_end].len() + 1; // + newline
            if lines > 0
                && (bytes + line_len > LEAF_SPLIT_MAX_BYTES || lines >= LEAF_SPLIT_MAX_LINES)
            {
                break;
            }
            bytes += line_len;
            lines += 1;
            window_end += 1;
        }
        windows.push(ChunkCandidate {
            kind: chunk.kind.clone(),
            text: source_lines[window_start..window_end].join("\n"),
            location: Some(SourceLocation {
                path: location.path.clone(),
                start_line: Some((window_start + 1) as u32),
                end_line: Some(window_end as u32),
                start_offset: None,
                end_offset: None,
                anchor: location.anchor.clone(),
            }),
        });
        window_start = window_end;
    }
    if windows.is_empty() {
        vec![chunk]
    } else {
        windows
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engram_domain::KnowledgeChunkKind;

    fn chunk(text_len_hint: usize, anchor: &str, start: u32, end: u32) -> ChunkCandidate {
        ChunkCandidate {
            kind: KnowledgeChunkKind::CodeBlock,
            text: "x".repeat(text_len_hint),
            location: Some(SourceLocation {
                path: None,
                start_line: Some(start),
                end_line: Some(end),
                start_offset: None,
                end_offset: None,
                anchor: Some(anchor.to_owned()),
            }),
        }
    }

    fn line(n: usize) -> String {
        format!("line {n} with some padding to look like source\n")
    }

    #[test]
    fn small_chunks_pass_through_unchanged() {
        let source = (0..10).map(line).collect::<String>();
        let chunks = vec![chunk(100, "fn small", 1, 10)];
        let out = split_oversized(chunks, &source);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].text.len(), 100, "text untouched");
    }

    #[test]
    fn oversized_chunk_splits_into_span_aligned_windows() {
        // 600 lines × ~44 bytes ≈ 26 KB — well over the 8 KiB cap.
        let source = (0..600).map(line).collect::<String>();
        let chunks = vec![chunk(26_000, "file", 1, 600)];
        let out = split_oversized(chunks, &source);
        assert!(out.len() > 1, "must split, got {} windows", out.len());
        // Every window under the caps and carries the anchor.
        for w in &out {
            assert!(w.text.len() <= LEAF_SPLIT_MAX_BYTES + 64);
            assert_eq!(
                w.location.as_ref().and_then(|l| l.anchor.clone()),
                Some("file".to_owned())
            );
        }
        // Windows are contiguous, span-aligned, and reassemble the source.
        let mut expected_start = 1u32;
        for w in &out {
            let loc = w.location.as_ref().unwrap();
            assert_eq!(loc.start_line, Some(expected_start));
            expected_start = loc.end_line.unwrap() + 1;
        }
        assert_eq!(expected_start, 601);
        let stitched = out
            .iter()
            .map(|w| w.text.clone())
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(stitched, source.trim_end());
    }

    #[test]
    fn span_outside_source_or_missing_lines_clamps_or_passes_through() {
        let source = "only one line";
        // Span claims lines 1..500 but the file has 1: clamp to the real
        // lines — one window carrying the actual source content.
        let chunks = vec![chunk(20_000, "file", 1, 500)];
        let out = split_oversized(chunks, source);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].text, "only one line");
        assert_eq!(
            out[0].location.as_ref().unwrap().end_line,
            Some(1),
            "span clamped to the file's real line count"
        );

        // No location at all: keep the chunk untouched.
        let mut unspanned = chunk(20_000, "fn x", 1, 1);
        unspanned.location = None;
        let out = split_oversized(vec![unspanned], source);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].text.len(), 20_000);
    }

    #[test]
    fn single_oversized_line_becomes_one_window_no_loop() {
        let source = "a".repeat(50_000);
        let chunks = vec![chunk(50_000, "file", 1, 1)];
        let out = split_oversized(chunks, &source);
        assert_eq!(out.len(), 1, "one line → one window, no infinite loop");
        assert_eq!(out[0].text.len(), 50_000);
    }
}
