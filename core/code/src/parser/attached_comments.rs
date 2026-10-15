//! Exact, bounded source attachment using grammar comment nodes only.
use tree_sitter::Node;

/// Keep whole adjacent comments, never file notices separated by a blank line,
/// trailing comments on another statement, strings or preprocessor directives.
/// Declaration spans stay independent of this optional source prefix.
pub(super) fn start(node: &Node<'_>, source: &[u8]) -> Option<u64> {
    let mut cursor = *node;
    // TypeScript exports put the function beneath an export wrapper. Include
    // its actual bytes between the attached comment and declaration.
    if let Some(parent) = cursor.parent() {
        if parent.kind() == "export_statement" {
            cursor = parent;
        }
    }
    let mut first = None;
    for _ in 0..5 {
        let Some(previous) = cursor.prev_named_sibling() else {
            break;
        };
        if !(previous.kind() == "comment" || previous.kind().ends_with("_comment")) {
            break;
        }
        let gap = &source[previous.end_byte()..cursor.start_byte()];
        let line_start = source[..previous.start_byte()]
            .iter()
            .rposition(|&b| b == b'\n')
            .map_or(0, |i| i + 1);
        if !gap.iter().all(u8::is_ascii_whitespace)
            || gap.iter().filter(|&&b| b == b'\n').count() > 1
            || !source[line_start..previous.start_byte()]
                .iter()
                .all(u8::is_ascii_whitespace)
            || node.start_byte() - previous.start_byte() > 8192
        {
            break;
        }
        first = Some(previous.start_byte() as u64);
        cursor = previous;
    }
    first
}
