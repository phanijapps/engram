//! Haskell: `function` (name: variable) → fn; `data_type` → struct;
//! `newtype` → struct; `type_synonym`/`type_family` → type; `module` → module.

use super::{kinds, KindMap};
use engram_code::TreeSitterChunker;

fn kind_map() -> KindMap {
    kinds(&[
        ("function", "fn"),
        ("data_type", "struct"),
        ("newtype", "struct"),
        ("type_synonym", "type"),
        ("module", "module"),
    ])
}

pub fn register(chunker: &mut TreeSitterChunker) {
    let language: tree_sitter::Language = tree_sitter_haskell::LANGUAGE.into();
    chunker.register_language(&["hs"], language, kind_map());
}
