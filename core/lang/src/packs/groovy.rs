//! Groovy (+ Gradle scripts): methods and classes become anchors;
//! annotations flow through the JVM framework-facts layer (Spock, Spring).

use super::{kinds, KindMap};
use engram_code::TreeSitterChunker;

fn kind_map() -> KindMap {
    kinds(&[
        ("method_declaration", "fn"),
        ("function_declaration", "fn"),
        ("class_declaration", "class"),
        ("declaration_list", "fn"),
    ])
}

pub fn register(chunker: &mut TreeSitterChunker) {
    let language: tree_sitter::Language = tree_sitter_groovy::LANGUAGE.into();
    chunker.register_language(&["groovy", "gradle"], language, kind_map());
}
