//! Zig: `Fn` declarations (name field) → fn; container types via
//! container_decl variants → struct/enum per kind.

use super::{kinds, KindMap};
use engram_code::TreeSitterChunker;

fn kind_map() -> KindMap {
    kinds(&[
        ("Fn", "fn"),
        ("FnProto", "fn"),
        ("function_declaration", "fn"),
    ])
}

pub fn register(chunker: &mut TreeSitterChunker) {
    let language: tree_sitter::Language = tree_sitter_zig::LANGUAGE.into();
    chunker.register_language(&["zig"], language, kind_map());
}
