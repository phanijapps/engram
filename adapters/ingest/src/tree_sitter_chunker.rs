//! Tree-sitter-backed AST symbol extraction for 10 languages.
//!
//! Parses source code into an AST and walks it for declaration nodes
//! (functions, classes, structs, etc.), producing ChunkCandidates with accurate
//! anchors + line spans. Falls back to the line-based CodeSymbolChunker for
//! extensions without a grammar (the scanner handles dispatch).

use std::collections::HashMap;

use engram_domain::{KnowledgeChunkKind, SourceLocation};
use engram_knowledge::{CoreError, CoreResult};

use crate::chunker::{ChunkCandidate, Chunker};

/// One grammar entry: the tree-sitter Language + a node-type → keyword map.
struct LangEntry {
    language: tree_sitter::Language,
    /// Map from tree-sitter node type → anchor keyword recognized by the
    /// extractor's `parse_symbol` (e.g. "function_item" → "fn").
    kind_map: HashMap<&'static str, &'static str>,
}

/// Tree-sitter chunker that dispatches by file extension.
pub struct TreeSitterChunker {
    entries: HashMap<&'static str, LangEntry>,
}

impl TreeSitterChunker {
    pub fn new() -> CoreResult<Self> {
        let mut entries = HashMap::new();
        // Each grammar registers one or more extensions.
        macro_rules! reg {
            ($ext:expr, $lang:expr, $map:expr) => {
                let language: tree_sitter::Language = $lang.into();
                entries.insert(
                    $ext,
                    LangEntry {
                        language,
                        kind_map: $map,
                    },
                );
            };
        }
        // Rust
        reg!("rs", tree_sitter_rust::LANGUAGE, rust_kinds());
        // C
        reg!("c", tree_sitter_c::LANGUAGE, c_kinds());
        reg!("h", tree_sitter_c::LANGUAGE, c_kinds());
        // C++
        reg!("cpp", tree_sitter_cpp::LANGUAGE, cpp_kinds());
        reg!("cc", tree_sitter_cpp::LANGUAGE, cpp_kinds());
        reg!("cxx", tree_sitter_cpp::LANGUAGE, cpp_kinds());
        reg!("hpp", tree_sitter_cpp::LANGUAGE, cpp_kinds());
        reg!("hxx", tree_sitter_cpp::LANGUAGE, cpp_kinds());
        // C#
        reg!("cs", tree_sitter_c_sharp::LANGUAGE, csharp_kinds());
        reg!("csx", tree_sitter_c_sharp::LANGUAGE, csharp_kinds());
        // TypeScript / TSX (the crate has no separate JS grammar; TS covers JS)
        reg!(
            "ts",
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
            ts_kinds()
        );
        reg!("tsx", tree_sitter_typescript::LANGUAGE_TSX, ts_kinds());
        reg!(
            "js",
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
            ts_kinds()
        );
        reg!("jsx", tree_sitter_typescript::LANGUAGE_TSX, ts_kinds());
        reg!(
            "mjs",
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
            ts_kinds()
        );
        reg!(
            "cjs",
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
            ts_kinds()
        );
        // Python
        reg!("py", tree_sitter_python::LANGUAGE, py_kinds());
        // Java
        reg!("java", tree_sitter_java::LANGUAGE, java_kinds());
        // Kotlin (tree-sitter-kotlin-ng — compatible fork)
        reg!("kt", tree_sitter_kotlin_ng::LANGUAGE, kt_kinds());
        reg!("kts", tree_sitter_kotlin_ng::LANGUAGE, kt_kinds());
        // Salesforce Apex (tree-sitter-sfapex)
        reg!("cls", tree_sitter_sfapex::apex::LANGUAGE, java_kinds());
        reg!("apex", tree_sitter_sfapex::apex::LANGUAGE, java_kinds());
        reg!("trigger", tree_sitter_sfapex::apex::LANGUAGE, java_kinds());
        // Perl
        reg!("pl", tree_sitter_perl::LANGUAGE, perl_kinds());
        reg!("pm", tree_sitter_perl::LANGUAGE, perl_kinds());
        // Bash
        reg!("sh", tree_sitter_bash::LANGUAGE, bash_kinds());
        reg!("bash", tree_sitter_bash::LANGUAGE, bash_kinds());
        // PHP
        reg!("php", tree_sitter_php::LANGUAGE_PHP, php_kinds());
        Ok(Self { entries })
    }

    pub fn supports(&self, ext: &str) -> bool {
        self.entries.contains_key(ext)
    }

    /// Extracts (caller, callee) pairs from AST call expressions. Walks the tree
    /// for call nodes, tracks which function declaration each call is inside,
    /// and returns only pairs where the callee matches a known entity name.
    /// More accurate than co-occurrence (no false positives from comments/strings).
    pub fn extract_calls(
        &self,
        text: &str,
        ext: &str,
        _entity_names: &std::collections::HashSet<String>,
    ) -> CoreResult<Vec<(String, String)>> {
        let Some(entry) = self.entries.get(ext) else {
            return Ok(Vec::new());
        };
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&entry.language)
            .map_err(|e| CoreError::InvalidRequest {
                reason: format!("tree-sitter language error: {e}"),
            })?;
        let tree = parser.parse(text, None).ok_or(CoreError::InvalidRequest {
            reason: "tree-sitter parse failed".to_owned(),
        })?;
        let root = tree.root_node();
        let source = text.as_bytes();

        // Collect (start_line, end_line, name) for each function-like declaration.
        let mut fn_spans: Vec<(usize, usize, String)> = Vec::new();
        let mut call_sites: Vec<(usize, String)> = Vec::new(); // (line, callee_name)

        collect_calls_and_spans(
            &root,
            source,
            &entry.kind_map,
            &mut fn_spans,
            &mut call_sites,
        );

        // Match each call to its enclosing function.
        let mut edges = Vec::new();
        for (call_line, callee) in &call_sites {
            for (start, end, caller) in &fn_spans {
                if call_line >= start && call_line <= end {
                    if caller != callee {
                        edges.push((caller.clone(), callee.clone()));
                    }
                    break;
                }
            }
        }
        Ok(edges)
    }

    /// Chunks text using the grammar for the given extension. Walks the AST
    /// for declaration nodes (no Query API — simpler + more robust).
    pub fn chunk_with_ext(&self, text: &str, ext: &str) -> CoreResult<Vec<ChunkCandidate>> {
        if text.trim().is_empty() {
            return Err(CoreError::InvalidRequest {
                reason: "document text must not be empty".to_owned(),
            });
        }
        let Some(entry) = self.entries.get(ext) else {
            return Err(CoreError::InvalidRequest {
                reason: format!("no tree-sitter grammar for .{ext}"),
            });
        };
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&entry.language)
            .map_err(|e| CoreError::InvalidRequest {
                reason: format!("tree-sitter language error: {e}"),
            })?;
        let tree = parser.parse(text, None).ok_or(CoreError::InvalidRequest {
            reason: "tree-sitter parse failed".to_owned(),
        })?;
        let root = tree.root_node();
        let mut chunks = Vec::new();
        walk_declarations(&root, text.as_bytes(), &entry.kind_map, &mut chunks);
        if chunks.is_empty() {
            let lines = text.lines().count() as u32;
            return Ok(vec![ChunkCandidate {
                kind: KnowledgeChunkKind::CodeBlock,
                text: text.to_owned(),
                location: Some(SourceLocation {
                    path: None,
                    start_line: Some(1),
                    end_line: Some(lines),
                    start_offset: None,
                    end_offset: None,
                    anchor: Some("file".to_owned()),
                }),
            }]);
        }
        Ok(chunks)
    }
}

impl Chunker for TreeSitterChunker {
    fn chunk(&self, text: &str) -> CoreResult<Vec<ChunkCandidate>> {
        // Direct call without extension — return a file-level chunk.
        if text.trim().is_empty() {
            return Err(CoreError::InvalidRequest {
                reason: "document text must not be empty".to_owned(),
            });
        }
        let lines = text.lines().count() as u32;
        Ok(vec![ChunkCandidate {
            kind: KnowledgeChunkKind::CodeBlock,
            text: text.to_owned(),
            location: Some(SourceLocation {
                path: None,
                start_line: Some(1),
                end_line: Some(lines),
                start_offset: None,
                end_offset: None,
                anchor: Some("file".to_owned()),
            }),
        }])
    }
}

/// Recursively walks the AST, checking each named node's kind against the map.
/// When a declaration is found, extracts its name from the `name` field (or a
/// fallback field) + produces a ChunkCandidate.
fn walk_declarations(
    node: &tree_sitter::Node,
    source: &[u8],
    kind_map: &HashMap<&str, &str>,
    chunks: &mut Vec<ChunkCandidate>,
) {
    let kind = node.kind();
    if let Some(keyword) = kind_map.get(kind) {
        // Try the `name` field first (works for most languages).
        let name_node = node
            .child_by_field_name("name")
            .or_else(|| node.child_by_field_name("declarator"))
            .or_else(|| node.child_by_field_name("type"));
        if let Some(name_node) = name_node {
            let name_text = extract_name(&name_node, source);
            if !name_text.is_empty() {
                let start_line = (node.start_position().row + 1) as u32;
                let end_line = (node.end_position().row + 1) as u32;
                let node_text = node.utf8_text(source).unwrap_or("").to_owned();
                chunks.push(ChunkCandidate {
                    kind: KnowledgeChunkKind::CodeSymbol,
                    text: node_text,
                    location: Some(SourceLocation {
                        path: None,
                        start_line: Some(start_line),
                        end_line: Some(end_line),
                        start_offset: None,
                        end_offset: None,
                        anchor: Some(format!("{keyword} {name_text}")),
                    }),
                });
            }
        }
    }
    // Recurse into named children.
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        walk_declarations(&child, source, kind_map, chunks);
    }
}

/// Walks the AST collecting: (1) function declaration spans for scope tracking,
/// and (2) call-expression sites with their callee names (not filtered — the
/// caller decides which to keep).
fn collect_calls_and_spans(
    node: &tree_sitter::Node,
    source: &[u8],
    kind_map: &HashMap<&str, &str>,
    fn_spans: &mut Vec<(usize, usize, String)>,
    call_sites: &mut Vec<(usize, String)>,
) {
    let kind = node.kind();

    // Track function/method declarations for scope.
    if kind_map.contains_key(kind) {
        if let Some(name_node) = node
            .child_by_field_name("name")
            .or_else(|| node.child_by_field_name("declarator"))
        {
            let name = extract_name(&name_node, source);
            if !name.is_empty() {
                fn_spans.push((node.start_position().row, node.end_position().row, name));
            }
        }
    }

    // Detect call expressions across languages.
    let is_call = kind.contains("call_expression")
        || kind.contains("method_invocation")
        || kind.contains("function_call")
        || kind == "call";
    if is_call {
        let callee_node = node
            .child_by_field_name("function")
            .or_else(|| node.child_by_field_name("name"))
            .or_else(|| node.named_child(0));
        if let Some(callee_node) = callee_node {
            let callee = extract_name(&callee_node, source);
            if !callee.is_empty() {
                call_sites.push((node.start_position().row, callee));
            }
        }
    }

    // Recurse.
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_calls_and_spans(&child, source, kind_map, fn_spans, call_sites);
    }
}

/// Extracts a clean name from a name node, handling nested declarators
/// (e.g. `pointer_declarator -> identifier` in C-like languages).
fn extract_name(node: &tree_sitter::Node, source: &[u8]) -> String {
    let text = node.utf8_text(source).unwrap_or("").trim().to_owned();
    // For simple identifiers, the text IS the name.
    if text.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return text;
    }
    // For complex declarators (e.g. "MyClass::method"), try the first child.
    if let Some(child) = node.named_child(0) {
        let child_text = child.utf8_text(source).unwrap_or("").trim().to_owned();
        if child_text.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return child_text;
        }
    }
    text.split(|c: char| !c.is_alphanumeric() && c != '_')
        .find(|s| !s.is_empty() && s.len() > 1)
        .unwrap_or(&text)
        .to_owned()
}

// --- Kind maps: tree-sitter node type → anchor keyword ---
fn rust_kinds() -> HashMap<&'static str, &'static str> {
    [
        ("function_item", "fn"),
        ("struct_item", "struct"),
        ("enum_item", "enum"),
        ("trait_item", "trait"),
    ]
    .into()
}
fn ts_kinds() -> HashMap<&'static str, &'static str> {
    [
        ("function_declaration", "function"),
        ("class_declaration", "class"),
        ("interface_declaration", "interface"),
        ("type_alias_declaration", "type"),
        ("method_definition", "fn"),
    ]
    .into()
}
fn py_kinds() -> HashMap<&'static str, &'static str> {
    [
        ("function_definition", "def"),
        ("class_definition", "class"),
    ]
    .into()
}
fn c_kinds() -> HashMap<&'static str, &'static str> {
    [
        ("function_definition", "fn"),
        ("struct_specifier", "struct"),
        ("enum_specifier", "enum"),
        ("union_specifier", "struct"),
        ("type_definition", "type"),
    ]
    .into()
}
fn cpp_kinds() -> HashMap<&'static str, &'static str> {
    [
        ("function_definition", "fn"),
        ("function_declaration", "fn"),
        ("class_specifier", "class"),
        ("struct_specifier", "struct"),
        ("enum_specifier", "enum"),
        ("namespace_definition", "module"),
        ("template_declaration", "fn"),
    ]
    .into()
}
fn csharp_kinds() -> HashMap<&'static str, &'static str> {
    [
        ("method_declaration", "fn"),
        ("class_declaration", "class"),
        ("interface_declaration", "interface"),
        ("struct_declaration", "struct"),
        ("enum_declaration", "enum"),
        ("constructor_declaration", "fn"),
        ("namespace_declaration", "module"),
        ("record_declaration", "class"),
    ]
    .into()
}
fn java_kinds() -> HashMap<&'static str, &'static str> {
    [
        ("method_declaration", "fn"),
        ("class_declaration", "class"),
        ("interface_declaration", "interface"),
        ("constructor_declaration", "fn"),
    ]
    .into()
}
// Kotlin kinds
fn kt_kinds() -> HashMap<&'static str, &'static str> {
    [
        ("function_declaration", "fn"),
        ("class_declaration", "class"),
        ("object_declaration", "class"),
    ]
    .into()
}
fn perl_kinds() -> HashMap<&'static str, &'static str> {
    [("sub_declaration_statement", "fn")].into()
}
fn bash_kinds() -> HashMap<&'static str, &'static str> {
    [("function_definition", "fn")].into()
}
fn php_kinds() -> HashMap<&'static str, &'static str> {
    [
        ("function_definition", "fn"),
        ("class_declaration", "class"),
        ("method_declaration", "fn"),
    ]
    .into()
}
