//! Dart: `class_definition` (name) → class; function/method signatures → fn.

use super::{kinds, KindMap};
use engram_code::TreeSitterChunker;

fn kind_map() -> KindMap {
    kinds(&[
        ("class_declaration", "class"),
        ("function_signature", "fn"),
        ("constructor_signature", "fn"),
        ("mixin_declaration", "class"),
        ("enum_declaration", "enum"),
    ])
}

pub fn register(chunker: &mut TreeSitterChunker) {
    let language: tree_sitter::Language = tree_sitter_dart::LANGUAGE.into();
    chunker.register_language(&["dart"], language, kind_map());
}
