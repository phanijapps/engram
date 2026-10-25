//! Per-language packs: grammar + declaration kind maps.
//!
//! Anchor keywords MUST be recognized by the extractor's `parse_symbol`
//! (fn/function/def/func, struct/record, enum, trait, interface, type,
//! class/impl, module/namespace/mod, variable). A pack ships only when its
//! fixtures prove real symbol anchors + conservative call handling — see
//! `tests/packs.rs`.

use std::collections::HashMap;

use engram_code::TreeSitterChunker;

/// Registers every enabled pack (idempotent; later registrations replace
/// earlier ones per extension).
pub fn register_all(chunker: &mut TreeSitterChunker) {
    #[cfg(feature = "lua")]
    lua::register(chunker);
    #[cfg(feature = "julia")]
    julia::register(chunker);
    #[cfg(feature = "zig")]
    zig::register(chunker);
    #[cfg(feature = "dart")]
    dart::register(chunker);
    #[cfg(feature = "erlang")]
    erlang::register(chunker);
    #[cfg(feature = "ocaml")]
    ocaml::register(chunker);
    #[cfg(feature = "haskell")]
    haskell::register(chunker);
    #[cfg(feature = "cobol")]
    cobol::register(chunker);
    #[cfg(feature = "fortran")]
    fortran::register(chunker);
    #[cfg(feature = "sql")]
    sql::register(chunker);
}

pub mod cobol;
pub mod dart;
pub mod erlang;
pub mod fortran;
pub mod haskell;
pub mod julia;
pub mod lua;
pub mod ocaml;
pub mod sql;
pub mod zig;

pub(crate) type KindMap = HashMap<&'static str, &'static str>;

pub(crate) fn kinds(pairs: &[(&'static str, &'static str)]) -> KindMap {
    pairs.iter().copied().collect()
}
