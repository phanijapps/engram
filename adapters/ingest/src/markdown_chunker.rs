//! Markdown-aware chunker.
//!
//! Splits Markdown into retrieval-friendly chunks by structure: ATX header
//! sections become [`KnowledgeChunkKind::DocumentSection`] chunks, fenced code
//! blocks become [`KnowledgeChunkKind::CodeBlock`] chunks, and loose prose
//! becomes [`KnowledgeChunkKind::Paragraph`] chunks. A leading YAML
//! front-matter block (`---\n…\n---`) is skipped (metadata, not content). Each
//! chunk carries a 1-based line-span [`SourceLocation`]; section chunks also
//! carry the heading text as `anchor`.

use engram_domain::{KnowledgeChunkKind, SourceLocation};
use engram_knowledge::{CoreError, CoreResult};

use crate::chunker::{ChunkCandidate, Chunker};

/// Markdown structure-aware chunker (no options yet; deterministic).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MarkdownChunker;

impl MarkdownChunker {
    /// Construct a chunker. (No options to validate yet; kept for parity with
    /// [`PlainTextChunker`](crate::chunker::PlainTextChunker).)
    pub fn new() -> CoreResult<Self> {
        Ok(Self)
    }
}

/// `true` if `line` is an ATX header (`#`..`######` followed by whitespace).
fn is_atx_header(line: &str) -> bool {
    let trimmed = line.trim_start();
    let hashes = trimmed.bytes().take_while(|&b| b == b'#').count();
    (1..=6).contains(&hashes)
        && trimmed
            .as_bytes()
            .get(hashes)
            .is_some_and(|b| *b == b' ' || *b == b'\t')
}

/// The fence marker (``` or ~~~) if `line` opens/closes a fenced block.
fn fence_marker(line: &str) -> Option<&'static str> {
    let trimmed = line.trim();
    if trimmed.starts_with("```") {
        Some("```")
    } else if trimmed.starts_with("~~~") {
        Some("~~~")
    } else {
        None
    }
}

fn candidate(
    kind: KnowledgeChunkKind,
    text: &str,
    start_line: u32,
    end_line: u32,
    anchor: Option<String>,
) -> ChunkCandidate {
    ChunkCandidate {
        kind,
        text: text.to_owned(),
        location: Some(SourceLocation {
            path: None,
            start_line: Some(start_line),
            end_line: Some(end_line),
            start_offset: None,
            end_offset: None,
            anchor,
        }),
    }
}

impl Chunker for MarkdownChunker {
    fn chunk(&self, text: &str) -> CoreResult<Vec<ChunkCandidate>> {
        if text.trim().is_empty() {
            return Err(CoreError::InvalidRequest {
                reason: "document text must not be empty".to_owned(),
            });
        }
        let lines: Vec<&str> = text.lines().collect();
        let n = lines.len();
        let mut chunks = Vec::new();
        let mut i = 0usize;

        // Skip a leading YAML front-matter block bounded by `---`. If no closing
        // `---` is found, the leading `---` is treated as content (a thematic
        // break), not front-matter — so an unterminated block can't swallow the
        // whole document.
        if n > 0 && lines[0].trim() == "---" {
            let mut j = 1;
            while j < n && lines[j].trim() != "---" {
                j += 1;
            }
            if j < n {
                i = j + 1;
            }
        }

        while i < n {
            let line = lines[i];
            if line.trim().is_empty() {
                i += 1;
                continue;
            }

            // Fenced code block: span fence-to-fence, content between.
            if fence_marker(line).is_some() {
                let start = i; // opening fence (0-based)
                let mut j = i + 1;
                while j < n && fence_marker(lines[j]).is_none() {
                    j += 1;
                }
                // j is the closing fence index, or n (unterminated).
                let body = lines[(start + 1)..j.min(n)].join("\n");
                if !body.trim().is_empty() {
                    let end_line = if j < n { (j + 1) as u32 } else { n as u32 };
                    chunks.push(candidate(
                        KnowledgeChunkKind::CodeBlock,
                        &body,
                        (start + 1) as u32,
                        end_line,
                        None,
                    ));
                }
                i = if j < n { j + 1 } else { n };
                continue;
            }

            // ATX header section: header + following prose until next header/fence.
            if is_atx_header(line) {
                let start = i;
                let anchor = line.trim().to_owned();
                i += 1;
                while i < n && !is_atx_header(lines[i]) && fence_marker(lines[i]).is_none() {
                    i += 1;
                }
                // Trim trailing blank lines.
                let mut end = i;
                while end > start + 1 && lines[end - 1].trim().is_empty() {
                    end -= 1;
                }
                let body = lines[start..end].join("\n");
                if !body.trim().is_empty() {
                    chunks.push(candidate(
                        KnowledgeChunkKind::DocumentSection,
                        &body,
                        (start + 1) as u32,
                        end as u32,
                        Some(anchor),
                    ));
                }
                continue;
            }

            // Loose prose paragraph.
            let start = i;
            i += 1;
            while i < n
                && !lines[i].trim().is_empty()
                && !is_atx_header(lines[i])
                && fence_marker(lines[i]).is_none()
            {
                i += 1;
            }
            let body = lines[start..i].join("\n");
            if !body.trim().is_empty() {
                chunks.push(candidate(
                    KnowledgeChunkKind::Paragraph,
                    &body,
                    (start + 1) as u32,
                    i as u32,
                    None,
                ));
            }
        }

        Ok(chunks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty() {
        let err = MarkdownChunker::new().unwrap().chunk("").unwrap_err();
        assert!(matches!(err, CoreError::InvalidRequest { .. }));
    }

    #[test]
    fn splits_by_atx_headers_with_line_spans() {
        let md = "# Title\nbody one\n## Sub\nbody two\n";
        let chunks = MarkdownChunker::new().unwrap().chunk(md).unwrap();
        let sections: Vec<_> = chunks
            .iter()
            .filter(|c| c.kind == KnowledgeChunkKind::DocumentSection)
            .collect();
        assert_eq!(sections.len(), 2, "{sections:?}");
        assert_eq!(sections[0].location.as_ref().unwrap().start_line, Some(1));
        assert_eq!(sections[0].location.as_ref().unwrap().end_line, Some(2));
        assert_eq!(
            sections[0].location.as_ref().unwrap().anchor.as_deref(),
            Some("# Title")
        );
        assert_eq!(sections[1].location.as_ref().unwrap().start_line, Some(3));
        assert_eq!(sections[1].location.as_ref().unwrap().end_line, Some(4));
    }

    #[test]
    fn fenced_code_becomes_codeblock() {
        let md = "intro\n```rust\nfn main() {}\n```\nafter\n";
        let chunks = MarkdownChunker::new().unwrap().chunk(md).unwrap();
        let code: Vec<_> = chunks
            .iter()
            .filter(|c| c.kind == KnowledgeChunkKind::CodeBlock)
            .collect();
        assert_eq!(code.len(), 1, "{chunks:?}");
        assert!(code[0].text.contains("fn main()"));
        assert_eq!(code[0].location.as_ref().unwrap().start_line, Some(2));
        assert_eq!(code[0].location.as_ref().unwrap().end_line, Some(4));
    }

    #[test]
    fn front_matter_is_skipped() {
        let md = "---\ntitle: Doc\n---\n# Real\ncontent\n";
        let chunks = MarkdownChunker::new().unwrap().chunk(md).unwrap();
        assert!(
            chunks.iter().all(|c| !c.text.contains("title: Doc")),
            "front-matter must not appear in any chunk"
        );
        let section = chunks
            .iter()
            .find(|c| c.kind == KnowledgeChunkKind::DocumentSection)
            .expect("a section chunk");
        assert_eq!(section.location.as_ref().unwrap().start_line, Some(4));
    }

    #[test]
    fn unterminated_front_matter_is_treated_as_content() {
        // A leading `---` with no closing `---` is a thematic break, not
        // front-matter — the document must still be chunked.
        let md = "---\nthis is not front matter\n";
        let chunks = MarkdownChunker::new().unwrap().chunk(md).unwrap();
        assert!(
            !chunks.is_empty(),
            "unterminated --- must not swallow the doc"
        );
        assert!(chunks.iter().any(|c| c.text.contains("not front matter")));
    }

    #[test]
    fn loose_prose_becomes_paragraph() {
        let md = "just a paragraph\nwith two lines\n";
        let chunks = MarkdownChunker::new().unwrap().chunk(md).unwrap();
        let paras: Vec<_> = chunks
            .iter()
            .filter(|c| c.kind == KnowledgeChunkKind::Paragraph)
            .collect();
        assert_eq!(paras.len(), 1);
        assert!(paras[0].text.contains("just a paragraph"));
        assert_eq!(paras[0].location.as_ref().unwrap().start_line, Some(1));
    }
}
