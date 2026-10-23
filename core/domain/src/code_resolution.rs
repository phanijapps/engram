//! Code-edge resolution evidence (wicked-estate adoption, slice 1).
//!
//! A code relationship's endpoints are established by different mechanisms:
//! some are direct structural facts from one document's AST, some are bound
//! later through the scope-wide symbol table, and some are heuristic guesses
//! (receiver capitalization, text co-occurrence). Until now the only signal
//! was a numeric `confidence`; this module gives every code edge a typed
//! `ResolutionTier` plus a stable `resolved_by` producer identifier so a
//! heuristic is never presented as a fact.
//!
//! Tiers describe the *binding mechanism*, not extraction effort: observing
//! a call spelling in the AST (`parsed`) is not proof that an inferred
//! destination entity is correct — the `resolved` flag on query evidence and
//! the bound entity id carry that separately.

use serde::{Deserialize, Serialize};

/// How a code edge's endpoint binding was established. Ordered from
/// strongest to weakest; defaults (`default_confidence`) are ordering and
/// diagnostic defaults, NOT calibrated probabilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionTier {
    /// Direct syntax/structural fact from one document's AST: the endpoint
    /// spelling (and, for containment/imports, both endpoint identities) is
    /// exactly what the parser observed.
    Parsed,
    /// Deterministic name-table binding through the scope-wide symbol index
    /// (as-written qualified form, exact-name rung, same-document,
    /// same-repo, or unique survivor). Deterministic — but not compiler
    /// proof.
    ScopedName,
    /// Inferential binding: receiver capitalization, text co-occurrence,
    /// framework-pattern synthesis. Never silently upgradeable.
    Heuristic,
    /// Pre-evidence code edge: persisted before this contract existed and
    /// lacking a recognizable producer signal. Conservative floor.
    LegacyUnknown,
}

impl ResolutionTier {
    /// The closed set, strongest first.
    pub const ALL: [ResolutionTier; 4] = [
        ResolutionTier::Parsed,
        ResolutionTier::ScopedName,
        ResolutionTier::Heuristic,
        ResolutionTier::LegacyUnknown,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ResolutionTier::Parsed => "parsed",
            ResolutionTier::ScopedName => "scoped_name",
            ResolutionTier::Heuristic => "heuristic",
            ResolutionTier::LegacyUnknown => "legacy_unknown",
        }
    }

    /// Ordering/diagnostic default confidence for edges of this tier when no
    /// explicit relationship confidence exists. Not a calibrated probability.
    pub fn default_confidence(self) -> f32 {
        match self {
            ResolutionTier::Parsed => 1.0,
            ResolutionTier::ScopedName => 0.8,
            ResolutionTier::Heuristic | ResolutionTier::LegacyUnknown => 0.5,
        }
    }
}

impl std::fmt::Display for ResolutionTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for ResolutionTier {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ResolutionTier::ALL
            .iter()
            .copied()
            .find(|t| t.as_str() == s)
            .ok_or(())
    }
}

/// Typed resolution evidence stamped on code relationships
/// (`KnowledgeRelationship.code_resolution`, optional; absent on non-code
/// relationships).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeEdgeResolution {
    pub tier: ResolutionTier,
    /// Stable, versioned producer identifier (e.g. `ast-calls-v1`,
    /// `scope-name-v1`, `receiver-capitalization-v1`, `cooccurrence-v1`,
    /// `orphan-sweep-v1`, `legacy-code-edge-v1`). Non-empty.
    pub resolved_by: String,
}

impl CodeEdgeResolution {
    /// Producer id for edges persisted before evidence existed and whose
    /// historical producer cannot be recognized.
    pub const LEGACY_PRODUCER: &'static str = "legacy-code-edge-v1";

    pub fn new(tier: ResolutionTier, resolved_by: impl Into<String>) -> Self {
        Self {
            tier,
            resolved_by: resolved_by.into(),
        }
    }

    /// Contract check: producer id must be a non-empty, trimmed, stable
    /// identifier (letters, digits, `-`, `_`, `.`; at least one `-v<N>`
    /// style suffix is encouraged but not enforced).
    pub fn is_valid(&self) -> bool {
        !self.resolved_by.trim().is_empty()
            && self
                .resolved_by
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_serde_round_trips_every_variant() {
        for tier in ResolutionTier::ALL {
            let json = serde_json::to_string(&tier).expect("serialize");
            let back: ResolutionTier = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(back, tier, "round-trip failed for {json}");
        }
    }

    #[test]
    fn tier_str_round_trip_is_closed() {
        for tier in ResolutionTier::ALL {
            assert_eq!(tier.as_str().parse::<ResolutionTier>(), Ok(tier));
        }
        assert_eq!("".parse::<ResolutionTier>(), Err(()));
        assert_eq!("compiler".parse::<ResolutionTier>(), Err(()));
        assert_eq!("PARSED".parse::<ResolutionTier>(), Err(()));
    }

    #[test]
    fn tier_ordering_is_strongest_first() {
        assert!(ResolutionTier::Parsed < ResolutionTier::ScopedName);
        assert!(ResolutionTier::ScopedName < ResolutionTier::Heuristic);
        assert!(ResolutionTier::Heuristic < ResolutionTier::LegacyUnknown);
    }

    #[test]
    fn default_confidences_are_ordered_and_finite() {
        let values: Vec<f32> = ResolutionTier::ALL.map(|t| t.default_confidence()).to_vec();
        assert!(values.windows(2).all(|w| w[0] >= w[1]));
        assert!(values.iter().all(|c| (0.0..=1.0).contains(c)));
    }

    #[test]
    fn resolution_serde_is_camel_case() {
        let ev = CodeEdgeResolution::new(ResolutionTier::ScopedName, "scope-name-v1");
        let json = serde_json::to_string(&ev).expect("serialize");
        assert!(json.contains("\"resolvedBy\""), "camelCase missing: {json}");
        let back: CodeEdgeResolution = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, ev);
    }

    #[test]
    fn producer_ids_must_be_nonempty_stable_identifiers() {
        assert!(CodeEdgeResolution::new(ResolutionTier::Parsed, "ast-calls-v1").is_valid());
        assert!(!CodeEdgeResolution::new(ResolutionTier::Parsed, "").is_valid());
        assert!(!CodeEdgeResolution::new(ResolutionTier::Parsed, "  ").is_valid());
        assert!(!CodeEdgeResolution::new(ResolutionTier::Parsed, "bad producer!").is_valid());
    }
}
