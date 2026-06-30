//! Deterministic source-code symbol chunking.
//!
//! This module owns a dependency-free declaration scanner for first-slice code
//! ingestion. It emits symbol-oriented chunk candidates with line spans and
//! anchors, but it is not an AST parser and does not infer relationships.

use engram_domain::{KnowledgeChunkKind, SourceLocation};
use engram_knowledge::{CoreError, CoreResult};

use crate::chunker::{ChunkCandidate, Chunker};

/// Line-oriented chunker for common source-code declarations.
///
/// The chunker recognizes declaration starts and groups each symbol until the
/// next recognized declaration. When no declaration is found, it returns one
/// file-level chunk so source text is never dropped silently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CodeSymbolChunker;

impl Chunker for CodeSymbolChunker {
    fn chunk(&self, text: &str) -> CoreResult<Vec<ChunkCandidate>> {
        if text.trim().is_empty() {
            return Err(CoreError::InvalidRequest {
                reason: "document text must not be empty".to_owned(),
            });
        }

        let lines = text.lines().collect::<Vec<_>>();
        let declarations = lines
            .iter()
            .enumerate()
            .filter_map(|(index, line)| declaration_anchor(line).map(|anchor| (index, anchor)))
            .collect::<Vec<_>>();

        if declarations.is_empty() {
            return Ok(vec![file_candidate(text, lines.len() as u32)]);
        }

        let mut chunks = Vec::with_capacity(declarations.len());
        for (position, (start_index, anchor)) in declarations.iter().enumerate() {
            let raw_end_index = declarations
                .get(position + 1)
                .map(|(next_index, _)| next_index.saturating_sub(1))
                .unwrap_or_else(|| lines.len().saturating_sub(1));
            let end_index = trim_trailing_blank_lines(&lines, *start_index, raw_end_index);
            let symbol_text = lines[*start_index..=end_index].join("\n");
            chunks.push(symbol_candidate(
                symbol_text,
                (*start_index + 1) as u32,
                (end_index + 1) as u32,
                anchor.clone(),
            ));
        }
        Ok(chunks)
    }
}

fn declaration_anchor(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") || trimmed.starts_with('#') && !trimmed.starts_with("#[") {
        return None;
    }

    rust_anchor(trimmed)
        .or_else(|| typescript_anchor(trimmed))
        .or_else(|| python_anchor(trimmed))
        .or_else(|| go_anchor(trimmed))
        .or_else(|| c_like_anchor(trimmed))
}

fn rust_anchor(line: &str) -> Option<String> {
    let line = strip_visibility(line, &["pub(crate)", "pub(super)", "pub"]);
    let line = strip_keyword(line, "async ").unwrap_or(line);
    let line = strip_keyword(line, "unsafe ").unwrap_or(line);
    if let Some(name) = name_after(line, "fn ") {
        return Some(format!("fn {name}"));
    }
    if let Some(name) = name_after(line, "struct ") {
        return Some(format!("struct {name}"));
    }
    if let Some(name) = name_after(line, "enum ") {
        return Some(format!("enum {name}"));
    }
    if let Some(name) = name_after(line, "trait ") {
        return Some(format!("trait {name}"));
    }
    if let Some(name) = name_after(line, "impl ") {
        return Some(format!("impl {name}"));
    }
    None
}

fn typescript_anchor(line: &str) -> Option<String> {
    let line = strip_keyword(line, "export default ").unwrap_or(line);
    let line = strip_keyword(line, "export ").unwrap_or(line);
    let line = strip_keyword(line, "async ").unwrap_or(line);
    if let Some(name) = name_after(line, "function ") {
        return Some(format!("function {name}"));
    }
    if let Some(name) = name_after(line, "class ") {
        return Some(format!("class {name}"));
    }
    if let Some(name) = name_after(line, "interface ") {
        return Some(format!("interface {name}"));
    }
    if let Some(name) = name_after(line, "type ") {
        return Some(format!("type {name}"));
    }
    if let Some(name) = const_function_name(line) {
        return Some(format!("function {name}"));
    }
    None
}

fn python_anchor(line: &str) -> Option<String> {
    if let Some(name) = name_after(line, "def ") {
        return Some(format!("def {name}"));
    }
    if let Some(name) = name_after(line, "async def ") {
        return Some(format!("def {name}"));
    }
    if let Some(name) = name_after(line, "class ") {
        return Some(format!("class {name}"));
    }
    None
}

fn go_anchor(line: &str) -> Option<String> {
    if let Some(name) = name_after(line, "func ") {
        return Some(format!("func {name}"));
    }
    if let Some(name) = name_after(line, "type ") {
        return Some(format!("type {name}"));
    }
    None
}

fn c_like_anchor(line: &str) -> Option<String> {
    let line = strip_visibility(line, &["public", "private", "protected"]);
    let line = strip_keyword(line, "static ").unwrap_or(line);
    if let Some(name) = name_after(line, "class ") {
        return Some(format!("class {name}"));
    }
    if let Some(name) = name_after(line, "interface ") {
        return Some(format!("interface {name}"));
    }
    if let Some(name) = name_after(line, "record ") {
        return Some(format!("record {name}"));
    }
    None
}

fn const_function_name(line: &str) -> Option<String> {
    for prefix in ["const ", "let ", "var "] {
        let Some(rest) = strip_keyword(line, prefix) else {
            continue;
        };
        let Some((name, initializer)) = rest.split_once('=') else {
            continue;
        };
        let initializer = initializer.trim_start();
        if initializer.starts_with('(') || initializer.starts_with("async ") {
            let name = identifier_prefix(name.trim())?;
            return Some(name.to_owned());
        }
    }
    None
}

fn strip_visibility<'a>(line: &'a str, keywords: &[&str]) -> &'a str {
    for keyword in keywords {
        let Some(rest) = strip_keyword(line, &format!("{keyword} ")) else {
            continue;
        };
        return rest;
    }
    line
}

fn strip_keyword<'a>(line: &'a str, keyword: &str) -> Option<&'a str> {
    line.strip_prefix(keyword).map(str::trim_start)
}

fn name_after<'a>(line: &'a str, prefix: &str) -> Option<&'a str> {
    let rest = strip_keyword(line, prefix)?;
    identifier_prefix(rest)
}

fn identifier_prefix(value: &str) -> Option<&str> {
    let value = value.trim_start();
    let end = value
        .find(|character: char| {
            !(character.is_ascii_alphanumeric()
                || character == '_'
                || character == '$'
                || character == '<'
                || character == '>')
        })
        .unwrap_or(value.len());
    if end == 0 {
        return None;
    }
    Some(&value[..end])
}

fn trim_trailing_blank_lines(lines: &[&str], start_index: usize, mut end_index: usize) -> usize {
    while end_index > start_index && lines[end_index].trim().is_empty() {
        end_index -= 1;
    }
    end_index
}

fn symbol_candidate(
    text: String,
    start_line: u32,
    end_line: u32,
    anchor: String,
) -> ChunkCandidate {
    ChunkCandidate {
        kind: KnowledgeChunkKind::CodeSymbol,
        text,
        location: Some(SourceLocation {
            path: None,
            start_line: Some(start_line),
            end_line: Some(end_line),
            start_offset: None,
            end_offset: None,
            anchor: Some(anchor),
        }),
    }
}

fn file_candidate(text: &str, end_line: u32) -> ChunkCandidate {
    ChunkCandidate {
        kind: KnowledgeChunkKind::File,
        text: text.to_owned(),
        location: Some(SourceLocation {
            path: None,
            start_line: Some(1),
            end_line: Some(end_line.max(1)),
            start_offset: None,
            end_offset: None,
            anchor: Some("file".to_owned()),
        }),
    }
}

#[cfg(test)]
mod tests {
    use engram_domain::KnowledgeChunkKind;

    use super::*;

    #[test]
    fn chunks_rust_declarations_with_anchors_and_lines() {
        let chunks = CodeSymbolChunker
            .chunk(
                r#"pub struct MemoryRecord {
    id: String,
}

fn remember() {
    println!("memory");
}
"#,
            )
            .expect("chunk rust");

        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].kind, KnowledgeChunkKind::CodeSymbol);
        assert_eq!(anchor(&chunks[0]), Some("struct MemoryRecord"));
        assert_eq!(line_range(&chunks[0]), (Some(1), Some(3)));
        assert_eq!(anchor(&chunks[1]), Some("fn remember"));
        assert_eq!(line_range(&chunks[1]), (Some(5), Some(7)));
    }

    #[test]
    fn chunks_typescript_declarations() {
        let chunks = CodeSymbolChunker
            .chunk(
                r#"export interface MemoryPort {
  write(): Promise<void>;
}

export const createClient = () => ({});
"#,
            )
            .expect("chunk typescript");

        assert_eq!(
            chunks.iter().map(anchor).collect::<Vec<_>>(),
            vec![Some("interface MemoryPort"), Some("function createClient")]
        );
    }

    #[test]
    fn chunks_python_declarations() {
        let chunks = CodeSymbolChunker
            .chunk(
                r#"class Memory:
    pass

async def recall():
    return []
"#,
            )
            .expect("chunk python");

        assert_eq!(
            chunks.iter().map(anchor).collect::<Vec<_>>(),
            vec![Some("class Memory"), Some("def recall")]
        );
    }

    #[test]
    fn falls_back_to_file_chunk_when_no_symbols_are_detected() {
        let chunks = CodeSymbolChunker
            .chunk("let value = memory.retrieve();\nvalue.await;\n")
            .expect("chunk fallback");

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].kind, KnowledgeChunkKind::File);
        assert_eq!(anchor(&chunks[0]), Some("file"));
    }

    fn anchor(candidate: &ChunkCandidate) -> Option<&str> {
        candidate
            .location
            .as_ref()
            .and_then(|location| location.anchor.as_deref())
    }

    fn line_range(candidate: &ChunkCandidate) -> (Option<u32>, Option<u32>) {
        let location = candidate.location.as_ref().expect("location");
        (location.start_line, location.end_line)
    }
}
