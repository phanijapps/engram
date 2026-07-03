//! Belief lifecycle transitions and evidence-reference predicates.
//!
//! Lifecycle helpers mutate only the fields required by the transition. They do
//! not rewrite content, evidence, provenance, policy, or scope, which keeps
//! compatibility adapters from accidentally turning review state into source
//! truth.

use engram_domain::{Belief, BeliefId, BeliefSourceTargetType, BeliefStatus, Timestamp};

use crate::live_at;

/// Returns true when a belief is an active recall candidate at the current
/// status layer.
pub fn is_live_belief(belief: &Belief) -> bool {
    belief.status == BeliefStatus::Active && belief.stale != Some(true)
}

/// Marks a belief stale while preserving its content and evidence.
pub fn mark_stale(mut belief: Belief, at: Timestamp) -> Belief {
    belief.status = BeliefStatus::Stale;
    belief.stale = Some(true);
    belief.updated_at = Some(at);
    belief
}

/// Clears stale state without changing content or evidence.
pub fn clear_stale_state(mut belief: Belief, at: Timestamp) -> Belief {
    if belief.status == BeliefStatus::Stale {
        belief.status = BeliefStatus::Active;
    }
    belief.stale = Some(false);
    belief.updated_at = Some(at);
    belief
}

/// Closes a belief interval and links the replacement belief.
pub fn supersede_belief(mut belief: Belief, replacement_id: BeliefId, at: Timestamp) -> Belief {
    belief.status = BeliefStatus::Superseded;
    belief.valid_until = Some(at);
    belief.superseded_by = Some(replacement_id);
    belief.updated_at = Some(at);
    belief
}

/// Closes a belief interval without selecting a replacement belief.
pub fn retract_belief(mut belief: Belief, at: Timestamp) -> Belief {
    belief.status = BeliefStatus::Retracted;
    belief.valid_until = Some(at);
    belief.superseded_by = None;
    belief.updated_at = Some(at);
    belief
}

/// Returns true when a live belief cites the requested source target.
pub fn belief_references_source(
    belief: &Belief,
    source_type: &BeliefSourceTargetType,
    source_id: &str,
    as_of: Timestamp,
) -> bool {
    is_live_belief(belief)
        && live_at(belief.valid_from, belief.valid_until, as_of)
        && belief.sources.iter().any(|source| {
            source.target_type == *source_type
                && source.target_id == source_id
                && live_at(source.valid_from, source.valid_until, as_of)
        })
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use engram_domain::*;

    use super::*;

    fn ts(seconds: i64) -> Timestamp {
        Utc.timestamp_opt(seconds, 0).single().expect("timestamp")
    }

    fn belief() -> Belief {
        Belief {
            id: Id::from("belief-1"),
            scope: Scope {
                tenant: "tenant-a".to_owned(),
                subject: None,
                workspace: None,
                session: None,
                environment: None,
            },
            subject: BeliefSubject {
                key: "svc-a".to_owned(),
                entity_ref: None,
                concept_ref: None,
                aliases: Vec::new(),
            },
            content: "up".to_owned(),
            status: BeliefStatus::Active,
            confidence: 0.8,
            sources: vec![BeliefSource {
                target_type: BeliefSourceTargetType::Memory,
                target_id: "fact-1".to_owned(),
                weight: None,
                confidence: None,
                valid_from: None,
                valid_until: None,
            }],
            valid_from: Some(ts(10)),
            valid_until: None,
            superseded_by: None,
            stale: None,
            synthesizer: None,
            reasoning: None,
            embedding_refs: Vec::new(),
            policy: Policy {
                visibility: Visibility::Workspace,
                retention: Retention::Durable,
                sensitivity: None,
                allowed_uses: vec![AllowedUse::Retrieval],
                expires_at: None,
                delete_mode: None,
            },
            provenance: Provenance {
                source: "test".to_owned(),
                actor: Actor {
                    id: Id::from("actor"),
                    kind: ActorKind::Agent,
                    display_name: None,
                    metadata: None,
                },
                observed_at: ts(10),
                evidence: Vec::new(),
                derivations: Vec::new(),
                confidence: Some(1.0),
                method: None,
            },
            created_at: ts(10),
            updated_at: None,
            metadata: None,
        }
    }

    #[test]
    fn lifecycle_transitions_change_only_lifecycle_fields() {
        let original = belief();

        let stale = mark_stale(original.clone(), ts(20));
        assert_eq!(stale.status, BeliefStatus::Stale);
        assert_eq!(stale.stale, Some(true));
        assert_eq!(stale.content, original.content);
        assert_eq!(stale.sources, original.sources);

        let active = clear_stale_state(stale, ts(21));
        assert_eq!(active.status, BeliefStatus::Active);
        assert_eq!(active.stale, Some(false));

        let superseded = supersede_belief(original.clone(), Id::from("belief-2"), ts(30));
        assert_eq!(superseded.status, BeliefStatus::Superseded);
        assert_eq!(superseded.valid_until, Some(ts(30)));
        assert_eq!(superseded.superseded_by, Some(Id::from("belief-2")));

        let retracted = retract_belief(original, ts(40));
        assert_eq!(retracted.status, BeliefStatus::Retracted);
        assert_eq!(retracted.valid_until, Some(ts(40)));
        assert_eq!(retracted.superseded_by, None);
    }

    #[test]
    fn source_reference_requires_live_belief_and_matching_valid_time() {
        let original = belief();
        assert!(belief_references_source(
            &original,
            &BeliefSourceTargetType::Memory,
            "fact-1",
            ts(20)
        ));

        let stale = mark_stale(original.clone(), ts(21));
        assert!(!belief_references_source(
            &stale,
            &BeliefSourceTargetType::Memory,
            "fact-1",
            ts(22)
        ));
        assert!(!belief_references_source(
            &original,
            &BeliefSourceTargetType::Memory,
            "missing",
            ts(20)
        ));
        assert!(!belief_references_source(
            &original,
            &BeliefSourceTargetType::Memory,
            "fact-1",
            ts(9)
        ));

        let mut source_expired = original;
        source_expired.sources[0].valid_from = Some(ts(10));
        source_expired.sources[0].valid_until = Some(ts(20));
        assert!(!belief_references_source(
            &source_expired,
            &BeliefSourceTargetType::Memory,
            "fact-1",
            ts(20)
        ));
    }
}
