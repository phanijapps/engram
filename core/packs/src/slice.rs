//! Deterministic slice and widening rules (DEC-P05).
//!
//! Slice = top-K terms ranked by (direct + transitive descendant count
//! desc, uri asc) with K = [`DEFAULT_CLASS_CAP`] classes and
//! [`DEFAULT_PREDICATE_CAP`] predicates. `--slice-size <n>|all` widens by
//! regenerating under a larger K from the same cached pack bytes — offline
//! and deterministic. Generated output embeds no timestamps, so two runs
//! over identical input are byte-identical (NFR-004, AC-005).
//!
//! Coverage reduction is ALWAYS reported: the selection carries
//! sliced-vs-available counts so an install can never silently drop
//! vocabulary (REQ-008).

use std::collections::BTreeMap;

use engram_domain::{OntologyClass, OntologyProperty};
use serde::Serialize;

/// Default cap for classes in the launch slice (DEC-P05).
pub const DEFAULT_CLASS_CAP: usize = 150;

/// Default cap for predicates in the launch slice (DEC-P05).
pub const DEFAULT_PREDICATE_CAP: usize = 40;

/// The `--slice-size` input surface: the default caps, an explicit number,
/// or `all` (widening to the full pack).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SliceSize {
    Default,
    N(usize),
    All,
}

impl SliceSize {
    /// Parse the `--slice-size` flag value (`150`, `all`, absent).
    pub fn parse(value: Option<&str>) -> Result<Self, String> {
        match value {
            None => Ok(Self::Default),
            Some("all") => Ok(Self::All),
            Some(raw) => raw
                .parse::<usize>()
                .map(|n| Self::N(n.max(1)))
                .map_err(|_| {
                    format!("--slice-size expects a positive number or 'all', got {raw:?}")
                }),
        }
    }
}

/// The applied slice rule (caps resolved from a [`SliceSize`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SliceRule {
    pub class_cap: usize,
    pub predicate_cap: usize,
}

impl Default for SliceRule {
    fn default() -> Self {
        Self {
            class_cap: DEFAULT_CLASS_CAP,
            predicate_cap: DEFAULT_PREDICATE_CAP,
        }
    }
}

impl SliceRule {
    pub fn from_size(size: SliceSize) -> Self {
        match size {
            SliceSize::Default => Self::default(),
            SliceSize::N(n) => Self {
                class_cap: n,
                predicate_cap: n,
            },
            // `all` keeps the ranking order but caps nothing.
            SliceSize::All => Self {
                class_cap: usize::MAX,
                predicate_cap: usize::MAX,
            },
        }
    }
}

/// The selected slice plus the sliced-vs-available counts that install
/// output MUST report (REQ-008, AC-012).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SliceSelection {
    pub pack_id: String,
    /// Class labels in ranked order (deduplicated, capped).
    pub class_labels: Vec<String>,
    /// Predicate labels in ranked order (deduplicated, capped).
    pub predicate_labels: Vec<String>,
    pub sliced_classes: usize,
    pub available_classes: usize,
    pub sliced_predicates: usize,
    pub available_predicates: usize,
}

impl SliceSelection {
    /// The one-line sliced-vs-available report for install/widen output.
    pub fn counts_line(&self) -> String {
        format!(
            "classes {}/{} available, predicates {}/{} available",
            self.sliced_classes,
            self.available_classes,
            self.sliced_predicates,
            self.available_predicates
        )
    }

    /// Deterministic JSON rendering (no timestamps; stable ordering).
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("slice selection serializes")
    }
}

impl SliceRule {
    /// Select the launch slice from a converted pack's structural records.
    ///
    /// Class ranking: (direct + transitive descendant count desc, uri asc).
    /// Predicate ranking: (uri asc) — the deterministic tie-break the plan
    /// names for classes, applied to predicates so widening order is stable.
    pub fn select(
        &self,
        pack_id: &str,
        classes: &[OntologyClass],
        properties: &[OntologyProperty],
    ) -> SliceSelection {
        let descendants = descendant_counts(classes);

        let mut ranked: Vec<&OntologyClass> = classes.iter().collect();
        ranked.sort_by(|a, b| {
            descendants
                .get(b.uri.as_str())
                .copied()
                .unwrap_or(0)
                .cmp(&descendants.get(a.uri.as_str()).copied().unwrap_or(0))
                .then_with(|| a.uri.cmp(&b.uri))
        });
        let class_labels = dedupe_labels(ranked.iter().map(|c| c.label.as_str()))
            .take(self.class_cap)
            .collect();

        let mut ranked_props: Vec<&OntologyProperty> = properties.iter().collect();
        ranked_props.sort_by(|a, b| a.uri.cmp(&b.uri));
        let predicate_labels = dedupe_labels(ranked_props.iter().map(|p| p.label.as_str()))
            .take(self.predicate_cap)
            .collect();

        SliceSelection {
            pack_id: pack_id.to_owned(),
            class_labels,
            predicate_labels,
            sliced_classes: ranked
                .iter()
                .map(|c| c.label.as_str())
                .collect::<Vec<_>>()
                .len()
                .min(self.class_cap),
            available_classes: classes.len(),
            sliced_predicates: properties.len().min(self.predicate_cap),
            available_predicates: properties.len(),
        }
    }
}

/// Direct + transitive descendant count per class URI.
fn descendant_counts(classes: &[OntologyClass]) -> BTreeMap<String, usize> {
    let id_to_uri: BTreeMap<&str, &str> = classes
        .iter()
        .map(|c| (c.id.as_str(), c.uri.as_str()))
        .collect();
    // parent URI -> child URIs
    let mut children: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for class in classes {
        for parent_id in &class.parent_class_ids {
            if let Some(parent_uri) = id_to_uri.get(parent_id.as_str()) {
                children
                    .entry(parent_uri)
                    .or_default()
                    .push(class.uri.as_str());
            }
        }
    }
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for class in classes {
        // Iterative DFS collecting distinct descendant URIs.
        let mut seen: Vec<&str> = Vec::new();
        let mut stack: Vec<&str> = children
            .get(class.uri.as_str())
            .cloned()
            .unwrap_or_default();
        while let Some(uri) = stack.pop() {
            if seen.contains(&uri) {
                continue;
            }
            seen.push(uri);
            if let Some(grandchildren) = children.get(uri) {
                stack.extend(grandchildren.iter().copied());
            }
        }
        counts.insert(class.uri.clone(), seen.len());
    }
    counts
}

/// Deduplicate labels preserving first occurrence (ranked order wins).
fn dedupe_labels<'a>(labels: impl Iterator<Item = &'a str>) -> impl Iterator<Item = String> {
    let mut seen: Vec<String> = Vec::new();
    labels
        .filter(move |label| {
            if seen.iter().any(|s| s == label) {
                false
            } else {
                seen.push((*label).to_owned());
                true
            }
        })
        .map(|label| label.to_owned())
        .collect::<Vec<_>>()
        .into_iter()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;

    #[test]
    fn slice_default_caps_enforced() {
        let classes = synthetic_classes(300);
        let properties = synthetic_properties(120);
        let selection = SliceRule::default().select("synthetic", &classes, &properties);
        assert_eq!(selection.sliced_classes, DEFAULT_CLASS_CAP.min(300));
        assert_eq!(selection.class_labels.len(), DEFAULT_CLASS_CAP);
        assert_eq!(selection.predicate_labels.len(), DEFAULT_PREDICATE_CAP);
        assert_eq!(selection.available_classes, 300);
        assert_eq!(selection.available_predicates, 120);
    }

    #[test]
    fn slice_two_runs_are_byte_identical() {
        let classes = synthetic_classes(220);
        let properties = synthetic_properties(80);
        let a = SliceRule::default()
            .select("synthetic", &classes, &properties)
            .to_json();
        let b = SliceRule::default()
            .select("synthetic", &classes, &properties)
            .to_json();
        assert_eq!(a, b, "two slices of identical input are byte-identical");
        assert!(!a.contains("created"), "no timestamps embedded");
        assert!(!a.contains("time"), "no time fields embedded");
    }

    #[test]
    fn slice_ranking_prefers_rooted_branchy_classes_then_uri_order() {
        // The synthetic chain is class-i subClassOf class-(i+1), so class-3
        // roots the whole chain (3 descendants) and must rank first; equal-
        // descendant classes would break ties by uri asc.
        let classes = synthetic_classes(4);
        let selection = SliceRule::default().select("synthetic", &classes, &[]);
        assert_eq!(
            selection.class_labels[0], "class-3",
            "most descendants first"
        );
        assert_eq!(selection.class_labels[1], "class-2", "next-most second");
    }

    #[test]
    fn slice_widen_all_recovers_full_pack() {
        let classes = synthetic_classes(300);
        let properties = synthetic_properties(120);
        let widened =
            SliceRule::from_size(SliceSize::All).select("synthetic", &classes, &properties);
        assert_eq!(widened.sliced_classes, 300, "all keeps every class");
        assert_eq!(widened.class_labels.len(), 300);
        assert_eq!(widened.predicate_labels.len(), 120);
        assert_eq!(widened.sliced_predicates, 120);
    }

    #[test]
    fn slice_explicit_size_overrides_caps() {
        let classes = synthetic_classes(50);
        let properties = synthetic_properties(50);
        let rule = SliceRule::from_size(SliceSize::N(10));
        let selection = rule.select("synthetic", &classes, &properties);
        assert_eq!(selection.class_labels.len(), 10);
        assert_eq!(selection.predicate_labels.len(), 10);
    }

    #[test]
    fn slice_counts_line_reports_sliced_vs_available() {
        let classes = synthetic_classes(200);
        let properties = synthetic_properties(60);
        let selection = SliceRule::default().select("synthetic", &classes, &properties);
        let line = selection.counts_line();
        assert_eq!(
            line,
            "classes 150/200 available, predicates 40/60 available"
        );
    }

    #[test]
    fn slice_size_parsing_rejects_garbage() {
        assert_eq!(SliceSize::parse(None), Ok(SliceSize::Default));
        assert_eq!(SliceSize::parse(Some("all")), Ok(SliceSize::All));
        assert_eq!(SliceSize::parse(Some("25")), Ok(SliceSize::N(25)));
        assert!(SliceSize::parse(Some("many")).is_err());
        assert!(SliceSize::parse(Some("-3")).is_err());
    }
}
