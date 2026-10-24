//! Repo-evidence collection (receiver-aware call resolution, plan v6 §1/§4).
//!
//! `collect_evidence` walks one parsed file and feeds a
//! [`RepoContextBuilder`]: method-declaration anchors with normalized return
//! evidence, type/module declarations, and struct fields. The adapter runs it
//! over EVERY eligible file (changed and unchanged) before extraction, so
//! receiver evidence is repo-wide. Pure — no I/O, no storage.

use crate::parser::repo_context::{RepoContextBuilder, ReturnEvidence, normalize_return};

/// Collects receiver evidence from one file into `builder`.
///
/// Language coverage mirrors the lexical inference set (`rs`, TS/JS, java):
/// - rs: `impl_item` methods (return types via the `return_type` field),
///   `struct_item` fields, `enum/trait/union` declarations, `mod_item`.
/// - ts/tsx/js: `class_declaration` (methods + typed properties),
///   `abstract_class_declaration`.
/// - java: `class_declaration` + `method_declaration` (return `type` field).
///
/// Anchors are receiver-qualified exactly as the declaration walker names
/// entities (`mod m { impl T { fn x } }` → `m::T::x`), so table lookups and
/// entity names agree.
pub fn collect_evidence(
    node: tree_sitter::Node,
    source: &[u8],
    ext: &str,
    receivers: &[String],
    builder: &mut RepoContextBuilder,
) {
    let kind = node.kind();
    let text = |n: tree_sitter::Node| n.utf8_text(source).unwrap_or("");

    match kind {
        // ── Rust ─────────────────────────────────────────────────────────
        "impl_item" if ext == "rs" => {
            let owner = node
                .child_by_field_name("type")
                .map(|t| extract_name(t, source))
                .unwrap_or_default();
            if owner.is_empty() {
                return;
            }
            let mut child_receivers = receivers.to_vec();
            child_receivers.push(owner.clone());
            let owner_path = child_receivers.join("::");
            // Struct-qualified owner registered for Self-resolution and
            // owner-uniqueness (short name registered by declare_type).
            // Methods live inside the impl's `declaration_list` body (one
            // level down), not as direct children.
            let body = node.child_by_field_name("body").or_else(|| {
                node.named_children(&mut node.walk())
                    .find(|c| c.kind() == "declaration_list")
            });
            if let Some(body) = body {
                let mut cursor = body.walk();
                for child in body.named_children(&mut cursor) {
                    if child.kind() == "function_item" {
                        if let Some(anchor_owner) =
                            method_anchor(child, source, &child_receivers, "name")
                        {
                            let evidence = child
                                .child_by_field_name("return_type")
                                .map(|r| normalize_return(text(r), &owner_path))
                                .unwrap_or(ReturnEvidence::Unsupported);
                            builder.declare_method(&anchor_owner, evidence);
                        }
                    }
                    collect_evidence(child, source, ext, &child_receivers, builder);
                }
            }
        }
        "struct_item" if ext == "rs" => {
            let name = node
                .child_by_field_name("name")
                .map(|n| extract_name(n, source))
                .unwrap_or_default();
            if name.is_empty() {
                return;
            }
            let mut child_receivers = receivers.to_vec();
            child_receivers.push(name.clone());
            let qualified = child_receivers.join("::");
            builder.declare_type(&qualified);
            if let Some(body) = node.child_by_field_name("body") {
                let mut cursor = body.walk();
                for field in body.named_children(&mut cursor) {
                    if field.kind() == "field_declaration" {
                        let fname = field
                            .child_by_field_name("name")
                            .map(|n| extract_name(n, source))
                            .unwrap_or_default();
                        let ftype = field
                            .child_by_field_name("type")
                            .and_then(|t| clean_type(text(t)));
                        if let (Some(f), Some(t)) = (Some(fname).filter(|f| !f.is_empty()), ftype) {
                            builder.declare_struct_field(&name, &f, &t);
                        }
                    }
                }
            }
        }
        "enum_item" | "trait_item" | "union_item" if ext == "rs" => {
            let name = node
                .child_by_field_name("name")
                .map(|n| extract_name(n, source))
                .unwrap_or_default();
            if !name.is_empty() {
                let mut child_receivers = receivers.to_vec();
                child_receivers.push(name);
                builder.declare_type(&child_receivers.join("::"));
            }
        }
        "mod_item" => {
            let name = node
                .child_by_field_name("name")
                .map(|n| extract_name(n, source))
                .unwrap_or_default();
            if !name.is_empty() {
                builder.declare_module(&name);
                let mut child_receivers = receivers.to_vec();
                child_receivers.push(name);
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    collect_evidence(child, source, ext, &child_receivers, builder);
                }
                return;
            }
        }
        // ── TS/JS ────────────────────────────────────────────────────────
        "class_declaration" | "abstract_class_declaration" => {
            let name = node
                .child_by_field_name("name")
                .map(|n| extract_name(n, source))
                .unwrap_or_default();
            if name.is_empty() {
                return;
            }
            let mut child_receivers = receivers.to_vec();
            child_receivers.push(name.clone());
            builder.declare_type(&child_receivers.join("::"));
            if let Some(body) = node.child_by_field_name("body") {
                let mut cursor = body.walk();
                for child in body.named_children(&mut cursor) {
                    match child.kind() {
                        "method_definition" | "method_signature" => {
                            if let Some(anchor) =
                                method_anchor(child, source, &child_receivers, "name")
                            {
                                let evidence = child
                                    .child_by_field_name("return_type")
                                    .and_then(|r| clean_type(text(r)))
                                    .map(ReturnEvidence::Returns)
                                    .unwrap_or(ReturnEvidence::Unsupported);
                                builder.declare_method(&anchor, evidence);
                            }
                        }
                        "property_definition"
                        | "property_signature"
                        | "public_field_definition" => {
                            let fname = child
                                .child_by_field_name("name")
                                .map(|n| extract_name(n, source))
                                .unwrap_or_default();
                            let ftype = child
                                .child_by_field_name("type")
                                .and_then(|t| clean_type(text(t)));
                            if let (f, Some(t)) = (fname, ftype) {
                                if !f.is_empty() {
                                    builder.declare_struct_field(&name, &f, &t);
                                }
                            }
                        }
                        _ => {}
                    }
                    collect_evidence(child, source, ext, &child_receivers, builder);
                }
            }
        }
        // ── Java ─────────────────────────────────────────────────────────
        "method_declaration" if ext == "java" => {
            if let Some(anchor) = method_anchor(node, source, receivers, "name") {
                let evidence = node
                    .child_by_field_name("type")
                    .and_then(|t| clean_type(text(t)))
                    .map(ReturnEvidence::Returns)
                    .unwrap_or(ReturnEvidence::Unsupported);
                builder.declare_method(&anchor, evidence);
            }
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_evidence(child, source, ext, receivers, builder);
    }
}

/// Anchor for a method declaration under `receivers`: `receivers…::name`.
fn method_anchor(
    node: tree_sitter::Node,
    source: &[u8],
    receivers: &[String],
    name_field: &str,
) -> Option<String> {
    let name = node
        .child_by_field_name(name_field)
        .map(|n| extract_name(n, source))
        .filter(|n| !n.is_empty())?;
    Some(if receivers.is_empty() {
        name
    } else {
        format!("{}::{}", receivers.join("::"), name)
    })
}

/// Extracts a name from a name node (mirrors the chunker's extract_name
/// behavior for identifier-ish nodes: generic args stripped, paths kept).
fn extract_name(node: tree_sitter::Node, source: &[u8]) -> String {
    let text = node.utf8_text(source).unwrap_or("");
    clean_type(text).unwrap_or_else(|| text.trim().to_owned())
}

/// Cleans a type text: strips generics and whitespace; None for empties.
fn clean_type(text: &str) -> Option<String> {
    let t = text.trim();
    let t = match t.find('<') {
        Some(i) => &t[..i],
        None => t,
    };
    let t = t.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_owned())
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::parser::repo_context::{DeclEvidence, RepoContext};

    fn parse_rs(text: &str) -> tree_sitter::Tree {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();
        parser.parse(text, None).unwrap()
    }

    fn collect(src: &str) -> RepoContext {
        let tree = parse_rs(src);
        let mut b = RepoContext::builder();
        collect_evidence(tree.root_node(), src.as_bytes(), "rs", &[], &mut b);
        b.build()
    }

    #[test]
    fn impl_methods_collect_with_returns_and_self() {
        let ctx = collect(
            "pub struct Searcher;\nimpl SearcherBuilder {\n    pub fn new() -> SearcherBuilder { Self }\n    pub fn build(&self) -> Searcher { Searcher }\n}\n",
        );
        assert_eq!(
            ctx.return_of("SearcherBuilder::new"),
            Some(&DeclEvidence::Returns("SearcherBuilder".into()))
        );
        assert_eq!(
            ctx.return_of("SearcherBuilder::build"),
            Some(&DeclEvidence::Returns("Searcher".into()))
        );
        assert_eq!(ctx.anchor_count("SearcherBuilder::build"), 1);
    }

    #[test]
    fn nested_module_anchors_are_qualified() {
        let ctx = collect(
            "mod remote {\n    pub struct Product;\n    impl Product {\n        pub fn setting(&self) -> remote::Product { remote::Product }\n    }\n}\n",
        );
        assert_eq!(
            ctx.return_of("remote::Product::setting"),
            Some(&DeclEvidence::Returns("remote::Product".into()))
        );
    }

    #[test]
    fn struct_fields_collect() {
        let ctx = collect("struct Config {\n    searcher: Searcher,\n    count: u32,\n}\n");
        assert_eq!(ctx.field_type_of("Config", "searcher"), Some("Searcher"));
        assert!(ctx.is_known_type("Config"));
    }

    #[test]
    fn collapsed_anchors_count_declaration_events() {
        let ctx = collect(
            "trait A {}\ntrait B {}\nstruct T;\nimpl A for T {\n    fn x(&self) {}\n}\nimpl B for T {\n    fn x(&self) {}\n}\n",
        );
        assert_eq!(ctx.anchor_count("T::x"), 2);
    }

    #[test]
    fn module_declarations_collect() {
        let ctx = collect("#[allow(non_snake_case)]\nmod Snake;\n");
        assert!(ctx.is_known_module("Snake"));
    }
}
