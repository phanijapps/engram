//! Code-symbol noise filter (moved from `adapters/ingest/src/extractor.rs`,
//! RFC-0020 Phase 1). Shared by entity emission and call extraction so
//! unknown-but-plausible callees reach the resolution ledger while bare
//! generics (`new`, `clone`, `len`, …) never do.

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

/// Rejects code-symbol names that are too generic to be useful graph nodes —
/// language primitives and ubiquitous one-word methods (`new`, `clone`, `len`,
/// `fmt`, …) that, as bare names, collide across every crate and become massive
/// cross-cutting hubs with no stable identity (RFC-0020 Phase 1).
///
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
    // Punctuation-only / non-alphanumeric / non-alpha-leading sanity.
    if !lower.chars().any(|c| c.is_alphanumeric()) {
        return true;
    }
    if !lower.starts_with(|c: char| c.is_alphabetic()) {
        return true;
    }
    // Bare-generic names. Primitives/type words first, then the ubiquitous
    // one-word methods named in RFC-0020 Phase 1 and the AgentZero indexing
    // guidance (`get`, `str`, `append`, `new`, `clone`, `read`, `write`, …).
    const SYMBOL_NOISE: &[&str] = &[
        // Language primitives & type words.
        "str", "string", "int", "integer", "float", "double", "bool", "boolean", "void", "null",
        "none", "nil", "true", "false", "self", "super", "this", "type", "kind", "value", "pub",
        "var", "let", "const", "static", "object", "array", "map", "set", "list", "dict", "tuple",
        "vector", "vec", "option", "result", "box", "rc", "arc", "ref", "u8", "u16", "u32", "u64",
        "i8", "i16", "i32", "i64", "f32", "f64", "usize", "isize",
        // Generic ubiquitous one-word symbols — bare, they collide across every
        // crate and dominate centrality without a stable identity.
        "new", "clone", "copy", "len", "fmt", "format", "print", "log", "get", "set", "run", "init",
        "send", "recv", "read", "write", "open", "close", "append", "name", "main",
    ];
    SYMBOL_NOISE.contains(&lower.as_str())
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
    fn rejects_garbage() {
        assert!(is_noise_symbol(""));
        assert!(is_noise_symbol("..."));
        assert!(is_noise_symbol("123abc"));
        assert!(is_noise_symbol("###"));
    }
}
