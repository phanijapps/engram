//! Test-wrapper call chunking (fetch-content coverage for test files).
//!
//! Test files keep their declarations inside CALL ARGUMENTS —
//! `describe("suite", () => { test("case", () => { ... }) })` — and an arrow
//! function in a call argument has no `name` field and no
//! `variable_declarator` parent, so the declaration walk finds nothing and
//! the whole file collapses to the single `file` fallback chunk.
//!
//! When the walk reaches a `call_expression` whose callee is a known test
//! wrapper, the INNERMOST such call (one containing no further wrapper
//! calls) is emitted as one chunk spanning the call — anchor
//! `test '<case name>'` / `describe '<suite name>'`. The anchor keywords
//! are deliberately OUTSIDE `parse_symbol`'s closed set: wrapper chunks are
//! stored and searchable as content, but emit no symbol entities (test
//! bodies are coverage, not graph nodes — and search already demotes test
//! paths). Only innermost wrappers chunk, and the walk does not descend
//! under them, so wrapper chunks never overlap each other or duplicate
//! nested content.

use engram_domain::{KnowledgeChunkKind, SourceLocation};

use crate::parser::chunking::ChunkCandidate;

/// Which wrapper family a call belongs to — determines the anchor keyword.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WrapperKind {
    /// `describe(...)` — a suite.
    Describe,
    /// `test(...)` / `it(...)` / `bench(...)` — a case.
    Test,
}

/// Classifies a callee text as a test wrapper, if it is one.
///
/// Handles base names plus common modifier suffixes (`it.each`, `test.only`,
/// `describe.skip`, `it.concurrent`, `it.todo`).
pub fn wrapper_kind(callee: &str) -> Option<WrapperKind> {
    let mut name = callee.trim();
    if let Some((base, modifier)) = name.rsplit_once('.')
        && matches!(modifier, "each" | "only" | "skip" | "todo" | "concurrent")
    {
        name = base;
    }
    match name {
        "describe" => Some(WrapperKind::Describe),
        "test" | "it" | "bench" => Some(WrapperKind::Test),
        _ => None,
    }
}

/// True when any descendant of `node` is a wrapper call — used to chunk only
/// the INNERMOST wrapper (outer `describe`s just recurse).
pub fn contains_wrapper_call(node: &tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "call_expression"
            && child
                .child_by_field_name("function")
                .map(|f| wrapper_kind(f.utf8_text(source).unwrap_or("")).is_some())
                .unwrap_or(false)
        {
            return true;
        }
        if contains_wrapper_call(&child, source) {
            return true;
        }
    }
    false
}

/// Builds the chunk for an innermost wrapper `call_expression` node, or
/// `None` when the node is not a wrapper call (or wraps further wrapper
/// calls — those recurse instead).
pub fn wrapper_chunk(node: &tree_sitter::Node, source: &[u8]) -> Option<ChunkCandidate> {
    if node.kind() != "call_expression" {
        return None;
    }
    let callee = node.child_by_field_name("function")?;
    let kind = wrapper_kind(callee.utf8_text(source).unwrap_or(""))?;
    if contains_wrapper_call(node, source) {
        return None; // outer wrapper — its inner cases will chunk instead
    }
    let display = wrapper_display_name(node, source);
    let keyword = match kind {
        WrapperKind::Describe => "describe",
        WrapperKind::Test => "test",
    };
    let anchor = format!("{keyword} '{display}'");
    Some(ChunkCandidate {
        kind: KnowledgeChunkKind::CodeBlock,
        text: node.utf8_text(source).unwrap_or("").to_owned(),
        location: Some(SourceLocation {
            path: None,
            start_line: Some((node.start_position().row + 1) as u32),
            end_line: Some((node.end_position().row + 1) as u32),
            start_offset: None,
            end_offset: None,
            anchor: Some(anchor),
        }),
    })
}

/// First string-ish argument of the wrapper call, quote-stripped and
/// single-lined, for the anchor. Falls back to the call's line number when
/// the call has no literal name (computed titles, tagged templates).
fn wrapper_display_name(node: &tree_sitter::Node, source: &[u8]) -> String {
    let mut cursor = node.walk();
    for arg in node.named_children(&mut cursor) {
        let text = arg.utf8_text(source).unwrap_or("").trim();
        let stripped = text
            .strip_prefix('"')
            .or_else(|| text.strip_prefix('\''))
            .or_else(|| text.strip_prefix('`'));
        if let Some(inner) = stripped {
            let single_line = inner.lines().next().unwrap_or("").trim();
            let truncated: String = single_line.chars().take(80).collect();
            if !truncated.is_empty() {
                return truncated;
            }
        }
    }
    format!("line {}", node.start_position().row + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_base_names_and_modifiers() {
        assert_eq!(wrapper_kind("describe"), Some(WrapperKind::Describe));
        assert_eq!(wrapper_kind("test"), Some(WrapperKind::Test));
        assert_eq!(wrapper_kind("it"), Some(WrapperKind::Test));
        assert_eq!(wrapper_kind("bench"), Some(WrapperKind::Test));
        assert_eq!(wrapper_kind("it.each"), Some(WrapperKind::Test));
        assert_eq!(wrapper_kind("test.only"), Some(WrapperKind::Test));
        assert_eq!(wrapper_kind("describe.skip"), Some(WrapperKind::Describe));
        assert_eq!(wrapper_kind("run"), None);
        assert_eq!(wrapper_kind("described"), None);
        assert_eq!(wrapper_kind("map.each"), None);
        // A dotted name whose tail is not a modifier stays whole.
        assert_eq!(wrapper_kind("ns.describe"), None);
    }
}
