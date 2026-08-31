//! Deterministic text chunking for source-grounded documents.
//!
//! This module owns first-slice chunk boundaries and source locations for plain
//! text. It does not assign stable domain IDs, persist chunks, or perform
//! language-aware code parsing; those responsibilities live in ingestion and
//! future source-specific adapters.

use engram_domain::{KnowledgeChunkKind, SourceLocation};
use engram_knowledge::{CoreError, CoreResult};

// Parsing contract moved to the engram-code crate (ADR-0028); re-exported
// here so text/markdown chunkers and external consumers keep one import path.
pub use engram_code::parser::chunking::{ChunkCandidate, Chunker};

/// Configuration for deterministic plain-text line chunking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlainTextChunkerOptions {
    pub max_chars_per_chunk: usize,
}

impl Default for PlainTextChunkerOptions {
    fn default() -> Self {
        Self {
            max_chars_per_chunk: 1_200,
        }
    }
}

/// Line-aware chunker for text, Markdown, and first-slice code ingestion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PlainTextChunker {
    options: PlainTextChunkerOptions,
}

impl PlainTextChunker {
    /// Creates a plain-text chunker after validating chunk size.
    ///
    /// A zero-sized chunk would make progress impossible, so it is rejected at
    /// construction instead of during ingestion.
    pub fn new(options: PlainTextChunkerOptions) -> CoreResult<Self> {
        if options.max_chars_per_chunk == 0 {
            return Err(CoreError::InvalidRequest {
                reason: "max_chars_per_chunk must be greater than zero".to_owned(),
            });
        }
        Ok(Self { options })
    }
}

impl Chunker for PlainTextChunker {
    fn chunk(&self, text: &str) -> CoreResult<Vec<ChunkCandidate>> {
        if text.trim().is_empty() {
            return Err(CoreError::InvalidRequest {
                reason: "document text must not be empty".to_owned(),
            });
        }

        let mut chunks = Vec::new();
        let mut current = String::new();
        let mut start_line = 1_u32;
        let mut end_line = 1_u32;

        for (offset, line) in text.lines().enumerate() {
            let line_number = (offset + 1) as u32;
            let additional = line.len() + usize::from(!current.is_empty());
            if !current.is_empty() && current.len() + additional > self.options.max_chars_per_chunk
            {
                chunks.push(candidate(&current, start_line, end_line));
                current.clear();
                start_line = line_number;
            }

            if !current.is_empty() {
                current.push('\n');
            }
            current.push_str(line);
            end_line = line_number;
        }

        if !current.trim().is_empty() {
            chunks.push(candidate(&current, start_line, end_line));
        }
        Ok(chunks)
    }
}

fn candidate(text: &str, start_line: u32, end_line: u32) -> ChunkCandidate {
    ChunkCandidate {
        kind: KnowledgeChunkKind::DocumentSection,
        text: text.to_owned(),
        location: Some(SourceLocation {
            path: None,
            start_line: Some(start_line),
            end_line: Some(end_line),
            start_offset: None,
            end_offset: None,
            anchor: None,
        }),
    }
}
