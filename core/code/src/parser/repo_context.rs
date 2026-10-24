//! Repo-wide receiver evidence (receiver-aware call resolution, plan v6).
//!
//! `RepoContext` carries the cross-file evidence the per-file chain state
//! machine consumes: declared method return types keyed by QUALIFIED anchor,
//! short-type-name → qualified-owner resolution, method-anchor declaration
//! counts (collapsed trait anchors), and the known type/module name sets the
//! resolution ladder uses for owner classification.
//!
//! Everything here is pure: the adapter parses files and feeds a
//! [`RepoContextBuilder`]; nothing in this module touches I/O, storage, or
//! language-specific grammar types beyond plain text.

use std::collections::{HashMap, HashSet};

/// Per-key declared-return evidence. Conflicting, unsupported, or ambiguous
/// declarations make the key `Ambiguous` (absorbing).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeclEvidence {
    /// The (single, consistent) declared return path, e.g. `remote::Product`.
    Returns(String),
    /// Conflicting returns, unsupported syntax, or ambiguity. Treated as
    /// "no usable evidence" (Unknown) at every consumer.
    Ambiguous,
}

/// Evidence contributed by ONE declaration of a method anchor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReturnEvidence {
    /// A normalized return path (refs peeled, `crate::` stripped, `Self`
    /// resolved, module path preserved).
    Returns(String),
    /// Raw-pointer returns, generic-wrapped returns (`Option<T>`,
    /// `Result<T, E>`, `Vec<T>`), `impl Trait`, `dyn Trait`, associated
    /// projections, or unparsable syntax.
    Unsupported,
}

/// One canonicalized owner spelling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Canonical {
    /// A unique qualified owner path (`remote::Product`).
    Qualified(String),
    /// A name that is its own (unique, top-level) owner (`Product`).
    Short(String),
    /// Ambiguous alias/owner or unknown name — conservative reject.
    Unknown,
}

/// Normalizes a declared return type text into evidence for
/// [`RepoContextBuilder::declare_method`].
///
/// Rules (plan §1): `&T`/`&mut T` peel refs; `*const T`/`*mut T` are
/// Unsupported (raw-pointer calls are not pointee calls); `crate::m::T`
/// strips `crate::` but keeps the module path; `Self` resolves to the
/// enclosing owner; generic-wrapped / `impl Trait` / `dyn Trait` /
/// associated projections are Unsupported; anything else keeps its written
/// path (`remote::Product`).
pub fn normalize_return(text: &str, owner: &str) -> ReturnEvidence {
    let mut t = text.trim();
    // Peel (possibly repeated) references.
    loop {
        if let Some(rest) = t.strip_prefix('&') {
            t = rest
                .trim_start()
                .strip_prefix("mut ")
                .map(str::trim_start)
                .unwrap_or(rest.trim_start());
            continue;
        }
        if let Some(rest) = t.strip_prefix("'") {
            // Lifetime-qualified reference `&'a T` — the `'a` was consumed by
            // the ref peel above only when adjacent; handle `'a mut T` shapes.
            let _ = rest;
            t = rest
                .split_once(' ')
                .map(|(_, r)| r.trim_start())
                .unwrap_or(rest);
            continue;
        }
        break;
    }
    if t.starts_with("*const ") || t.starts_with("*mut ") {
        return ReturnEvidence::Unsupported;
    }
    // Strip any generic arguments wholesale (`Option<T>`, `Vec<T>`, `Box<T>`)
    // — a generic-wrapped return never establishes the inner nominal type.
    if t.contains('<') {
        return ReturnEvidence::Unsupported;
    }
    if t.starts_with("impl ") || t.starts_with("dyn ") {
        return ReturnEvidence::Unsupported;
    }
    // `Self` / `Self::path` resolves to the owner (or owner::path).
    if t == "Self" {
        return ReturnEvidence::Returns(owner.to_owned());
    }
    if let Some(path) = t.strip_prefix("Self::") {
        return ReturnEvidence::Returns(format!("{owner}::{path}"));
    }
    if t == "()" || t.is_empty() {
        return ReturnEvidence::Unsupported;
    }
    // Strip crate/self prefixes, KEEPING the module path (`crate::m::T` → `m::T`).
    let stripped = t.strip_prefix("crate::").unwrap_or(t);
    let stripped = stripped.strip_prefix("self::").unwrap_or(stripped);
    // The written path is preserved (`remote::Product` or `Product`).
    if stripped
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == ':')
        && !stripped.is_empty()
    {
        ReturnEvidence::Returns(stripped.to_owned())
    } else {
        ReturnEvidence::Unsupported
    }
}

/// Repo-wide receiver evidence. Built only through [`RepoContextBuilder`].
#[derive(Debug, Default, Clone)]
pub struct RepoContext {
    method_returns: HashMap<String, DeclEvidence>,
    type_owners: HashMap<String, HashSet<String>>,
    anchor_counts: HashMap<String, usize>,
    known_types: HashMap<String, HashSet<String>>,
    known_modules: HashSet<String>,
    /// Short struct type → field → type (repo-wide; populated by the
    /// context-collection pass from ALL eligible files).
    struct_fields: HashMap<String, HashMap<String, String>>,
    /// Precomputed fingerprint over all evidence (invalidation trigger).
    fingerprint: u64,
}

impl RepoContext {
    /// An empty context (tests, legacy paths) — every lookup is a miss.
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn builder() -> RepoContextBuilder {
        RepoContextBuilder::default()
    }

    /// Declared return evidence for a qualified anchor (`remote::T::m`).
    pub fn return_of(&self, qualified_anchor: &str) -> Option<&DeclEvidence> {
        self.method_returns.get(qualified_anchor)
    }

    /// Declaration-event count for a qualified anchor. Collapsed (>1)
    /// anchors must not be emitted by typed links.
    pub fn anchor_count(&self, qualified_anchor: &str) -> usize {
        self.anchor_counts
            .get(qualified_anchor)
            .copied()
            .unwrap_or(0)
    }

    /// The unique qualified owner for a short type name, if exactly one.
    pub fn unique_owner(&self, short: &str) -> Option<String> {
        match self.type_owners.get(short) {
            Some(owners) if owners.len() == 1 => owners.iter().next().cloned(),
            _ => None,
        }
    }

    /// All qualified owners registered for a short name.
    pub fn owners_of(&self, short: &str) -> Option<&HashSet<String>> {
        self.type_owners.get(short)
    }

    /// Is the name a known type declaration (any owner count)?
    pub fn is_known_type(&self, name: &str) -> bool {
        self.known_types.contains_key(name)
    }

    /// Is the name a known module declaration?
    pub fn is_known_module(&self, name: &str) -> bool {
        self.known_modules.contains(name)
    }

    /// Canonicalizes an owner name used at a call site: alias → qualified;
    /// short name → unique owner or Unknown; else Short.
    pub fn canonical_owner(&self, name: &str, aliases: &HashMap<String, String>) -> Canonical {
        if let Some(target) = aliases.get(name) {
            let stripped = target.strip_prefix("crate::").unwrap_or(target);
            return Canonical::Qualified(stripped.to_owned());
        }
        if self.is_known_type(name) {
            return match self.unique_owner(name) {
                Some(owner) => Canonical::Qualified(owner),
                None => Canonical::Unknown,
            };
        }
        Canonical::Short(name.to_owned())
    }

    /// Repo-wide struct field type lookup.
    pub fn field_type_of(&self, struct_short: &str, field: &str) -> Option<&str> {
        self.struct_fields
            .get(struct_short)?
            .get(field)
            .map(|s| s.as_str())
    }

    /// Stable fingerprint over all evidence (plan §4 invalidation trigger).
    pub fn fingerprint(&self) -> u64 {
        self.fingerprint
    }

    /// Number of method-return keys (diagnostics/tests).
    pub fn method_count(&self) -> usize {
        self.method_returns.len()
    }
}

/// Incremental builder for [`RepoContext`].
#[derive(Debug, Default)]
pub struct RepoContextBuilder {
    inner: RepoContext,
    /// Pending per-key evidence before absorption.
    pending: HashMap<String, ReturnEvidence>,
}

impl RepoContextBuilder {
    /// Registers a type declaration (`struct`/`enum`/`trait`/`union`) under
    /// its qualified owner path; the short name maps to the owner set.
    pub fn declare_type(&mut self, qualified: &str) {
        let short = qualified.rsplit("::").next().unwrap_or(qualified);
        self.inner
            .type_owners
            .entry(short.to_owned())
            .or_default()
            .insert(qualified.to_owned());
        self.inner
            .known_types
            .entry(short.to_owned())
            .or_default()
            .insert(qualified.to_owned());
    }

    /// Registers a module declaration (`mod x;` / `mod x {}`).
    pub fn declare_module(&mut self, name: &str) {
        self.inner.known_modules.insert(name.to_owned());
    }

    /// Registers a struct field (`struct T { f: U }` → T.f → U), repo-wide.
    pub fn declare_struct_field(&mut self, struct_short: &str, field: &str, ty: &str) {
        self.inner
            .struct_fields
            .entry(struct_short.to_owned())
            .or_default()
            .insert(field.to_owned(), ty.to_owned());
    }

    /// Registers one method declaration event under its qualified anchor
    /// with normalized return evidence.
    pub fn declare_method(&mut self, qualified_anchor: &str, evidence: ReturnEvidence) {
        *self
            .inner
            .anchor_counts
            .entry(qualified_anchor.to_owned())
            .or_insert(0) += 1;
        match self.pending.get_mut(qualified_anchor) {
            // First declaration wins the slot.
            None => {
                self.pending.insert(qualified_anchor.to_owned(), evidence);
            }
            // Idempotent repeat of the SAME evidence: keep. ANY conflict
            // (differing return, or Unsupported beside anything) absorbs to
            // Unsupported (→ Ambiguous at build), and is absorbing thereafter.
            Some(existing) => {
                if *existing != evidence {
                    *existing = ReturnEvidence::Unsupported;
                }
            }
        }
    }

    /// Freezes the builder into a [`RepoContext`].
    pub fn build(mut self) -> RepoContext {
        for (anchor, evidence) in self.pending {
            let decl = match evidence {
                ReturnEvidence::Returns(path) => DeclEvidence::Returns(path),
                ReturnEvidence::Unsupported => DeclEvidence::Ambiguous,
            };
            self.inner.method_returns.insert(anchor, decl);
        }
        self.inner.fingerprint = fingerprint_of(&self.inner);
        self.inner
    }
}

/// Deterministic order-insensitive fingerprint over all evidence maps.
fn fingerprint_of(ctx: &RepoContext) -> u64 {
    fn hash_sorted(items: impl Iterator<Item = String>) -> u64 {
        let mut v: Vec<String> = items.collect();
        v.sort();
        let mut h: u64 = 0xcbf29ce484222325;
        for s in v {
            for b in s.bytes() {
                h ^= b as u64;
                h = h.wrapping_mul(0x100000001b3);
            }
            h ^= 0xff;
            h = h.wrapping_mul(0x100000001b3);
        }
        h
    }
    let returns = ctx.method_returns.iter().map(|(k, e)| match e {
        DeclEvidence::Returns(p) => format!("{k}->{p}"),
        DeclEvidence::Ambiguous => format!("{k}->?"),
    });
    let owners = ctx.type_owners.iter().flat_map(|(s, o)| {
        let mut v: Vec<String> = o.iter().map(|q| format!("{s}={q}")).collect();
        v.sort();
        v
    });
    let counts = ctx.anchor_counts.iter().map(|(k, c)| format!("{k}#{c}"));
    let modules = ctx.known_modules.iter().cloned();
    let fields = ctx.struct_fields.iter().flat_map(|(s, fs)| {
        let mut v: Vec<String> = fs.iter().map(|(f, t)| format!("{s}.{f}:{t}")).collect();
        v.sort();
        v
    });
    let mut h = hash_sorted(returns);
    h ^= hash_sorted(owners);
    h ^= hash_sorted(counts);
    h ^= hash_sorted(modules);
    h ^= hash_sorted(fields);
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx_of(entries: Vec<(&str, ReturnEvidence)>) -> RepoContext {
        let mut b = RepoContext::builder();
        for (anchor, ev) in entries {
            b.declare_method(anchor, ev);
        }
        b.build()
    }

    #[test]
    fn return_normalization_rules() {
        use ReturnEvidence::*;
        assert_eq!(
            normalize_return("&mut WalkBuilder", "W"),
            Returns("WalkBuilder".into())
        );
        assert_eq!(
            normalize_return("remote::Product", "B"),
            Returns("remote::Product".into())
        );
        assert_eq!(
            normalize_return("crate::searcher::Searcher", "B"),
            Returns("searcher::Searcher".into())
        );
        assert_eq!(
            normalize_return("Self", "remote::Builder"),
            Returns("remote::Builder".into())
        );
        assert_eq!(normalize_return("&Self", "T"), Returns("T".into()));
        assert_eq!(normalize_return("*const Product", "F"), Unsupported);
        assert_eq!(normalize_return("*mut Product", "F"), Unsupported);
        assert_eq!(normalize_return("Option<Product>", "F"), Unsupported);
        assert_eq!(normalize_return("Result<T, E>", "F"), Unsupported);
        assert_eq!(normalize_return("impl Settings", "B"), Unsupported);
        assert_eq!(normalize_return("dyn Settings", "B"), Unsupported);
        assert_eq!(normalize_return("()", "F"), Unsupported);
    }

    #[test]
    fn conflicting_returns_absorb_ambiguously_regardless_of_order() {
        for entries in [
            vec![
                ("B::output", ReturnEvidence::Returns("P1".into())),
                ("B::output", ReturnEvidence::Returns("P2".into())),
            ],
            vec![
                ("B::output", ReturnEvidence::Returns("P2".into())),
                ("B::output", ReturnEvidence::Returns("P1".into())),
            ],
        ] {
            let ctx = ctx_of(entries);
            assert_eq!(ctx.return_of("B::output"), Some(&DeclEvidence::Ambiguous));
        }
    }

    #[test]
    fn same_return_repeat_is_idempotent() {
        let ctx = ctx_of(vec![
            ("B::output", ReturnEvidence::Returns("P".into())),
            ("B::output", ReturnEvidence::Returns("P".into())),
        ]);
        assert_eq!(
            ctx.return_of("B::output"),
            Some(&DeclEvidence::Returns("P".into()))
        );
    }

    #[test]
    fn unsupported_beside_supported_is_ambiguous() {
        let ctx = ctx_of(vec![
            ("B::output", ReturnEvidence::Returns("P".into())),
            ("B::output", ReturnEvidence::Unsupported),
        ]);
        assert_eq!(ctx.return_of("B::output"), Some(&DeclEvidence::Ambiguous));
    }

    #[test]
    fn ambiguity_is_absorbing_over_further_insertions() {
        let ctx = ctx_of(vec![
            ("B::m", ReturnEvidence::Returns("P1".into())),
            ("B::m", ReturnEvidence::Returns("P2".into())),
            ("B::m", ReturnEvidence::Returns("P1".into())),
        ]);
        assert_eq!(ctx.return_of("B::m"), Some(&DeclEvidence::Ambiguous));
    }

    #[test]
    fn anchor_counts_count_declaration_events_not_spellings() {
        let mut b = RepoContext::builder();
        b.declare_method("Builder::build", ReturnEvidence::Returns("P".into()));
        b.declare_method("Builder::build", ReturnEvidence::Returns("P".into()));
        let ctx = b.build();
        assert_eq!(ctx.anchor_count("Builder::build"), 2);
    }

    #[test]
    fn short_name_owner_resolution_is_uniqueness_gated() {
        let mut b = RepoContext::builder();
        b.declare_type("remote::Product");
        let ctx = b.build();
        assert_eq!(ctx.unique_owner("Product"), Some("remote::Product".into()));
        assert!(ctx.is_known_type("Product"));

        let mut b2 = RepoContext::builder();
        b2.declare_type("remote::Product");
        b2.declare_type("Product");
        let ctx2 = b2.build();
        assert_eq!(ctx2.unique_owner("Product"), None);
        assert_eq!(
            ctx2.canonical_owner("Product", &HashMap::new()),
            Canonical::Unknown
        );
    }

    #[test]
    fn canonical_owner_resolves_aliases_before_sets() {
        let mut b = RepoContext::builder();
        b.declare_type("remote::Product");
        let ctx = b.build();
        let mut aliases = HashMap::new();
        aliases.insert("RP".to_owned(), "crate::remote::Product".to_owned());
        assert_eq!(
            ctx.canonical_owner("RP", &aliases),
            Canonical::Qualified("remote::Product".into())
        );
    }

    #[test]
    fn fingerprint_changes_when_evidence_changes() {
        let mut b = RepoContext::builder();
        b.declare_method("B::m", ReturnEvidence::Returns("P1".into()));
        let f1 = b.build().fingerprint();

        let mut b2 = RepoContext::builder();
        b2.declare_method("B::m", ReturnEvidence::Returns("P2".into()));
        let f2 = b2.build().fingerprint();
        assert_ne!(f1, f2);

        // Adding a new key also changes the fingerprint (invalidation for
        // absent → Returns transitions).
        let mut b3 = RepoContext::builder();
        b3.declare_method("B::m", ReturnEvidence::Returns("P1".into()));
        assert_eq!(f1, b3.build().fingerprint());
        let mut b4 = RepoContext::builder();
        b4.declare_method("B::m", ReturnEvidence::Returns("P1".into()));
        b4.declare_method("B::other", ReturnEvidence::Returns("Q".into()));
        assert_ne!(f1, b4.build().fingerprint());
    }
}
