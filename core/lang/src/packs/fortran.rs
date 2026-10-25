//! Fortran (legacy modernization): program/module/subroutine/function
//! statements become anchors.

use super::{kinds, KindMap};
use engram_code::TreeSitterChunker;

fn kind_map() -> KindMap {
    kinds(&[
        ("subroutine_statement", "fn"),
        ("function_statement", "fn"),
        ("program_statement", "module"),
        ("module_statement", "module"),
        ("derived_type_definition", "struct"),
    ])
}

pub fn register(chunker: &mut TreeSitterChunker) {
    let language: tree_sitter::Language = tree_sitter_fortran::LANGUAGE.into();
    chunker.register_language(
        &["f", "for", "f90", "f95", "f03", "f08"],
        language,
        kind_map(),
    );
}
