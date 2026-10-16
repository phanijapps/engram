//! Source syntax for deterministic context. Policy and token budgets are callers'
//! responsibilities; observed import syntax never implies binding resolution.
mod category;
use engram_domain::{CodeByteRange, CodeSourceCategory};
use tree_sitter::{Node, Tree};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeUnitSyntax {
    pub category: CodeSourceCategory,
    pub signature_range: Option<CodeByteRange>,
}

pub fn analyze_code_unit(
    source: &str,
    path: &str,
    tree: &Tree,
    declaration: Option<CodeByteRange>,
) -> CodeUnitSyntax {
    let node = declaration.and_then(|r| {
        let start = usize::try_from(r.start).ok()?;
        let end = usize::try_from(r.end).ok()?;
        (start < end && source.get(start..end).is_some())
            .then(|| {
                tree.root_node()
                    .named_descendant_for_byte_range(start, end - 1)
            })
            .flatten()
    });
    let signature_range = node.filter(|n| !n.has_error()).and_then(|node| {
        let start = node.start_byte();
        let end = node
            .child_by_field_name("body")
            .map(|body| body.start_byte())
            .unwrap_or_else(|| {
                let raw = &source[start..node.end_byte()];
                start + raw.find('\n').unwrap_or(raw.len())
            });
        (end > start).then_some(CodeByteRange {
            start: start as u64,
            end: end as u64,
        })
    });
    CodeUnitSyntax {
        category: category::classify(source, path, tree, node),
        signature_range,
    }
}

pub fn source_import_ranges(tree: &Tree) -> Vec<CodeByteRange> {
    let mut found = Vec::new();
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        if matches!(
            node.kind(),
            "import_statement"
                | "import_declaration"
                | "import_from_statement"
                | "use_declaration"
                | "extern_crate_declaration"
        ) {
            found.push(CodeByteRange {
                start: node.start_byte() as u64,
                end: node.end_byte() as u64,
            });
        }
        let mut cursor = node.walk();
        stack.extend(node.named_children(&mut cursor));
    }
    found.sort_by_key(|r| (r.start, r.end));
    found.dedup();
    found
}

fn node_text<'a>(source: &'a str, node: Node<'_>) -> &'a str {
    source.get(node.start_byte()..node.end_byte()).unwrap_or("")
}
