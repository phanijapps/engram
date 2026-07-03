//! Federated source assertions and their authority tiers.
//!
//! A [`SourceAssertion`] is a single subject-predicate-object claim made by an
//! external system of record. It is distinct from [`crate::MemoryAssertion`],
//! which the agent asserts from its own experience and which is embedded in a
//! `MemoryRecord`: a source assertion stands alone, links back to its source
//! (`source_record_id`/`source_uri`) rather than copying it, and carries the
//! authority and review metadata reconciliation needs.

use serde::{Deserialize, Serialize};

use crate::{AssertionId, BeliefSubject, Policy, Provenance, Scalar, Scope, Timestamp};

/// How much a source is trusted for a claim.
///
/// This is deliberately distinct from [`AssertionReviewStatus`] (how far a claim
/// has progressed through review): the two vocabularies share no serialized
/// token, so "authority" and "state" never collide. Ordering between tiers is
/// not fixed here — it is supplied by an authority policy, so profiles can
/// reorder without a type change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityTier {
    Primary,
    Secondary,
    Inferred,
}

impl Default for AuthorityTier {
    /// A source with no declared authority reproduces today's single-source
    /// behavior: it is treated as `Primary`.
    fn default() -> Self {
        AuthorityTier::Primary
    }
}

/// Promotion lifecycle state of a source assertion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssertionReviewStatus {
    Source,
    Candidate,
    Reviewed,
    Authoritative,
    Disputed,
    Deprecated,
    Rejected,
}

/// One claim asserted by an external system of record.
///
/// The claim is `(subject, predicate, object)`; the source is identified and
/// linked, never replicated. `valid_from`/`valid_until` are the claim's
/// event/application-time interval; `asserted_at` is when the source stated it
/// (knowledge/transaction time).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceAssertion {
    pub id: AssertionId,
    pub scope: Scope,
    pub subject: BeliefSubject,
    pub predicate: String,
    pub object: Scalar,
    pub source_system: String,
    pub source_record_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_uri: Option<String>,
    #[serde(default)]
    pub authority_level: AuthorityTier,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<Timestamp>,
    pub asserted_at: Timestamp,
    pub review_status: AssertionReviewStatus,
    pub policy: Policy,
    pub provenance: Provenance,
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use serde_json::{Value, json};

    use super::*;
    use crate::{
        Actor, ActorKind, BeliefSource, BeliefSourceTargetType, Id, Policy, Retention, Visibility,
    };

    fn scope() -> Scope {
        Scope {
            tenant: "t1".to_owned(),
            subject: None,
            workspace: None,
            session: None,
            environment: None,
        }
    }

    fn policy() -> Policy {
        Policy {
            visibility: Visibility::Workspace,
            retention: Retention::Durable,
            sensitivity: None,
            allowed_uses: Vec::new(),
            expires_at: None,
            delete_mode: None,
        }
    }

    fn provenance() -> Provenance {
        Provenance {
            source: "catalog".to_owned(),
            actor: Actor {
                id: Id::from("sys"),
                kind: ActorKind::System,
                display_name: None,
                metadata: None,
            },
            observed_at: Utc.timestamp_opt(10, 0).single().unwrap(),
            evidence: Vec::new(),
            derivations: Vec::new(),
            confidence: Some(0.9),
            method: None,
        }
    }

    fn sample() -> SourceAssertion {
        SourceAssertion {
            id: Id::from("a1"),
            scope: scope(),
            subject: BeliefSubject {
                key: "system:auth".to_owned(),
                entity_ref: None,
                concept_ref: None,
                aliases: Vec::new(),
            },
            predicate: "owned_by".to_owned(),
            object: json!("team-blue"),
            source_system: "catalog".to_owned(),
            source_record_id: "rec-1".to_owned(),
            source_uri: Some("https://catalog/rec-1".to_owned()),
            authority_level: AuthorityTier::Primary,
            confidence: Some(0.9),
            valid_from: Some(Utc.timestamp_opt(0, 0).single().unwrap()),
            valid_until: None,
            asserted_at: Utc.timestamp_opt(10, 0).single().unwrap(),
            review_status: AssertionReviewStatus::Reviewed,
            policy: policy(),
            provenance: provenance(),
        }
    }

    #[test]
    fn source_assertion_round_trips() {
        let assertion = sample();
        let json = serde_json::to_string(&assertion).unwrap();
        let back: SourceAssertion = serde_json::from_str(&json).unwrap();
        assert_eq!(assertion, back);
    }

    #[test]
    fn authority_level_defaults_to_primary_when_absent() {
        // Serialize, drop authorityLevel, and confirm it deserializes to Primary.
        let mut value: Value = serde_json::to_value(sample()).unwrap();
        value.as_object_mut().unwrap().remove("authorityLevel");
        assert!(value.get("authorityLevel").is_none());
        let back: SourceAssertion = serde_json::from_value(value).unwrap();
        assert_eq!(back.authority_level, AuthorityTier::Primary);
    }

    #[test]
    fn belief_source_authority_level_is_absent_by_default() {
        // A pre-change belief-source JSON (no authorityLevel) deserializes with
        // authority_level None — additive and wire-compatible.
        let json = json!({
            "targetType": "assertion",
            "targetId": "a1"
        });
        let source: BeliefSource = serde_json::from_value(json).unwrap();
        assert_eq!(source.target_type, BeliefSourceTargetType::Assertion);
        assert_eq!(source.authority_level, None);
    }

    #[test]
    fn authority_and_review_status_tokens_are_disjoint() {
        let tiers = [
            AuthorityTier::Primary,
            AuthorityTier::Secondary,
            AuthorityTier::Inferred,
        ];
        let statuses = [
            AssertionReviewStatus::Source,
            AssertionReviewStatus::Candidate,
            AssertionReviewStatus::Reviewed,
            AssertionReviewStatus::Authoritative,
            AssertionReviewStatus::Disputed,
            AssertionReviewStatus::Deprecated,
            AssertionReviewStatus::Rejected,
        ];
        let tier_tokens: Vec<String> = tiers
            .iter()
            .map(|t| {
                serde_json::to_value(t)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_owned()
            })
            .collect();
        let status_tokens: Vec<String> = statuses
            .iter()
            .map(|s| {
                serde_json::to_value(s)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_owned()
            })
            .collect();
        for tier in &tier_tokens {
            assert!(
                !status_tokens.contains(tier),
                "authority tier token `{tier}` collides with a review status token"
            );
        }
    }
}
