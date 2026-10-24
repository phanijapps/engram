//! Code-symbol noise filter (moved from `adapters/ingest/src/extractor.rs`,
//! RFC-0020 Phase 1). Two policies live here — they look similar but answer
//! different questions:
//!
//! * [`is_noise_declaration`] — is this DECLARED symbol name garbage? Used at
//!   entity emission. Deliberately permissive: leading `_` (Python/Rust/JS
//!   private-by-convention helpers, `__init__`) and `$` (JS/framework names)
//!   are legal identifier shapes carrying real logic, and bare generics
//!   (`new`, `run`, `String`) are kept because qualified identities
//!   (`Foo::new`, RFC-0020 Phase 2) disambiguate them across receivers.
//! * [`is_noise_symbol`] — is this unknown CALLEE name too generic to be worth
//!   a ledger entry? Bare generics (`new`, `clone`, `len`, …) collide across
//!   every crate and would flood the unresolved-refs ledger, so they are
//!   dropped here — but a callee that IS declared resolves normally before
//!   this filter is consulted (the caller checks `entity_names` first).

/// Reject entities that aren't real concepts — punctuation tokens, single-char
/// symbols, code-block delimiters, common type annotations, YAML keys.
/// Returns true = "this is noise, skip it."
///
/// Reference implementation for the TS `extract-knowledge` noise filter
/// (RFC-0020 T5): no longer called from the Rust extractor after T3 removed
/// document→Concept emission, but kept as the canonical logic the TS op ports
/// (plus a doc-heading-generic blocklist).
#[allow(dead_code)]
fn is_noise_concept(name: &str) -> bool {
    if name.len() < 3 {
        return true;
    }
    // Must contain at least one alphanumeric char (reject punctuation-only).
    if !name.chars().any(|c| c.is_alphanumeric()) {
        return true;
    }
    let lower = name.to_lowercase();
    // Common type annotations / system words that aren't real concepts.
    const TYPE_NOISE: &[&str] = &[
        "str",
        "string",
        "int",
        "float",
        "bool",
        "void",
        "null",
        "none",
        "nil",
        "true",
        "false",
        "self",
        "super",
        "this",
        "type",
        "kind",
        "value",
        "name",
        "pub",
        "var",
        "let",
        "const",
        "fn",
        "def",
        "class",
        "struct",
        "enum",
        "import",
        "export",
        "return",
        "async",
        "await",
        "yield",
        "static",
        "u8",
        "u16",
        "u32",
        "u64",
        "i8",
        "i16",
        "i32",
        "i64",
        "f32",
        "f64",
        "usize",
        "isize",
        "vec",
        "option",
        "result",
        "box",
        "rc",
        "arc",
        "object",
        "array",
        "map",
        "set",
        "list",
        "dict",
        "tuple",
        "models",
        "description",
        "available",
        "contents",
        "approach",
        "append",
        "clone",
        "print",
        "join",
        "exists",
        "encode",
    ];
    if TYPE_NOISE.contains(&lower.as_str()) {
        return true;
    }
    // Reject "key: value" patterns (YAML/TOML keys like "type: string").
    if name.contains(':') && name.split(':').count() == 2 {
        return true;
    }
    // Reject if it starts with a non-alpha char (likely code noise).
    if !name.starts_with(|c: char| c.is_alphabetic()) {
        return true;
    }
    false
}

/// Rejects DECLARED symbol names that are garbage — empty, punctuation-only,
/// or starting with a character no identifier grammar allows (a digit, or
/// punctuation other than `_` / `$`).
///
/// This is the entity-emission policy (the noise half of the RFC-0020 Phase 1
/// filter split): it must NOT drop real declarations. Leading `_` (Python
/// `_helper`, `__init__`, Rust `_unused`) and `$` (JS `$apply`, framework
/// names) are private-by-convention, not noise, and bare generics (`new`,
/// `run`, `String`) are kept — their qualified identities disambiguate them,
/// and dropping them erased entire constructor/interface surfaces (Rust
/// `Type::new`, Go `Read`/`Write`/`String`/`Error` — reproduced in
/// `adapters/ingest/tests/audit_symbol_name_survival.rs`).
///
/// Returns true = "this declaration is noise, skip it."
pub fn is_noise_declaration(name: &str) -> bool {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return true;
    }
    // Must contain at least one alphanumeric char (reject punctuation-only
    // tokens like "..." / "###" / "->").
    if !trimmed.chars().any(|c| c.is_alphanumeric()) {
        return true;
    }
    // Leading char: letters (any script), `_`, or `$` are identifier shapes.
    // A leading digit (`123abc`) or any other punctuation (`@foo`, `#foo`)
    // is garbage in every identifier grammar we ingest.
    !trimmed.starts_with(|c: char| c.is_alphabetic() || c == '_' || c == '$')
}

/// Rejects code-symbol names that are too generic to be useful graph nodes —
/// language primitives and ubiquitous one-word methods (`new`, `clone`, `len`,
/// `fmt`, …) that, as bare names, collide across every crate and become massive
/// cross-cutting hubs with no stable identity (RFC-0020 Phase 1).
///
/// This is the unknown-CALLEE policy: applied to callee names only AFTER a
/// declared-entity lookup misses, so a repo that declares one of these
/// resolves normally and only the unresolved-ledger entry is skipped.
/// Tuned for CODE, so unlike [`is_noise_concept`] it does NOT reject short
/// names — `tx`, `db`, `id`, `kv` are meaningful identifiers in code. Only the
/// bare-generic set is blocked. This is the pre-qualified-identity filter: once
/// `parse_symbol` emits qualified identities (`{repo}/{path}::{module}::{name}`,
/// RFC-0020 Phase 1), the generic-method portion of this list can be relaxed and
/// only the true type primitives (`str`, `vec`, `option`, …) kept.
///
/// Returns true = "this symbol is noise, skip it."
pub fn is_noise_symbol(name: &str) -> bool {
    let lower = name.trim().to_lowercase();
    if lower.is_empty() {
        return true;
    }
    // Punctuation-only / non-alphanumeric sanity.
    if !lower.chars().any(|c| c.is_alphanumeric()) {
        return true;
    }
    // Leading char must be an identifier shape (letter, `_`, or `$`) — same
    // rule as [`is_noise_declaration`]. Historically this rejected every
    // leading-underscore callee (`_helper`), erasing cross-file private-helper
    // references from the resolution ledger.
    if !lower.starts_with(|c: char| c.is_alphabetic() || c == '_' || c == '$') {
        return true;
    }
    // Bare-generic names. Primitives/type words first, then the ubiquitous
    // one-word methods named in RFC-0020 Phase 1 and the AgentZero indexing
    // guidance (`get`, `str`, `append`, `new`, `clone`, `read`, `write`, …).
    const SYMBOL_NOISE: &[&str] = &[
        // Language primitives & type words.
        "str",
        // Rust enum constructors + panics (bare position; receiver-verified
        // qualified forms bypass via the state machine's qualified path).
        "Ok",
        "Some",
        "None",
        "Err",
        "unwrap",
        "expect",
        "string",
        "int",
        "integer",
        "float",
        "double",
        "bool",
        "boolean",
        "void",
        "null",
        "none",
        "nil",
        "true",
        "false",
        "self",
        "super",
        "this",
        "type",
        "kind",
        "value",
        "pub",
        "var",
        "let",
        "const",
        "static",
        "object",
        "array",
        "map",
        "set",
        "list",
        "dict",
        "tuple",
        "vector",
        "vec",
        "option",
        "result",
        "box",
        "rc",
        "arc",
        "ref",
        "u8",
        "u16",
        "u32",
        "u64",
        "i8",
        "i16",
        "i32",
        "i64",
        "f32",
        "f64",
        "usize",
        "isize",
        // Generic ubiquitous one-word symbols — bare, they collide across every
        // crate and dominate centrality without a stable identity.
        "new",
        "clone",
        "copy",
        "len",
        "fmt",
        "format",
        "print",
        "log",
        "get",
        "set",
        "run",
        "init",
        "send",
        "recv",
        "read",
        "write",
        "open",
        "close",
        "append",
        "name",
        "main",
        // code-graph-quality [java-resolution]: JDK/stdlib/chained-call idioms
        // that arrive bare and never resolve to a stable identity — measured
        // as the top unresolved callees on a real Java repo (SpringApplication
        // .run → `run`, log.info → `info`, Stream.builder → `builder`, …).
        // A repo that DECLARES one of these still resolves normally — the
        // filter only drops unknown callees (extract_calls checks
        // entity_names first).
        "info",
        "warn",
        "error",
        "debug",
        "trace",
        "build",
        "stream",
        "forEach",
        "builder",
        "map",
        "filter",
        "collect",
        "toList",
        "of",
        "equals",
        "hashcode",
        "tostring",
        "valueof",
        "isempty",
        "size",
        "add",
        "put",
        "remove",
        "clear",
        "contains",
        "keyset",
        "entryset",
        "getclass",
        "getname",
        "getid",
        "tojson",
        "fromjson",
        "println",
        "concat",
        "trim",
        "substring",
        "indexof",
        "split",
        "replace",
        "tolowercase",
        "touppercase",
        "cast",
    ];
    SYMBOL_NOISE.contains(&lower.as_str())
}

/// Generic METHOD names for receiver-position drop decisions (plan §2):
/// a generic method on an UNKNOWN receiver is worthless (the `Error::clone`
/// / `Glob::as_ref` false-binding class) and is dropped rather than ledgered.
pub fn is_generic_method(name: &str) -> bool {
    const GENERIC_METHODS: &[&str] = &[
        "clone",
        "as_ref",
        "as_mut",
        "to_string",
        "to_owned",
        "to_vec",
        "into",
        "from",
        "unwrap",
        "expect",
        "map",
        "filter",
        "iter",
        "into_iter",
        "collect",
        "new",
        "default",
        "build",
        "build_parallel",
        "eq",
        "ne",
        "hash",
        "fmt",
        "len",
        "is_empty",
        "get",
        "set",
        "next",
        "send",
        "spawn",
        "borrow",
        "borrow_mut",
        "deref",
        "drop",
        "extend",
        "into_boxed_slice",
        "copy",
        "compare",
        "display",
        "debug",
        "serialize",
        "deserialize",
    ];
    GENERIC_METHODS.contains(&name.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_bare_generics_and_primitives() {
        assert!(is_noise_symbol("new"));
        assert!(is_noise_symbol("clone"));
        assert!(is_noise_symbol("len"));
        assert!(is_noise_symbol("Vec"));
        assert!(is_noise_symbol("Option"));
    }

    #[test]
    fn keeps_meaningful_symbols() {
        assert!(!is_noise_symbol("parse_config"));
        assert!(!is_noise_symbol("SymbolIndex"));
        assert!(!is_noise_symbol("scan_repository"));
    }

    #[test]
    fn declaration_policy_keeps_private_and_generic_declarations() {
        // Leading underscore: Python/Rust/JS private helpers are real logic.
        assert!(!is_noise_declaration("_tokens"));
        assert!(!is_noise_declaration("_check_review"));
        assert!(!is_noise_declaration("__init__"));
        // Leading `$`: JS/framework identifier convention.
        assert!(!is_noise_declaration("$apply"));
        // Bare generics are kept as declarations (qualified identities
        // disambiguate): constructors, entrypoints, interface impls.
        assert!(!is_noise_declaration("new"));
        assert!(!is_noise_declaration("String"));
        assert!(!is_noise_declaration("run"));
        // Unicode identifiers are legal letters.
        assert!(!is_noise_declaration("名前を返す"));
        // Trailing punctuation shapes stay declarations (Ruby ?/!, Haskell ').
        assert!(!is_noise_declaration("save!"));
        assert!(!is_noise_declaration("valid?"));
        assert!(!is_noise_declaration("foo'"));
    }

    #[test]
    fn declaration_policy_rejects_garbage() {
        assert!(is_noise_declaration(""));
        assert!(is_noise_declaration("   "));
        assert!(is_noise_declaration("..."));
        assert!(is_noise_declaration("###"));
        assert!(is_noise_declaration("123abc"));
        assert!(is_noise_declaration("->"));
        assert!(is_noise_declaration("@decorator"));
    }

    #[test]
    fn callee_policy_keeps_underscore_and_dollar_callees() {
        // Unknown `_`/`$` callees must reach the resolution ledger (they are
        // plausibly cross-file private helpers), while bare generics stay
        // dropped (a declared `new` resolves before this filter runs).
        assert!(!is_noise_symbol("_helper"));
        assert!(!is_noise_symbol("$apply"));
        assert!(is_noise_symbol("new"));
    }

    #[test]
    fn rejects_garbage() {
        assert!(is_noise_symbol(""));
        assert!(is_noise_symbol("..."));
        assert!(is_noise_symbol("123abc"));
        assert!(is_noise_symbol("###"));
    }
}
