//! Lua: `function_declaration` (name field) → fn; methods on tables ride
//! the function_name path chain. Calls route through the receiver evaluator
//! (`dot_index_expression` / `method_index_expression` normalization).

use super::{kinds, KindMap};
use engram_code::TreeSitterChunker;

fn kind_map() -> KindMap {
    kinds(&[
        ("function_declaration", "fn"),
        ("function_definition", "fn"),
    ])
}

pub fn register(chunker: &mut TreeSitterChunker) {
    let language: tree_sitter::Language = tree_sitter_lua::LANGUAGE.into();
    chunker.register_language(&["lua"], language, kind_map());
}
