//! Tree-sitter-backed AST symbol extraction for 10 languages.
//!
//! Parses source code into an AST and walks it for declaration nodes
//! (functions, classes, structs, etc.), producing ChunkCandidates with accurate
//! anchors + line spans. Falls back to the line-based CodeSymbolChunker for
//! extensions without a grammar (the scanner handles dispatch).

use std::collections::HashMap;

use engram_domain::{KnowledgeChunkKind, SourceLocation};
use engram_runtime::{CoreError, CoreResult};

use crate::edges::{StructuralEdges, contains_pairs};
use crate::parser::chunking::{ChunkCandidate, Chunker};

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
        // Go
        reg!("go", tree_sitter_go::LANGUAGE, go_kinds());
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
        // Ruby
        reg!("rb", tree_sitter_ruby::LANGUAGE, ruby_kinds());
        // Scala
        reg!("scala", tree_sitter_scala::LANGUAGE, scala_kinds());
        reg!("sc", tree_sitter_scala::LANGUAGE, scala_kinds());
        // Swift
        reg!("swift", tree_sitter_swift::LANGUAGE, swift_kinds());
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

    /// Parses `text` once for `ext` — the shared-tree entry point
    /// (code-graph-quality [parse-multiplicity]): chunking, call extraction,
    /// and structural extraction each used to re-parse the same text (3×
    /// redundant parses per file in the main scan phase, 4× with the pre-pass
    /// name collection). Parse here, pass `&Tree` to the `*_tree` variants.
    pub fn parse(&self, text: &str, ext: &str) -> CoreResult<tree_sitter::Tree> {
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
        parser.parse(text, None).ok_or(CoreError::InvalidRequest {
            reason: "tree-sitter parse failed".to_owned(),
        })
    }

    /// Extracts typed structural facts (imports / contains / extends /
    /// implements) from the AST (RFC-0020 Phase 2). Pure name-level output —
    /// the extractor attaches entity refs. Languages without a mapped
    /// import/inheritance node kind emit nothing for that fact.
    ///
    /// One-shot convenience (parses internally). Scan paths should use
    /// [`Self::parse`] + [`Self::extract_structural_tree`].
    pub fn extract_structural(&self, text: &str, ext: &str) -> CoreResult<StructuralEdges> {
        let Some(entry) = self.entries.get(ext) else {
            return Ok(StructuralEdges::default());
        };
        let tree = self.parse(text, ext)?;
        let mut edges = StructuralEdges::default();
        walk_structural(
            &tree.root_node(),
            text.as_bytes(),
            &entry.kind_map,
            ext,
            &[],
            &mut edges,
        );
        Ok(edges)
    }

    /// [`Self::extract_structural`] over a pre-parsed tree — see [`Self::parse`].
    pub fn extract_structural_tree(
        &self,
        tree: &tree_sitter::Tree,
        text: &str,
        ext: &str,
    ) -> CoreResult<StructuralEdges> {
        let Some(entry) = self.entries.get(ext) else {
            return Ok(StructuralEdges::default());
        };
        let mut edges = StructuralEdges::default();
        walk_structural(
            &tree.root_node(),
            text.as_bytes(),
            &entry.kind_map,
            ext,
            &[],
            &mut edges,
        );
        Ok(edges)
    }

    /// Extracts (caller, callee) pairs from AST call expressions. Walks the tree
    /// for call nodes, tracks which function declaration each call is inside,
    /// and returns only pairs where the callee matches a known entity name.
    /// More accurate than co-occurrence (no false positives from comments/strings).
    /// Extracts (caller, callee) pairs from AST call expressions. Walks the tree
    /// for call nodes, tracks which function declaration each call is inside,
    /// and returns only pairs where the callee matches a known entity name.
    /// More accurate than co-occurrence (no false positives from comments/strings).
    ///
    /// One-shot convenience (parses internally). Scan paths should use
    /// [`Self::parse`] + [`Self::extract_calls_tree`].
    pub fn extract_calls(
        &self,
        text: &str,
        ext: &str,
        entity_names: &std::collections::HashSet<String>,
    ) -> CoreResult<Vec<(String, String)>> {
        if !self.supports(ext) {
            return Ok(Vec::new());
        }
        let tree = self.parse(text, ext)?;
        self.extract_calls_tree(&tree, text, ext, entity_names)
    }

    /// [`Self::extract_calls`] over a pre-parsed tree — see [`Self::parse`].
    pub fn extract_calls_tree(
        &self,
        tree: &tree_sitter::Tree,
        text: &str,
        ext: &str,
        entity_names: &std::collections::HashSet<String>,
    ) -> CoreResult<Vec<(String, String)>> {
        let Some(entry) = self.entries.get(ext) else {
            return Ok(Vec::new());
        };
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

        // Match each call to its enclosing function. Known callees emit
        // immediately; unknown-but-plausible callees (not bare generics —
        // the noise filter) emit as name-only references so Phase-2
        // resolution can settle them cross-file or ledger them. Bare
        // generics (`new`, `clone`, `len`, …) are dropped: they collide
        // across every crate and never resolve to a stable identity.
        let mut edges = Vec::new();
        for (call_line, callee) in &call_sites {
            // Dotted references match known entities by their bare tail.
            let tail = callee.rsplit('.').next().unwrap_or(callee);
            if !entity_names.contains(tail) && crate::noise::is_noise_symbol(tail) {
                continue;
            }
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
        if !self.supports(ext) {
            return Err(CoreError::InvalidRequest {
                reason: format!("no tree-sitter grammar for .{ext}"),
            });
        }
        let tree = self.parse(text, ext)?;
        self.chunk_with_tree(&tree, text, ext)
    }

    /// [`Self::chunk_with_ext`] over a pre-parsed tree — see [`Self::parse`].
    pub fn chunk_with_tree(
        &self,
        tree: &tree_sitter::Tree,
        text: &str,
        ext: &str,
    ) -> CoreResult<Vec<ChunkCandidate>> {
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
///
/// Container declarations — a class/struct/trait/interface that itself contains
/// named declaration children (methods, nested functions) — emit an
/// **anchor-only** candidate: empty `text` with the anchor/location preserved.
/// This keeps the declaration visible to the graph extractor (which derives the
/// `KnowledgeEntity` from the chunk's anchor, not its text) so the class entity
/// still exists for graph/codegraph queries, while eliminating the bloated
/// whole-body chunk from the vector index. The child declarations are emitted
/// by the recursion below, so every method/function is still individually
/// indexed + embedded — no information loss. Leaf declarations (functions,
/// methods with no declaration children) emit their full text as before.
fn walk_declarations(
    node: &tree_sitter::Node,
    source: &[u8],
    kind_map: &HashMap<&str, &str>,
    chunks: &mut Vec<ChunkCandidate>,
) {
    walk_declarations_with_receivers(node, source, kind_map, &[], chunks);
}

/// Receiver-aware walk (RFC-0020 Phase 2): a declaration nested inside other
/// declarations (a method inside `impl Foo`, a nested `fn`) carries its
/// enclosing chain in the anchor — `fn Foo::bar` — so the extractor can form
/// receiver-qualified entity names. Top-level declarations stay bare.
fn walk_declarations_with_receivers(
    node: &tree_sitter::Node,
    source: &[u8],
    kind_map: &HashMap<&str, &str>,
    receivers: &[String],
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
                // A container (e.g. a class with method children) emits empty
                // text — the entity is created from the anchor, but the
                // multi-thousand-line body is never embedded. Empty-text chunks
                // are skipped by the embedder (scan_repo filters them) and by
                // every retrieval lane, so they are graph-only anchors.
                // T9: code-symbol chunks carry their doc comment +
                // signature for the lexical lane. Containers (whose whole
                // body would bloat embeddings) get a SYNTHESIZED small text
                // — docstring + signature line — instead of empty text, so
                // they are lexically searchable; leaves prepend the doc
                // comment to their full body.
                let doc = leading_doc_comment(node, source);
                let signature = signature_line(node, source);
                let text = if has_declaration_descendant(node, kind_map) {
                    match (&doc, signature.is_empty()) {
                        (Some(d), _) => format!("{d}\n{signature}"),
                        (None, true) => String::new(),
                        (None, false) => signature,
                    }
                } else {
                    match &doc {
                        Some(d) => format!("{d}\n{}", node.utf8_text(source).unwrap_or("")),
                        None => node.utf8_text(source).unwrap_or("").to_owned(),
                    }
                };
                // Receiver-qualified anchor: `fn Foo::bar` inside `impl Foo`,
                // bare `fn parse` at the top level.
                let qualified = if receivers.is_empty() {
                    name_text.clone()
                } else {
                    format!("{}::{}", receivers.join("::"), name_text)
                };
                chunks.push(ChunkCandidate {
                    kind: KnowledgeChunkKind::CodeSymbol,
                    text,
                    location: Some(SourceLocation {
                        path: None,
                        start_line: Some(start_line),
                        end_line: Some(end_line),
                        start_offset: None,
                        end_offset: None,
                        anchor: Some(format!("{keyword} {qualified}")),
                    }),
                });
                // Children of this declaration descend with it on the
                // receiver chain (its bare name, not the qualified form).
                let mut child_receivers = receivers.to_vec();
                child_receivers.push(name_text);
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    walk_declarations_with_receivers(
                        &child,
                        source,
                        kind_map,
                        &child_receivers,
                        chunks,
                    );
                }
                return;
            }
        }
    }
    // Not a recognized declaration — recurse with the unchanged chain.
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        walk_declarations_with_receivers(&child, source, kind_map, receivers, chunks);
    }
}

/// Contiguous comment lines immediately above a declaration (up to 5):
/// `//`, `///`, `#`, or block-comment `*`/`*/` tails. Returns None when the
/// declaration has no doc comment.
fn leading_doc_comment(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    let start_row = node.start_position().row;
    if start_row == 0 {
        return None;
    }
    let text = std::str::from_utf8(source).ok()?;
    let lines: Vec<&str> = text.lines().collect();
    let mut collected: Vec<&str> = Vec::new();
    let mut row = start_row as isize - 1;
    while row >= 0 && collected.len() < 5 {
        let line = lines.get(row as usize)?.trim();
        if line.is_empty() {
            break;
        }
        // Preprocessor directives and shebangs are not documentation.
        if line.starts_with("#!")
            || line.starts_with("#include")
            || line.starts_with("#import")
            || line.starts_with("#pragma")
            || line.starts_with("#define")
            || line.starts_with("#ifdef")
            || line.starts_with("#ifndef")
            || line.starts_with("#endif")
        {
            break;
        }
        let is_doc = line.starts_with("///")
            || line.starts_with("//")
            || line.starts_with('#')
            || line.starts_with('*')
            || line.starts_with("/*");
        if !is_doc {
            break;
        }
        let stripped = line
            .trim_start_matches("///")
            .trim_start_matches("//")
            .trim_start_matches("/*")
            .trim_start_matches('*')
            .trim_start_matches('#')
            .trim();
        if !stripped.is_empty() {
            collected.push(stripped);
        }
        row -= 1;
    }
    if collected.is_empty() {
        None
    } else {
        collected.reverse();
        Some(collected.join("\n"))
    }
}

/// First source line of the declaration (its signature), trimmed.
fn signature_line(node: &tree_sitter::Node, source: &[u8]) -> String {
    let text = std::str::from_utf8(source).ok().unwrap_or("");
    let lines: Vec<&str> = text.lines().collect();
    lines
        .get(node.start_position().row)
        .map(|l: &&str| l.trim().to_owned())
        .unwrap_or_default()
}

/// True when `node`'s subtree contains at least one named descendant whose
/// tree-sitter kind is a recognized declaration (present in `kind_map`). Used
/// to detect container declarations (class/struct/trait/interface with method
/// or function children) whose whole-body chunk would duplicate every child's
/// content.
///
/// Descendants (not just direct named children) are checked because many
/// tree-sitter grammars wrap a class's members in a body node
/// (`class_declaration` → `class_body` → `method_declaration`), so the
/// declaration children are not direct children of the class node. Since
/// `walk_declarations` recurses into the entire subtree, any declaration
/// descendant is already emitted as its own chunk — the parent's whole-body
/// chunk is therefore redundant bloat and is suppressed.
fn has_declaration_descendant(node: &tree_sitter::Node, kind_map: &HashMap<&str, &str>) -> bool {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if kind_map.contains_key(child.kind()) || has_declaration_descendant(&child, kind_map) {
            return true;
        }
    }
    false
}

/// Walks the AST collecting: (1) function declaration spans for scope tracking,
/// and (2) call-expression sites with their callee names (not filtered — the
/// caller decides which to keep).

/// Reference name for a call site: the callee node's full dotted text with
/// `self`/`Self` prefixes stripped (`self.store.save` → `store.save`);
/// non-dotted calls return the bare callee name.
fn dotted_reference(full_text: &str, bare_callee: &str) -> String {
    if !full_text.contains('.') {
        return bare_callee.to_owned();
    }
    let segments: Vec<&str> = full_text.split('.').filter(|s| !s.is_empty()).collect();
    if segments.len() < 2 {
        return bare_callee.to_owned();
    }
    // Only keep a hint when the text really is a receiver chain ending in
    // the callee identifier (guards against calls inside strings/paths).
    let name = segments[segments.len() - 1];
    if name != bare_callee {
        return bare_callee.to_owned();
    }
    // Closest non-self segment before the callee carries the hint.
    let mut hint_idx = segments.len() - 2;
    while hint_idx > 0 && matches!(segments[hint_idx], "self" | "Self") {
        hint_idx -= 1;
    }
    if matches!(segments[hint_idx], "self" | "Self") {
        return bare_callee.to_owned();
    }
    format!("{}.{}", segments[hint_idx], name)
}

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
                // Dotted callees keep their receiver hint as-written
                // (`self.store.save` → `store.save`): resolution (Phase 2)
                // tries `Store::save` through the hint before the bare
                // ladder. `self`/`Self` receivers carry no hint.
                let full = callee_node.utf8_text(source).unwrap_or("");
                let reference = dotted_reference(full, &callee);
                call_sites.push((node.start_position().row, reference));
            }
        }
    }

    // scan-reliability AC5: JSX element usage is a reference. Components are
    // used declaratively (`<Button …>`), never via a call_expression, so
    // without this every React component has zero incoming `calls` edges and
    // dead-code analytics flag them all as dead. Only PascalCase element
    // names count — lowercase names are DOM intrinsics (`<div>`, `<span>`),
    // not entities, and would otherwise flood the unresolved-refs ledger.
    if kind == "jsx_opening_element" || kind == "jsx_self_closing_element" {
        if let Some(name_node) = node.child_by_field_name("name") {
            let name = extract_name(&name_node, source);
            if name.chars().next().is_some_and(char::is_uppercase) {
                call_sites.push((node.start_position().row, name));
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
/// (e.g. `pointer_declarator -> identifier` in C-like languages) AND
/// qualified/method-call expressions (`receiver.method`, `Type::method`).
fn extract_name(node: &tree_sitter::Node, source: &[u8]) -> String {
    let text = node.utf8_text(source).unwrap_or("").trim().to_owned();
    // For simple identifiers, the text IS the name.
    if text.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return text;
    }
    // For qualified/method-call expressions (receiver.method, Type::method,
    // self.store.save), return the LAST segment — the method/function name, not
    // the receiver. This is language-agnostic: Rust field_expression / scoped
    // identifier, Java/TS member_expression, Python attribute, etc. all use
    // dots or colons to separate the receiver from the method.
    if text.contains('.') || text.contains("::") {
        return text
            .split(['.', ':'])
            .rfind(|s| !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_'))
            .unwrap_or(&text)
            .to_owned();
    }
    // For complex declarators (e.g. "*foo"), try the first named child.
    if let Some(child) = node.named_child(0) {
        let child_text = child.utf8_text(source).unwrap_or("").trim().to_owned();
        if child_text.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return child_text;
        }
    }
    text.split(|c: char| !c.is_alphanumeric() && c != '_')
        .rfind(|s| !s.is_empty() && s.len() > 1)
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
        // RFC-0020 Phase 2: impl blocks are declarations too — the impl target
        // becomes the receiver chain for its methods (`impl Foo { fn bar }` →
        // anchor `fn Foo::bar`). `impl Trait for Type` names the concrete type
        // (the `type` field), so trait-impl methods are `Type::method`.
        ("impl_item", "impl"),
        // `mod` blocks likewise: `mod a { fn f }` → anchor `fn a::f`, so
        // same-named fns in different mods stay distinct entities.
        ("mod_item", "mod"),
    ]
    .into()
}
fn go_kinds() -> HashMap<&'static str, &'static str> {
    [
        ("function_declaration", "fn"),
        ("method_declaration", "fn"),
        ("type_declaration", "type"),
    ]
    .into()
}
fn ruby_kinds() -> HashMap<&'static str, &'static str> {
    [
        ("method", "def"),
        ("singleton_method", "def"),
        ("class", "class"),
    ]
    .into()
}
fn scala_kinds() -> HashMap<&'static str, &'static str> {
    [
        ("function_definition", "def"),
        ("class_definition", "class"),
        ("trait_definition", "trait"),
        ("object_definition", "class"),
    ]
    .into()
}
fn swift_kinds() -> HashMap<&'static str, &'static str> {
    [
        ("function_declaration", "fn"),
        ("class_declaration", "class"),
        ("struct_declaration", "struct"),
        ("protocol_declaration", "trait"),
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
        // React functional components and Redux action creators use arrow functions
        // assigned to const/let — e.g. `const QuickEdit = (props) => { ... }`.
        // These are lexical_declaration → variable_declarator → arrow_function in
        // the tree-sitter AST and are invisible without this entry.
        ("arrow_function", "function"),
        ("generator_function_declaration", "function"),
        ("enum_declaration", "enum"),
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

/// Receiver-aware structural walk: collects containment pairs from
/// declaration nesting, inheritance clauses, and import statements.
fn walk_structural(
    node: &tree_sitter::Node,
    source: &[u8],
    kind_map: &HashMap<&str, &str>,
    ext: &str,
    receivers: &[String],
    edges: &mut StructuralEdges,
) {
    let kind = node.kind();
    // Imports: language-specific node kinds; the path text is normalized by
    // `import_path`.
    let paths = import_paths(node, source, ext);
    if !paths.is_empty() {
        edges.imports.extend(paths);
    } else if kind_map.contains_key(kind) {
        let name_node = node
            .child_by_field_name("name")
            .or_else(|| node.child_by_field_name("declarator"))
            .or_else(|| node.child_by_field_name("type"));
        if let Some(name_node) = name_node {
            let name_text = extract_name(&name_node, source);
            if !name_text.is_empty() {
                let qualified = if receivers.is_empty() {
                    name_text.clone()
                } else {
                    format!("{}::{}", receivers.join("::"), name_text)
                };
                edges.contains.extend(contains_pairs(receivers, &name_text));
                extract_inheritance(node, source, ext, &qualified, edges);
                let mut child_receivers = receivers.to_vec();
                child_receivers.push(name_text);
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    walk_structural(&child, source, kind_map, ext, &child_receivers, edges);
                }
                return;
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        walk_structural(&child, source, kind_map, ext, receivers, edges);
    }
}

/// Import paths for a node, when the node is an import statement for the
/// language — a Vec so `import a, b` records every module. Empty for
/// non-import nodes. Kind is matched before any text is sliced (the walk
/// calls this for every AST node).
fn import_paths(node: &tree_sitter::Node, source: &[u8], ext: &str) -> Vec<String> {
    match (ext, node.kind()) {
        ("rs", "use_declaration") => {
            let text = node.utf8_text(source).unwrap_or("").trim();
            // `pub[crate] use …` carries an OPTIONAL visibility modifier in
            // the node text — strip it when present, then the `use` keyword.
            let body = text
                .strip_prefix("pub ")
                .or_else(|| text.strip_prefix("pub(crate) "))
                .or_else(|| text.strip_prefix("pub(super) "))
                .unwrap_or(text);
            let Some(body) = body.strip_prefix("use ").map(|b| b.trim_end_matches(';')) else {
                return Vec::new();
            };
            // Grouped uses (`std::io::{Read, Write}`) attribute the group's
            // parent path; plain uses keep the full path.
            let path = body.split('{').next().unwrap_or(body).trim_matches(':');
            vec![path.to_owned()]
        }
        ("py", "import_statement") => {
            let text = node.utf8_text(source).unwrap_or("").trim();
            let Some(body) = text.strip_prefix("import ") else {
                return Vec::new();
            };
            // `import a, b as c` → one module per comma item, alias stripped.
            body.split(',')
                .filter_map(|item| item.split(" as ").next().map(|m| m.trim().to_owned()))
                .filter(|m| !m.is_empty())
                .collect()
        }
        ("py", "import_from_statement") => {
            let text = node.utf8_text(source).unwrap_or("").trim();
            let Some(after) = text.strip_prefix("from ") else {
                return Vec::new();
            };
            vec![after.split(" import").next().unwrap_or(after).to_owned()]
        }
        ("ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs", "import_statement") => {
            // `import { x } from './y'` / `import "z"` → the quoted module,
            // single- or double-quoted.
            let text = node.utf8_text(source).unwrap_or("").trim();
            let Some(start) = text.find('\'').or_else(|| text.find('"')) else {
                return Vec::new();
            };
            let quote = &text[start..start + 1];
            let rest = &text[start + 1..];
            let Some(end) = rest.find(quote) else {
                return Vec::new();
            };
            vec![rest[..end].to_owned()]
        }
        ("go", "import_spec") => {
            let text = node.utf8_text(source).unwrap_or("").trim();
            vec![text.trim_matches('"').to_owned()]
        }
        _ => Vec::new(),
    }
}

/// Recursively collect `extends_clause` / `implements_clause` targets under a
/// TS class node; each clause's named children are the base/interface names.
fn collect_heritage(
    node: &tree_sitter::Node,
    source: &[u8],
    qualified: &str,
    edges: &mut StructuralEdges,
) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "extends_clause" | "implements_clause" => {
                let mut inner = child.walk();
                for target in child.named_children(&mut inner) {
                    let name = extract_name(&target, source);
                    if name.is_empty() {
                        continue;
                    }
                    if child.kind() == "implements_clause" {
                        edges.implements.push((qualified.to_owned(), name));
                    } else {
                        edges.extends.push((qualified.to_owned(), name));
                    }
                }
            }
            _ => collect_heritage(&child, source, qualified, edges),
        }
    }
}

/// Inheritance clauses: Python bases, TS heritage clauses, Rust trait impls.
fn extract_inheritance(
    node: &tree_sitter::Node,
    source: &[u8],
    ext: &str,
    qualified: &str,
    edges: &mut StructuralEdges,
) {
    match ext {
        "py" => {
            if node.kind() == "class_definition" {
                if let Some(supers) = node.child_by_field_name("superclasses") {
                    let text = supers.utf8_text(source).unwrap_or("");
                    for base in text.split(',') {
                        let base = base.trim().trim_start_matches('(').trim_end_matches(')');
                        if !base.is_empty() {
                            edges.extends.push((qualified.to_owned(), base.to_owned()));
                        }
                    }
                }
            }
        }
        "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" => {
            // The grammar nests `extends_clause` / `implements_clause` under
            // `class_heritage` on the class node.
            if node.kind() == "class_declaration" || node.kind() == "class" {
                collect_heritage(node, source, qualified, edges);
            }
        }
        "rs" => {
            // `impl Trait for Type` → Type implements Trait.
            if node.kind() == "impl_item" {
                if let (Some(trait_node), Some(type_node)) = (
                    node.child_by_field_name("trait"),
                    node.child_by_field_name("type"),
                ) {
                    let trait_name = extract_name(&trait_node, source);
                    let type_name = extract_name(&type_node, source);
                    if !trait_name.is_empty() && !type_name.is_empty() {
                        edges.implements.push((type_name, trait_name));
                    }
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod dotted_reference_tests {
    use super::*;

    fn calls(source: &str, ext: &str, names: &[&str]) -> Vec<(String, String)> {
        let set: std::collections::HashSet<String> =
            names.iter().map(|n| (*n).to_owned()).collect();
        TreeSitterChunker::new()
            .expect("chunker")
            .extract_calls(source, ext, &set)
            .expect("calls")
    }

    #[test]
    fn dotted_reference_keeps_receiver_hint() {
        assert_eq!(dotted_reference("self.store.save", "save"), "store.save");
        assert_eq!(dotted_reference("self.store.save", "wrong"), "wrong");
        assert_eq!(dotted_reference("self.save", "save"), "save");
        assert_eq!(dotted_reference("save", "save"), "save");
        assert_eq!(dotted_reference("client.db.query", "query"), "db.query");
    }

    #[test]
    fn rust_method_calls_carry_their_receiver_hint() {
        let source = "impl Engine {\n    fn drive(&self) {\n        self.store.save();\n    }\n}\nstruct Store;\nimpl Store {\n    fn save(&self) {}\n}\n";
        let edges = calls(
            source,
            "rs",
            &["save", "drive", "Store::save", "Engine::drive"],
        );
        assert!(
            edges.iter().any(|(_caller, callee)| callee == "store.save"),
            "edges: {edges:?}"
        );
    }

    #[test]
    fn jsx_element_usage_counts_as_reference() {
        // scan-reliability AC5: `<Button …>` inside a component is a usage of
        // Button — dead-code analytics must see it as an incoming edge.
        let source = "function Page() {\n    return (<div>\n        <Button onClick={handleClick} />\n    </div>);\n}\n";
        let edges = calls(source, "tsx", &["Page", "Button", "handleClick"]);
        assert!(
            edges
                .iter()
                .any(|(caller, callee)| caller == "Page" && callee == "Button"),
            "JSX usage must yield (Page, Button): {edges:?}"
        );
        // Lowercase DOM intrinsics must NOT emit edges (they are not entities).
        assert!(
            !edges.iter().any(|(_c, callee)| callee == "div"),
            "div is a DOM intrinsic, not a reference: {edges:?}"
        );
    }
}

#[cfg(test)]
mod receiver_chain_tests {
    use super::*;

    fn anchors_for(source: &str, ext: &str) -> Vec<String> {
        let chunker = TreeSitterChunker::new().expect("chunker");
        let candidates = chunker.chunk_with_ext(source, ext).expect("chunk ok");
        candidates
            .iter()
            .map(|c| {
                c.location
                    .as_ref()
                    .and_then(|l| l.anchor.clone())
                    .unwrap_or_default()
            })
            .collect()
    }

    #[test]
    fn rust_methods_carry_their_impl_receiver() {
        let source = "impl Foo {\n    fn bar(&self) {}\n    fn baz(&self) {}\n}\n";
        let anchors = anchors_for(source, "rs");
        assert!(
            anchors.contains(&"impl Foo".to_owned()),
            "anchors: {anchors:?}"
        );
        assert!(
            anchors.contains(&"fn Foo::bar".to_owned()),
            "anchors: {anchors:?}"
        );
        assert!(
            anchors.contains(&"fn Foo::baz".to_owned()),
            "anchors: {anchors:?}"
        );
    }

    #[test]
    fn trait_impl_methods_use_the_concrete_type() {
        let source = "impl Display for Foo {\n    fn fmt(&self) {}\n}\n";
        let anchors = anchors_for(source, "rs");
        assert!(
            anchors.contains(&"fn Foo::fmt".to_owned()),
            "anchors: {anchors:?}"
        );
    }

    #[test]
    fn nested_declarations_chain_all_receivers() {
        let source = "struct Outer {\n}\nimpl Outer {\n    fn run(&self) {}\n}\n";
        let anchors = anchors_for(source, "rs");
        assert!(
            anchors.contains(&"struct Outer".to_owned()),
            "anchors: {anchors:?}"
        );
        assert!(
            anchors.contains(&"impl Outer".to_owned()),
            "anchors: {anchors:?}"
        );
        assert!(
            anchors.contains(&"fn Outer::run".to_owned()),
            "anchors: {anchors:?}"
        );
    }

    #[test]
    fn top_level_functions_stay_bare() {
        let source = "fn parse(input: &str) {}\n";
        let anchors = anchors_for(source, "rs");
        assert_eq!(anchors, vec!["fn parse".to_owned()]);
    }

    #[test]
    fn python_methods_carry_their_class_receiver() {
        let source = "class Config:\n    def parse(self):\n        pass\n";
        let anchors = anchors_for(source, "py");
        assert!(
            anchors.contains(&"class Config".to_owned()),
            "anchors: {anchors:?}"
        );
        assert!(
            anchors.contains(&"def Config::parse".to_owned()),
            "anchors: {anchors:?}"
        );
    }

    #[test]
    fn typescript_methods_carry_their_class_receiver() {
        let source = "class Greeter {\n    greet(): void {}\n}\n";
        let anchors = anchors_for(source, "ts");
        assert!(
            anchors.contains(&"class Greeter".to_owned()),
            "anchors: {anchors:?}"
        );
        assert!(
            anchors.contains(&"fn Greeter::greet".to_owned()),
            "anchors: {anchors:?}"
        );
    }
}

#[cfg(test)]
mod lexical_text_tests {
    use super::*;

    fn chunk_texts(source: &str, ext: &str) -> Vec<(String, String)> {
        TreeSitterChunker::new()
            .expect("chunker")
            .chunk_with_ext(source, ext)
            .expect("chunk")
            .into_iter()
            .map(|c| {
                let anchor = c
                    .location
                    .as_ref()
                    .and_then(|l| l.anchor.clone())
                    .unwrap_or_default();
                (anchor, c.text)
            })
            .collect()
    }

    #[test]
    fn leaf_chunks_carry_their_docstring() {
        let chunks = chunk_texts(
            "/// Persists the aggregate root to the store.\nfn save_all(root: &Root) {}\n",
            "rs",
        );
        let (anchor, text) = chunks.iter().find(|(a, _)| a == "fn save_all").expect("fn");
        let _ = anchor;
        assert!(
            text.contains("Persists the aggregate root"),
            "docstring missing from chunk text: {text:?}"
        );
        assert!(text.contains("fn save_all"), "signature missing: {text:?}");
    }

    #[test]
    fn container_chunks_are_lexically_searchable() {
        // A class with a method is a container: its text is synthesized
        // (docstring + signature) instead of empty, so lexical search can
        // find it — while the multi-line body stays out.
        let chunks = chunk_texts(
            "// Manages user sessions.\nclass SessionManager {\n    fn touch(&self) {}\n}\n",
            "ts",
        );
        let (_, text) = chunks
            .iter()
            .find(|(a, _)| a == "class SessionManager")
            .expect("class");
        assert!(
            text.contains("Manages user sessions"),
            "container docstring missing: {text:?}"
        );
        assert!(
            text.contains("class SessionManager"),
            "container signature missing: {text:?}"
        );
        assert!(
            !text.contains("touch"),
            "container text must not include the body: {text:?}"
        );
    }

    #[test]
    fn python_docstrings_attach() {
        let chunks = chunk_texts(
            "# Validates the incoming payload.\ndef validate(payload):\n    pass\n",
            "py",
        );
        let (_, text) = chunks
            .iter()
            .find(|(a, _)| a == "def validate")
            .expect("def");
        assert!(text.contains("Validates the incoming payload"), "{text:?}");
    }

    #[test]
    fn undocummented_declarations_unchanged() {
        let chunks = chunk_texts("fn plain() {}\n", "rs");
        let (_, text) = chunks.iter().find(|(a, _)| a == "fn plain").expect("fn");
        assert_eq!(text, "fn plain() {}");
    }
}
