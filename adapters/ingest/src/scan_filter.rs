//! Tunable scan filters: the cross-document concept-link gate and the file
//! denylist.
//!
//! Historically both were hardcoded `const` arrays (`GENERIC` in `scanner.rs`,
//! `DENY_DIRS`/`DENY_FILE_EXT` in `classifier.rs`), so an enterprise that wanted
//! to block a project-specific generic term or skip a project-specific directory
//! had to fork the scanner. This module externalizes them behind a single
//! [`ScanFilter`] value that a host (e.g. `engram-mcp`) builds from an optional
//! JSON config and threads through [`crate::ScanOptions`].
//!
//! `engram-ingest` never reads a config file from disk — it receives a ready
//! [`ScanFilter`]. [`ScanFilter::builtin`] / [`ScanFilter::default`] reproduce
//! the prior hardcoded behavior exactly (the regression net).

use std::collections::HashSet;

use crate::classifier::{DENY_DIRS, DENY_FILE_EXT};

/// Generic concept names that are too common to cross-document-link. Single
/// source of truth (previously a `const` local in `scanner.rs`).
const GENERIC_CONCEPTS: &[&str] = &[
    "authentication",
    "authorization",
    "configuration",
    "documentation",
    "implementation",
    "initialization",
    "integration",
    "management",
    "processing",
    "connection",
    "database",
    "controller",
    "middleware",
    "application",
    "environment",
    "repository",
    "component",
    "interface",
    "structure",
    "parameter",
    "attribute",
    "operation",
    "function",
    "response",
    "request",
    "service",
    "handler",
    "config",
    "module",
    "entity",
    "system",
    "server",
    "client",
    "router",
    "engine",
    "factory",
    "builder",
    "reader",
    "writer",
    "parser",
    "loader",
    "runner",
    "worker",
    "manager",
    "provider",
    "listener",
    "observer",
    "visitor",
    "strategy",
    "adapter",
    "wrapper",
    "proxy",
    "filter",
    "validator",
    "converter",
    "serializer",
    "deserializer",
    "executor",
    "scheduler",
    "dispatcher",
    "resolver",
    "formatter",
    "iterator",
    "generator",
];

/// Minimum concept-name length (in bytes) for cross-document linking. Names at
/// or below this length are too ambiguous at enterprise scale.
const DEFAULT_MIN_CONCEPT_NAME_LEN: usize = 8;

/// The ready-to-use scan filter: merged builtin + user overrides, held as
/// `HashSet`s for O(1) decisions. Built once per scan and threaded through
/// [`crate::ScanOptions`].
///
/// Construct via [`ScanFilter::builtin`] (prior behavior) or
/// [`ScanFilter::merge`] (builtin + a loaded [`ScanFilterConfig`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanFilter {
    min_concept_name_len: usize,
    blocklist: HashSet<String>,
    allowlist: HashSet<String>,
    deny_dirs: HashSet<String>,
    deny_exts: HashSet<String>,
}

impl Default for ScanFilter {
    fn default() -> Self {
        Self::builtin()
    }
}

impl ScanFilter {
    /// The prior hardcoded behavior — no user overrides. Every existing
    /// `should_link_concept` / `is_denylisted` outcome is preserved.
    pub fn builtin() -> Self {
        Self {
            min_concept_name_len: DEFAULT_MIN_CONCEPT_NAME_LEN,
            blocklist: GENERIC_CONCEPTS.iter().map(|s| (*s).to_owned()).collect(),
            allowlist: HashSet::new(),
            deny_dirs: DENY_DIRS.iter().map(|s| (*s).to_owned()).collect(),
            deny_exts: DENY_FILE_EXT.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    /// Builtin defaults merged with a loaded config. Config values are
    /// *additive* for the list-shaped fields (extra entries join the built-in
    /// sets) and *overriding* for `min_name_length` (a present value replaces
    /// the default). The allowlist is checked first and wins over both the
    /// blocklist and the length threshold.
    pub fn merge(config: &ScanFilterConfig) -> Self {
        let mut filter = Self::builtin();
        if let Some(min) = config.concepts.min_name_length {
            filter.min_concept_name_len = min;
        }
        for term in &config.concepts.blocklist {
            filter.blocklist.insert(term.to_lowercase());
        }
        for term in &config.concepts.allowlist {
            filter.allowlist.insert(term.to_lowercase());
        }
        for dir in &config.deny.dirs {
            filter.deny_dirs.insert(dir.clone());
        }
        for ext in &config.deny.extensions {
            filter.deny_exts.insert(ext.to_lowercase());
        }
        filter
    }

    /// `true` if a concept name is specific enough to create a cross-document
    /// `mentions` edge. Allowlist overrides everything; otherwise the name must
    /// exceed the length threshold and not be a generic/blocked term.
    pub fn should_link_concept(&self, name: &str) -> bool {
        let lower = name.to_lowercase();
        if self.allowlist.contains(&lower) {
            return true;
        }
        if name.len() <= self.min_concept_name_len {
            return false;
        }
        !self.blocklist.contains(&lower)
    }

    /// `true` if any path segment is a denied dir, or the file suffix is a
    /// denied extension.
    ///
    /// `deny.dirs` matches path segments **case-sensitively** (matching the
    /// `ignore` crate's directory semantics); `deny.extensions` is
    /// **case-insensitive**.
    pub fn is_denylisted(&self, rel_path: &str) -> bool {
        let segs: Vec<&str> = rel_path.split(['/', '\\']).collect();
        if segs.iter().any(|s| self.deny_dirs.contains(*s)) {
            return true;
        }
        let base = segs.last().copied().unwrap_or("");
        let ext = match base.rsplit_once('.') {
            Some((_, e)) => e.to_lowercase(),
            None => String::new(),
        };
        !ext.is_empty() && self.deny_exts.contains(&ext)
    }
}

/// User-facing JSON config shape. Every field is optional; an empty/default
/// config yields [`ScanFilter::builtin`].
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ScanFilterConfig {
    #[serde(default)]
    pub concepts: ConceptFilterConfig,
    #[serde(default)]
    pub deny: DenyFilterConfig,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ConceptFilterConfig {
    #[serde(default)]
    pub min_name_length: Option<usize>,
    #[serde(default)]
    pub blocklist: Vec<String>,
    #[serde(default)]
    pub allowlist: Vec<String>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct DenyFilterConfig {
    #[serde(default)]
    pub dirs: Vec<String>,
    #[serde(default)]
    pub extensions: Vec<String>,
}

impl ScanFilterConfig {
    /// Parse a JSON config string. Malformed JSON → `Err` (the host decides
    /// whether to soft-fail to builtin).
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- builtin == prior hardcoded behavior (regression) ---

    #[test]
    fn default_rejects_short_and_generic() {
        let f = ScanFilter::builtin();
        // short
        assert!(!f.should_link_concept("Config"));
        assert!(!f.should_link_concept("Handler"));
        // generic (long) — case-insensitive
        assert!(!f.should_link_concept("authentication"));
        assert!(!f.should_link_concept("Authentication"));
        assert!(!f.should_link_concept("AUTHORIZATION"));
        assert!(!f.should_link_concept("configuration"));
        assert!(!f.should_link_concept("repository"));
        // specific
        assert!(f.should_link_concept("RetrievalIndex"));
        assert!(f.should_link_concept("BeliefRepository"));
    }

    #[test]
    fn default_length_boundary_is_eight() {
        let f = ScanFilter::builtin();
        // exactly 8 bytes rejected, 9+ accepted (when not generic).
        assert!(!f.should_link_concept("12345678"));
        assert!(f.should_link_concept("SpecificNm")); // 10 chars, not generic
    }

    #[test]
    fn default_denies_builtin_dirs_and_exts() {
        let f = ScanFilter::builtin();
        assert!(f.is_denylisted("src/node_modules/x.js"));
        assert!(f.is_denylisted("target/debug/lib.rs"));
        assert!(f.is_denylisted("a/b/c.db"));
        assert!(f.is_denylisted("app.log"));
        // not denied
        assert!(!f.is_denylisted("src/main.rs"));
        assert!(!f.is_denylisted("docs/readme.md"));
    }

    // --- overrides ---

    #[test]
    fn allowlist_overrides_blocklist_and_length() {
        let cfg = ScanFilterConfig {
            concepts: ConceptFilterConfig {
                allowlist: vec!["api".to_owned(), "repository".to_owned()],
                ..Default::default()
            },
            ..Default::default()
        };
        let f = ScanFilter::merge(&cfg);
        // "api" is 3 chars (<=8) — allowlist forces it.
        assert!(f.should_link_concept("api"));
        assert!(f.should_link_concept("API"));
        // "repository" is blocklisted — allowlist forces it.
        assert!(f.should_link_concept("repository"));
    }

    #[test]
    fn extra_blocklist_merges() {
        let cfg = ScanFilterConfig {
            concepts: ConceptFilterConfig {
                blocklist: vec!["KafkaConsumer".to_owned()],
                ..Default::default()
            },
            ..Default::default()
        };
        let f = ScanFilter::merge(&cfg);
        // added term now rejected (case-insensitive)…
        assert!(!f.should_link_concept("KafkaConsumer"));
        assert!(!f.should_link_concept("kafkaconsumer"));
        // …while a builtin-accepted specific name still passes.
        assert!(f.should_link_concept("RetrievalIndex"));
    }

    #[test]
    fn min_name_length_override() {
        let cfg = ScanFilterConfig {
            concepts: ConceptFilterConfig {
                min_name_length: Some(12),
                ..Default::default()
            },
            ..Default::default()
        };
        let f = ScanFilter::merge(&cfg);
        // 11 chars, specific, not generic — rejected under the raised bar.
        assert!(!f.should_link_concept("SpecificNm"));
        // long + specific — accepted.
        assert!(f.should_link_concept("RetrievalIndex"));
    }

    #[test]
    fn deny_dirs_and_exts_merge() {
        let cfg = ScanFilterConfig {
            deny: DenyFilterConfig {
                dirs: vec!["generated".to_owned(), "vendor".to_owned()],
                extensions: vec!["map".to_owned(), "SVG".to_owned()],
            },
            ..Default::default()
        };
        let f = ScanFilter::merge(&cfg);
        // custom additions…
        assert!(f.is_denylisted("generated/foo.rs"));
        assert!(f.is_denylisted("vendor/lib.ts"));
        assert!(f.is_denylisted("styles/app.css.map"));
        assert!(f.is_denylisted("logo.SVG")); // case-insensitive ext
        // builtin still applies…
        assert!(f.is_denylisted("node_modules/x.js"));
        // not denied…
        assert!(!f.is_denylisted("src/main.rs"));
    }

    // --- config serde round-trip ---

    #[test]
    fn config_json_round_trip() {
        let json = r#"{
          "concepts": {
            "min_name_length": 10,
            "blocklist": ["KafkaConsumer"],
            "allowlist": ["api"]
          },
          "deny": {
            "dirs": ["generated"],
            "extensions": ["map"]
          }
        }"#;
        let cfg = ScanFilterConfig::from_json(json).expect("parses");
        let f = ScanFilter::merge(&cfg);
        // min raised to 10 → 9-char specific name rejected.
        assert!(!f.should_link_concept("SpecificN"));
        // blocklist merge.
        assert!(!f.should_link_concept("KafkaConsumer"));
        // allowlist override (short).
        assert!(f.should_link_concept("api"));
        // deny merge.
        assert!(f.is_denylisted("generated/x.rs"));
        assert!(f.is_denylisted("a/b.map"));
    }

    #[test]
    fn empty_config_yields_builtin() {
        let cfg = ScanFilterConfig::from_json("{}").expect("parses");
        // Exact structural equality — a future field that fails to default to
        // the builtin value would break this, not just a sampled name.
        assert_eq!(ScanFilter::merge(&cfg), ScanFilter::builtin());
    }

    #[test]
    fn malformed_json_errors() {
        assert!(ScanFilterConfig::from_json("{ not json").is_err());
    }
}
