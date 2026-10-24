//! Erlang: `function_clause` (name atom) → fn per clause.

use super::{kinds, KindMap};
use engram_code::TreeSitterChunker;

fn kind_map() -> KindMap {
    kinds(&[("function_clause", "fn")])
}

pub fn register(chunker: &mut TreeSitterChunker) {
    let language: tree_sitter::Language = tree_sitter_erlang::LANGUAGE.into();
    chunker.register_language(&["erl", "hrl", "app"], language, kind_map());
}
