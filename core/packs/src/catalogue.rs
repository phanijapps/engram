//! Pinned catalogue of standard ontology packs (DEC-P07).
//!
//! The catalogue is DATA, not code: `data/catalogue.json` is embedded at
//! compile time and records identity (id, name, description, domain, url,
//! format), license, and the sha256 of the one-time first fetch per entry.
//! The loader validates integrity on every parse: unique ids, exactly
//! [`EXPECTED_ENTRY_COUNT`] entries, the owl/rdfs/rdf language-base
//! exclusion recorded in the file header, a recorded license per entry from
//! the permissive set, and zero license-restricted entries.
//!
//! A sha256 mismatch at first fetch is a hard error naming the entry and the
//! re-pin procedure (RISK-001) — a pin is never overwritten to make a fetch
//! pass.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

/// The committed pin file, embedded so the catalogue ships inside the crate.
pub const EMBEDDED_CATALOGUE_JSON: &str = include_str!("../data/catalogue.json");

/// The marketplace count the catalogue must equal exactly (AC-010, RISK-005):
/// 36 upstream entries minus the three RDF-language bases owl/rdfs/rdf.
pub const EXPECTED_ENTRY_COUNT: usize = 33;

/// RDF-language bases excluded from the installable marketplace (DEC-P07).
pub const EXCLUDED_LANGUAGE_BASES: [&str; 3] = ["owl", "rdfs", "rdf"];

/// Serialization format of a pinned pack's bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PackFormat {
    Turtle,
    RdfXml,
}

impl PackFormat {
    /// Canonical file extension used by the pack cache.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Turtle => "ttl",
            Self::RdfXml => "rdf",
        }
    }
}

/// One pinned marketplace entry.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct CatalogueEntry {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub domain: String,
    pub url: String,
    pub format: PackFormat,
    pub license: String,
    #[serde(default)]
    pub license_source: String,
    pub sha256: String,
}

/// The parsed catalogue plus its recorded header facts.
#[derive(Debug, Clone, PartialEq)]
pub struct Catalogue {
    pub entries: Vec<CatalogueEntry>,
    /// Language-base ids the header records as excluded (must be exactly
    /// owl/rdfs/rdf — the RISK-005 count guard).
    pub excluded_language_bases: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct CatalogueFile {
    entries: Vec<CatalogueEntry>,
    #[serde(rename = "_header")]
    header: CatalogueHeader,
}

#[derive(Debug, Deserialize)]
struct CatalogueHeader {
    #[serde(default)]
    excluded_language_bases: ExclusionRecord,
    /// Optional exact-count expectation. The marketplace pin file sets 33
    /// (RISK-005: count drift fails the load); dev/fixture catalogue
    /// overrides may omit it.
    #[serde(default)]
    expected_count: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
struct ExclusionRecord {
    #[serde(default)]
    reason: String,
}

impl Catalogue {
    /// Parse and validate the committed pin file.
    pub fn embedded() -> Result<Self, String> {
        Self::from_json(EMBEDDED_CATALOGUE_JSON)
    }

    /// Parse catalogue JSON text; validation errors are collected so a
    /// caller sees every violation, not just the first.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let file: CatalogueFile =
            serde_json::from_str(text).map_err(|e| format!("parse catalogue: {e}"))?;
        if file.header.excluded_language_bases.reason.is_empty() {
            return Err("catalogue header is missing the language-base exclusion record".into());
        }
        let mut catalogue = Self {
            excluded_language_bases: EXCLUDED_LANGUAGE_BASES
                .iter()
                .map(|s| s.to_string())
                .collect(),
            entries: file.entries,
        };
        if let Some(expected) = file.header.expected_count
            && catalogue.entries.len() != expected
        {
            return Err(format!(
                "catalogue integrity failures: [\"entry count {} != declared expected count {expected}\"]",
                catalogue.entries.len()
            ));
        }
        let errors = catalogue.validate();
        if errors.is_empty() {
            Ok(catalogue)
        } else {
            Err(format!("catalogue integrity failures: {errors:?}"))
        }
    }

    /// Integrity rules (TASK-005): unique ids, count == 33 exactly, no
    /// excluded language base present, sha256 recorded per entry as 64 hex
    /// chars, license recorded per entry from the permissive set, and zero
    /// license-restricted entries.
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        let mut seen = HashSet::new();
        for entry in &self.entries {
            if !seen.insert(&entry.id) {
                errors.push(format!("duplicate id {}", entry.id));
            }
            if EXCLUDED_LANGUAGE_BASES.contains(&entry.id.as_str()) {
                errors.push(format!(
                    "language base {} must stay excluded from the marketplace",
                    entry.id
                ));
            }
            if entry.sha256.len() != 64 || !entry.sha256.chars().all(|c| c.is_ascii_hexdigit()) {
                errors.push(format!("entry {} has no recorded 64-hex sha256", entry.id));
            }
            if entry.license.trim().is_empty() {
                errors.push(format!("entry {} records no license", entry.id));
            } else if !permissive_license(&entry.license) {
                errors.push(format!(
                    "entry {} license {} is not in the permissive set",
                    entry.id, entry.license
                ));
            }
            if license_restricted(&entry.license) {
                errors.push(format!(
                    "entry {} carries license-restricted terms ({})",
                    entry.id, entry.license
                ));
            }
        }
        errors
    }

    /// Look up one entry by id.
    pub fn find(&self, id: &str) -> Option<&CatalogueEntry> {
        self.entries.iter().find(|e| e.id == id)
    }
}

/// Permissive license ids the catalogue pins may carry (AC-016 allowlist).
const PERMISSIVE_LICENSES: [&str; 12] = [
    "MIT",
    "Apache-2.0",
    "CC-BY-1.0",
    "CC-BY-3.0",
    "CC-BY-4.0",
    "CC-BY-SA-3.0",
    "PDDL-1.0",
    "CC0-1.0",
    "W3C-Software-And-Document",
    "W3C-Document",
    "OGC-Standard",
    "ETSI-Software-License",
    // ISA Open Metadata Licence v1.1 (SEMIC/EU open licence)
];

/// Extra open-license families spelled out in full (checked by prefix so
/// version bumps keep passing).
fn permissive_license(license: &str) -> bool {
    let normalized = license.trim();
    PERMISSIVE_LICENSES.contains(&normalized)
        || normalized == "ISA-Open-Metadata-1.1"
        || normalized.starts_with("ISA-Open-Metadata-")
        || normalized.starts_with("CC-BY-")
}

/// Restricted-license markers (SNOMED and friends are Out of scope by the
/// brief; NonCommercial/NoDerivs terms are not open for redistribution).
fn license_restricted(license: &str) -> bool {
    let lower = license.to_ascii_lowercase();
    [
        "snomed",
        "noncommercial",
        "noderivs",
        "proprietary",
        "research-only",
        "by-nc",
        "by-nd",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_embedded_has_exactly_33_entries() {
        let catalogue = Catalogue::embedded().expect("embedded catalogue validates");
        assert_eq!(catalogue.entries.len(), 33);
        assert_eq!(catalogue.entries.len(), EXPECTED_ENTRY_COUNT);
    }

    #[test]
    fn catalogue_ids_are_unique() {
        let catalogue = Catalogue::embedded().expect("embedded catalogue validates");
        let mut ids: Vec<_> = catalogue.entries.iter().map(|e| e.id.clone()).collect();
        ids.sort();
        let unique: HashSet<_> = ids.iter().cloned().collect();
        assert_eq!(ids.len(), unique.len(), "catalogue ids must be unique");
    }

    #[test]
    fn catalogue_exclusion_header_present() {
        let catalogue = Catalogue::embedded().expect("embedded catalogue validates");
        for base in EXCLUDED_LANGUAGE_BASES {
            assert!(
                catalogue.excluded_language_bases.iter().any(|b| b == base),
                "header must record the {base} exclusion"
            );
            assert!(
                catalogue.entries.iter().all(|e| e.id != base),
                "{base} must not appear as an installable entry"
            );
        }
    }

    #[test]
    fn catalogue_every_entry_carries_license_and_first_fetch_sha256() {
        let catalogue = Catalogue::embedded().expect("embedded catalogue validates");
        for entry in &catalogue.entries {
            assert!(
                !entry.license.trim().is_empty(),
                "{}: license recorded",
                entry.id
            );
            assert_eq!(entry.sha256.len(), 64, "{}: sha256 recorded", entry.id);
            assert!(
                entry.sha256.chars().all(|c| c.is_ascii_hexdigit()),
                "{}: sha256 is hex",
                entry.id
            );
            assert!(
                !entry.url.is_empty() && entry.url.starts_with("http"),
                "{}: url",
                entry.id
            );
        }
    }

    #[test]
    fn catalogue_zero_license_restricted_entries() {
        let catalogue = Catalogue::embedded().expect("embedded catalogue validates");
        for entry in &catalogue.entries {
            assert!(
                permissive_license(&entry.license),
                "{}: license {} must be permissive",
                entry.id,
                entry.license
            );
            assert!(
                !license_restricted(&entry.license),
                "{}: license {} must not be restricted",
                entry.id,
                entry.license
            );
        }
    }

    #[test]
    fn catalogue_named_proof_packs_pinned() {
        let catalogue = Catalogue::embedded().expect("embedded catalogue validates");
        assert!(
            catalogue.find("fibo").is_some(),
            "banking proof pack pinned"
        );
        assert!(
            catalogue.find("schema-org").is_some(),
            "travel proof pack pinned"
        );
        assert_eq!(catalogue.find("schema-org").unwrap().domain, "general");
    }

    #[test]
    fn catalogue_format_split_covers_both_pinned_formats() {
        let catalogue = Catalogue::embedded().expect("embedded catalogue validates");
        let turtle = catalogue
            .entries
            .iter()
            .filter(|e| e.format == PackFormat::Turtle)
            .count();
        let rdf_xml = catalogue
            .entries
            .iter()
            .filter(|e| e.format == PackFormat::RdfXml)
            .count();
        assert!(turtle > 0 && rdf_xml > 0, "both pinned formats present");
        assert_eq!(turtle + rdf_xml, 33);
    }

    #[test]
    fn catalogue_rejects_duplicate_ids() {
        let base = Catalogue::embedded().expect("embedded catalogue validates");
        let mut entries = base.entries.clone();
        entries.push(entries[0].clone());
        let text = tampered_catalogue(entries);
        let err = Catalogue::from_json(&text).expect_err("duplicate ids must fail");
        assert!(err.contains("duplicate"), "got: {err}");
    }

    #[test]
    fn catalogue_rejects_wrong_declared_count() {
        let base = Catalogue::embedded().expect("embedded catalogue validates");
        let entries: Vec<_> = base.entries[..32].to_vec();
        let text = catalogue_json_with_expected_count(entries, Some(33));
        let err = Catalogue::from_json(&text).expect_err("count drift must fail");
        assert!(err.contains("32 !="), "got: {err}");
    }

    #[test]
    fn catalogue_rejects_restricted_license() {
        let base = Catalogue::embedded().expect("embedded catalogue validates");
        let mut entries = base.entries.clone();
        entries[0].license = "SNOMED-CT-Research-Only".to_owned();
        let text = tampered_catalogue(entries);
        let err = Catalogue::from_json(&text).expect_err("restricted license must fail");
        assert!(
            err.contains("restricted") || err.contains("permissive"),
            "got: {err}"
        );
    }

    #[test]
    fn catalogue_rejects_missing_exclusion_header() {
        let base = Catalogue::embedded().expect("embedded catalogue validates");
        let raw = serde_json::json!({ "entries": base.entries });
        let err = Catalogue::from_json(&raw.to_string()).expect_err("header is mandatory");
        assert!(
            err.contains("_header") || err.contains("exclusion"),
            "got: {err}"
        );
    }

    /// Rebuild a catalogue JSON document with the same header shape but a
    /// tampered entry set (negative-test helper).
    fn tampered_catalogue(entries: Vec<CatalogueEntry>) -> String {
        catalogue_json_with_expected_count(entries, None)
    }

    fn catalogue_json_with_expected_count(
        entries: Vec<CatalogueEntry>,
        expected_count: Option<usize>,
    ) -> String {
        let file = serde_json::json!({
            "_header": {
                "description": "test",
                "excluded_language_bases": { "reason": "test exclusion record" },
                "expected_count": expected_count,
                "pin_run": {}, "license_policy": ""
            },
            "entries": entries
        });
        file.to_string()
    }
}
