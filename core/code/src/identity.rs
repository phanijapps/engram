//! Symbol identity — the RFC-0020 Phase-2 scope-wide symbol table.
//!
//! Entity names are receiver-qualified logical names (`Foo::bar` where a
//! receiver exists, bare otherwise); repo/path/revision are disambiguators
//! carried on the candidate, never in the name. The index maps every name
//! key (qualified form and bare tail) to ALL candidates that may denote it —
//! no candidate silently overwrites another (supersedes the Phase-1
//! last-write-wins `name_index`).

use std::collections::HashMap;

/// One denotation of a name key: the entity id plus its discriminators.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolCandidate {
    pub id: String,
    /// Repo discriminator (the source's stable repository key, if known).
    pub repo: Option<String>,
    /// Path discriminator (the defining file path, if known).
    pub path: Option<String>,
}

/// Outcome of resolving a name against the symbol table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// Exactly one candidate (after disambiguation).
    Resolved(SymbolCandidate),
    /// Multiple candidates remain — the reference site must record them
    /// (the unresolved-reference ledger) rather than pick one arbitrarily.
    Ambiguous(Vec<SymbolCandidate>),
    /// No candidate for the name key.
    NotFound,
}

/// Scope-wide, multi-candidate name → candidates index.
///
/// Insertion appends: registering `Foo::bar` from two files yields two
/// candidates under the bare key `bar` and one candidate under each
/// qualified key. Resolution prefers the referring file's own candidates
/// (same document), then the same repo, then a unique survivor; anything
/// still tied is `Ambiguous`.
#[derive(Debug, Default)]
pub struct SymbolIndex {
    entries: HashMap<String, Vec<SymbolCandidate>>,
}

impl SymbolIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an entity under both its qualified name and its bare tail
    /// (when they differ). Appends — never overwrites an existing candidate.
    /// Returns the keys the candidate was registered under.
    pub fn register(&mut self, qualified_name: &str, candidate: SymbolCandidate) -> Vec<String> {
        let mut keys = vec![qualified_name.to_owned()];
        self.append(qualified_name, candidate.clone());
        if let Some(bare) = bare_tail(qualified_name) {
            self.append(bare, candidate);
            keys.push(bare.to_owned());
        }
        keys
    }

    fn append(&mut self, key: &str, candidate: SymbolCandidate) {
        let bucket = self.entries.entry(key.to_owned()).or_default();
        if !bucket.iter().any(|c| c.id == candidate.id) {
            bucket.push(candidate);
        }
    }

    /// Resolve a reference written at `from_path` (in repo `from_repo`) to
    /// the best candidate for `name` (a bare or qualified form).
    pub fn resolve(
        &self,
        name: &str,
        from_path: Option<&str>,
        from_repo: Option<&str>,
    ) -> Resolution {
        let Some(bucket) = self.entries.get(name) else {
            return Resolution::NotFound;
        };
        if bucket.is_empty() {
            return Resolution::NotFound;
        }
        // Disambiguation ladder (RFC-0020 Phase 2): same document > same
        // repo > unique survivor > ambiguous.
        if let Some(path) = from_path {
            let same_doc: Vec<_> = bucket
                .iter()
                .filter(|c| c.path.as_deref() == Some(path))
                .cloned()
                .collect();
            if same_doc.len() == 1 {
                return Resolution::Resolved(same_doc[0].clone());
            }
        }
        if let Some(repo) = from_repo {
            let same_repo: Vec<_> = bucket
                .iter()
                .filter(|c| c.repo.as_deref() == Some(repo))
                .cloned()
                .collect();
            if same_repo.len() == 1 {
                return Resolution::Resolved(same_repo[0].clone());
            }
        }
        if bucket.len() == 1 {
            return Resolution::Resolved(bucket[0].clone());
        }
        Resolution::Ambiguous(bucket.clone())
    }

    /// Number of distinct name keys (diagnostics/tests).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Bare tail of a qualified name (`Foo::bar` → `bar`; `bar` → `None`).
pub fn bare_tail(qualified_name: &str) -> Option<&str> {
    let tail = qualified_name.rsplit("::").next()?;
    if tail.is_empty() || tail == qualified_name {
        None
    } else {
        Some(tail)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(id: &str, repo: Option<&str>, path: Option<&str>) -> SymbolCandidate {
        SymbolCandidate {
            id: id.to_owned(),
            repo: repo.map(str::to_owned),
            path: path.map(str::to_owned),
        }
    }

    #[test]
    fn register_never_overwrites_a_collision() {
        let mut index = SymbolIndex::new();
        index.register("parse", cand("e1", Some("repo-a"), Some("src/a.rs")));
        index.register("parse", cand("e2", Some("repo-a"), Some("src/b.rs")));
        // Both candidates survive under the bare key — no last-write-wins.
        match index.resolve("parse", None, None) {
            Resolution::Ambiguous(cands) => {
                assert_eq!(cands.len(), 2);
                let ids: Vec<_> = cands.iter().map(|c| c.id.as_str()).collect();
                assert!(ids.contains(&"e1"));
                assert!(ids.contains(&"e2"));
            }
            other => panic!("expected ambiguous, got {other:?}"),
        }
    }

    #[test]
    fn same_document_outranks_everything() {
        let mut index = SymbolIndex::new();
        index.register("parse", cand("e1", Some("repo-a"), Some("src/a.rs")));
        index.register("parse", cand("e2", Some("repo-a"), Some("src/b.rs")));
        // A reference from src/b.rs resolves to its own e2 even though e1
        // was registered first.
        assert_eq!(
            index.resolve("parse", Some("src/b.rs"), Some("repo-a")),
            Resolution::Resolved(cand("e2", Some("repo-a"), Some("src/b.rs")))
        );
    }

    #[test]
    fn same_repo_outranks_cross_repo() {
        let mut index = SymbolIndex::new();
        index.register("parse", cand("e1", Some("repo-a"), Some("src/a.rs")));
        index.register("parse", cand("e2", Some("repo-b"), Some("src/a.rs")));
        // Same path in both repos: from repo-b, the same-repo candidate wins.
        assert_eq!(
            index.resolve("parse", Some("src/a.rs"), Some("repo-b")),
            Resolution::Resolved(cand("e2", Some("repo-b"), Some("src/a.rs")))
        );
        // No path context: two repos, both unique-per-repo — ambiguous.
        assert!(matches!(
            index.resolve("parse", None, None),
            Resolution::Ambiguous(_)
        ));
    }

    #[test]
    fn qualified_and_bare_keys_both_resolve() {
        let mut index = SymbolIndex::new();
        index.register("Config::parse", cand("e1", Some("r"), Some("src/c.rs")));
        assert_eq!(
            index.resolve("Config::parse", None, None),
            Resolution::Resolved(cand("e1", Some("r"), Some("src/c.rs")))
        );
        assert_eq!(
            index.resolve("parse", None, None),
            Resolution::Resolved(cand("e1", Some("r"), Some("src/c.rs")))
        );
    }

    #[test]
    fn unknown_name_is_not_found() {
        let index = SymbolIndex::new();
        assert_eq!(index.resolve("nope", None, None), Resolution::NotFound);
    }

    #[test]
    fn bare_tail_splits_on_last_separator() {
        assert_eq!(bare_tail("Foo::bar"), Some("bar"));
        assert_eq!(bare_tail("A::B::c"), Some("c"));
        assert_eq!(bare_tail("bare"), None);
        assert_eq!(bare_tail("Foo::"), None);
    }

    #[test]
    fn duplicate_registration_is_idempotent() {
        let mut index = SymbolIndex::new();
        index.register("parse", cand("e1", None, None));
        index.register("parse", cand("e1", None, None));
        assert_eq!(
            index.resolve("parse", None, None),
            Resolution::Resolved(cand("e1", None, None))
        );
    }
}

// The qualified-name formation helpers below keep this module the single
// home of Phase-2 name rules; the chunker anchors and the extractor both
// delegate here.

/// Form the entity name for a declaration: receiver-qualified when a
/// receiver chain exists, bare otherwise (RFC-0020 Phase 2).
pub fn qualified_name(receivers: &[String], name: &str) -> String {
    if receivers.is_empty() {
        name.to_owned()
    } else {
        format!("{}::{}", receivers.join("::"), name)
    }
}

#[cfg(test)]
mod formation_tests {
    use super::*;

    #[test]
    fn qualified_name_joins_receivers() {
        assert_eq!(qualified_name(&["Impl".to_owned()], "parse"), "Impl::parse");
        assert_eq!(
            qualified_name(&["A".to_owned(), "B".to_owned()], "c"),
            "A::B::c"
        );
        assert_eq!(qualified_name(&[], "parse"), "parse");
    }
}
