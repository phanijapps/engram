//! Language packs for the engram code chunker.
//!
//! Each pack registers a tree-sitter grammar + a declaration kind map into
//! a [`engram_code::TreeSitterChunker`] at composition time — language
//! support is additive (register a pack), never a rewrite of `engram-code`.
//! Packs are cargo-feature gated so hosts can slim builds.
//!
//! First-cut eligibility: a pack ships only when its declaration shapes
//! produce real symbol anchors (name/declarator/type fields) AND its call
//! shapes route through the conservative receiver evaluation — verified by
//! the per-language fixtures in `tests/packs.rs`.

pub mod packs;

use engram_code::TreeSitterChunker;

/// Registers every enabled pack into `chunker` (idempotent per extension;
/// a later registration replaces an earlier one for the same extension).
pub fn register_all(chunker: &mut TreeSitterChunker) {
    packs::register_all(chunker);
}
