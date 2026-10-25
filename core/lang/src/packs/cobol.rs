//! COBOL (legacy modernization): paragraphs and sections in the PROCEDURE
//! DIVISION become fn anchors; divisions carry program structure.

use super::{kinds, KindMap};
use engram_code::TreeSitterChunker;

fn kind_map() -> KindMap {
    kinds(&[("paragraph_header", "fn"), ("program_definition", "module")])
}

pub fn register(chunker: &mut TreeSitterChunker) {
    let language: tree_sitter::Language = tree_sitter_cobol::LANGUAGE.into();
    chunker.register_language(&["cbl", "cob", "cpy"], language, kind_map());
}
