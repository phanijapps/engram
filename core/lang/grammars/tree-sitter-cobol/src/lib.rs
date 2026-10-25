//! COBOL grammar for tree-sitter — vendored from
//! yutaro-sakamoto/tree-sitter-cobol (MIT), packaged for the engram
//! language pack. Regenerate parser.c from grammar.js with
//! `tree-sitter generate` when upgrading.

use tree_sitter_language::LanguageFn;

extern "C" {
    fn tree_sitter_COBOL() -> *const ();
}

/// The COBOL tree-sitter language.
pub const LANGUAGE: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_COBOL) };
