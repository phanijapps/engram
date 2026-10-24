//! Julia: `function_definition` / `macro_definition` (name fields) → fn;
//! `struct_definition` → struct; `module_definition` → module;
//! `abstract_definition` → type.

use super::{kinds, KindMap};
use engram_code::TreeSitterChunker;

fn kind_map() -> KindMap {
    kinds(&[
        ("function_definition", "fn"),
        ("macro_definition", "fn"),
        ("struct_definition", "struct"),
        ("module_definition", "module"),
        ("abstract_definition", "type"),
    ])
}

pub fn register(chunker: &mut TreeSitterChunker) {
    let language: tree_sitter::Language = tree_sitter_julia::LANGUAGE.into();
    chunker.register_language(&["jl"], language, kind_map());
}
