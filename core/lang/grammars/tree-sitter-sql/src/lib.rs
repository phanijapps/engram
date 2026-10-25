//! SQL grammar for tree-sitter — vendored from DerekStride/tree-sitter-sql
//! (MIT; parser.c taken from the crates.io 0.0.2 source package, which ships
//! the generated parser). Packaged for the engram language pack.

use tree_sitter_language::LanguageFn;

extern "C" {
    fn tree_sitter_sql() -> *const ();
}

/// The SQL tree-sitter language.
pub const LANGUAGE: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_sql) };
