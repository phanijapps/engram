//! AUDIT: identifier normalization battery — what `normalize_identifier_text`
//! (the pre-tokenization step feeding BM25) does to non-ASCII-identifier
//! shapes. Companion to `adapters/ingest/tests/audit_symbol_name_survival.rs`:
//! these assertions are the lexical half of the mangling story (the ingest
//! half lives there).
//!
//! The normalizer lowercases and separates per Unicode rules, with two
//! documented behaviors when tree-sitters for identifier-richer languages
//! land:
//!
//!   1. (FIXED 2026-10-19) Unicode identifiers keep their token — a Rust/JS
//!      `名前を返す` normalizes to itself, so BM25 can match it (it used to
//!      normalize to "", leaving the entity search-blind).
//!   2. (ACCEPTED) `$`-names (`$apply`), primes (`foo'`), bangs/questions
//!      (`save!`, `valid?`) normalize to their stem. This is symmetric on
//!      query and index sides (both run this function), and the exact-name
//!      search paths compare raw names — so search works; the trade-off is
//!      lexical-stem collision, not blindness.

use engram_store_lexical::normalize_identifier_text;

#[test]
fn regression_unicode_identifier_keeps_its_token() {
    assert_eq!(normalize_identifier_text("名前を返す"), "名前を返す");
    assert_eq!(normalize_identifier_text("größe"), "größe");
    assert_eq!(normalize_identifier_text("GrößeMessung"), "größe messung");
}

#[test]
fn accepted_stem_behavior_for_dollar_prime_bang_question() {
    assert_eq!(normalize_identifier_text("$apply"), "apply");
    assert_eq!(normalize_identifier_text("foo'"), "foo");
    assert_eq!(normalize_identifier_text("save!"), "save");
    assert_eq!(normalize_identifier_text("valid?"), "valid");
    assert_eq!(normalize_identifier_text("_tokens"), "tokens");
}

#[test]
fn accepted_cpp_operators_share_a_lexical_stem() {
    assert_eq!(normalize_identifier_text("operator=="), "operator");
    assert_eq!(normalize_identifier_text("operator+"), "operator");
    assert_eq!(normalize_identifier_text("operator<<"), "operator");
}

#[test]
fn control_ascii_identifiers_normalize_as_documented() {
    assert_eq!(normalize_identifier_text("parse_error"), "parse error");
    assert_eq!(normalize_identifier_text("parseError"), "parse error");
    assert_eq!(normalize_identifier_text("run_gate"), "run gate");
}
