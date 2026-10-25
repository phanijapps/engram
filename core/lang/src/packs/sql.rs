//! SQL (legacy modernization): stored procedures, functions, views, and
//! tables become anchors; call graphs flow through procedure calls.

use super::{kinds, KindMap};
use engram_code::TreeSitterChunker;

fn kind_map() -> KindMap {
    kinds(&[
        ("create_function", "fn"),
        ("create_procedure", "fn"),
        ("create_view", "type"),
        ("create_table", "struct"),
    ])
}

pub fn register(chunker: &mut TreeSitterChunker) {
    let language: tree_sitter::Language = tree_sitter_sql::LANGUAGE.into();
    chunker.register_language(&["sql"], language, kind_map());
}
