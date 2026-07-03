//! Query value objects for belief repository reads.
//!
//! These structs describe storage-neutral reads that compatibility adapters can
//! map to their public contracts. They avoid AgentZero-specific names while
//! preserving the required behavior: scope isolation, subject lookup, valid-time
//! filtering, stale/live filtering, and explicit record-time rejection when a
//! repository cannot answer historical versions.

use engram_domain::{Belief, BeliefSourceTargetType, BeliefStatus, Scope, Timestamp};

use crate::{is_live_belief, live_at};

/// Ordering applied when more than one belief matches a query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeliefQueryOrder {
    /// Prefer the most recent valid interval, then newest record timestamp.
    LatestValidFirst,
    /// Prefer newest record timestamp before valid interval order.
    LatestRecordedFirst,
}

/// Read query for a single belief stance.
#[derive(Debug, Clone, PartialEq)]
pub struct BeliefQuery {
    pub scope: Scope,
    pub subject_key: Option<String>,
    pub valid_at: Option<Timestamp>,
    pub recorded_at: Option<Timestamp>,
    pub statuses: Vec<BeliefStatus>,
    pub include_stale: bool,
    pub order: BeliefQueryOrder,
}

impl BeliefQuery {
    /// Creates a live-belief query scoped to one subject and valid at `as_of`.
    pub fn live_subject(scope: Scope, subject_key: impl Into<String>, as_of: Timestamp) -> Self {
        Self {
            scope,
            subject_key: Some(subject_key.into()),
            valid_at: Some(as_of),
            recorded_at: None,
            statuses: vec![BeliefStatus::Active],
            include_stale: false,
            order: BeliefQueryOrder::LatestValidFirst,
        }
    }

    /// Returns whether this query asks for record-time history.
    ///
    /// Repositories without version history must reject such queries. Current
    /// row timestamps alone are not enough to answer bitemporal audit semantics.
    pub fn requires_record_time_history(&self) -> bool {
        self.recorded_at.is_some()
    }

    /// Tests all storage-neutral predicates except repository-specific scope
    /// visibility. Callers should apply their own `ScopeMatcher` first.
    pub fn matches_after_scope(&self, belief: &Belief, now: Timestamp) -> bool {
        if self
            .subject_key
            .as_ref()
            .is_some_and(|subject| belief.subject.key != *subject)
        {
            return false;
        }
        if !self.statuses.is_empty() && !self.statuses.contains(&belief.status) {
            return false;
        }
        if !self.include_stale && !is_live_belief(belief) {
            return false;
        }
        let as_of = self.valid_at.unwrap_or(now);
        live_at(belief.valid_from, belief.valid_until, as_of)
    }
}

/// Query for beliefs that cite a source/evidence target.
#[derive(Debug, Clone, PartialEq)]
pub struct BeliefReferenceQuery {
    pub scope: Scope,
    pub source_type: BeliefSourceTargetType,
    pub source_id: String,
    pub valid_at: Option<Timestamp>,
}

impl BeliefReferenceQuery {
    /// Creates a source-reference query using the supplied target identity.
    pub fn new(
        scope: Scope,
        source_type: BeliefSourceTargetType,
        source_id: impl Into<String>,
    ) -> Self {
        Self {
            scope,
            source_type,
            source_id: source_id.into(),
            valid_at: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use engram_domain::*;

    use super::*;

    fn ts(seconds: i64) -> Timestamp {
        Utc.timestamp_opt(seconds, 0).single().expect("timestamp")
    }

    fn belief(status: BeliefStatus, stale: Option<bool>) -> Belief {
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
            status,
            confidence: 0.8,
            sources: Vec::new(),
            valid_from: Some(ts(10)),
            valid_until: Some(ts(20)),
            superseded_by: None,
            stale,
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
    fn live_subject_query_filters_subject_status_stale_and_valid_time() {
        let query = BeliefQuery::live_subject(
            Scope {
                tenant: "tenant-a".to_owned(),
                subject: None,
                workspace: None,
                session: None,
                environment: None,
            },
            "svc-a",
            ts(15),
        );

        assert!(query.matches_after_scope(&belief(BeliefStatus::Active, None), ts(99)));
        assert!(!query.matches_after_scope(&belief(BeliefStatus::Stale, Some(true)), ts(99)));
        assert!(!query.matches_after_scope(&belief(BeliefStatus::Superseded, None), ts(99)));

        let mut wrong_subject = belief(BeliefStatus::Active, None);
        wrong_subject.subject.key = "svc-b".to_owned();
        assert!(!query.matches_after_scope(&wrong_subject, ts(99)));

        let expired = BeliefQuery::live_subject(query.scope.clone(), "svc-a", ts(20));
        assert!(!expired.matches_after_scope(&belief(BeliefStatus::Active, None), ts(99)));
    }

    #[test]
    fn recorded_at_is_an_explicit_history_request() {
        let mut query = BeliefQuery::live_subject(
            Scope {
                tenant: "tenant-a".to_owned(),
                subject: None,
                workspace: None,
                session: None,
                environment: None,
            },
            "svc-a",
            ts(15),
        );
        assert!(!query.requires_record_time_history());
        query.recorded_at = Some(ts(16));
        assert!(query.requires_record_time_history());
    }
}
