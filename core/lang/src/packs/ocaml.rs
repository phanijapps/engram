//! OCaml: `value_definition` (let bindings) and `module_definition` carry
//! names through binding patterns; type definitions via `type_definition`.

use super::{kinds, KindMap};
use engram_code::TreeSitterChunker;

fn kind_map() -> KindMap {
    kinds(&[
        ("value_definition", "fn"),
        ("module_definition", "module"),
        ("type_definition", "type"),
    ])
}

pub fn register(chunker: &mut TreeSitterChunker) {
    let language: tree_sitter::Language = tree_sitter_ocaml::LANGUAGE_OCAML.into();
    chunker.register_language(&["ml"], language, kind_map());
}
