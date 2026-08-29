//! Graph maintenance contracts (ADR-0027).
//!
//! Reversible, plan/apply graph repair over knowledge entities and relationships.
//! Reversibility is archive/restore only — there is no durable audit table and no
//! export-snapshot rollback; attribution rides the existing [`crate::Provenance`]
//! on each mutated record. Atomicity is backend-dependent (ADR-0022): the port
//! declares intent and the adapter reports the realized level in
//! [`MaintenanceApplyResult`].
//!
//! These are storage-neutral data contracts only — no ports, no SQL, no I/O or
//! stateful behavior. The maintenance port lands in `engram-knowledge`; the
//! SQLite apply lands in the adapter.

use serde::{Deserialize, Serialize};

use crate::{
    Actor, EntityId, EntityKind, EvidenceRef, KnowledgeEntity, KnowledgeGraphId,
    KnowledgeRelationship, RelationshipId, Scope, SourceId,
};

// ── Targets and mutation kinds ──────────────────────────────────────────────

/// A targetable knowledge record for a maintenance mutation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaintenanceTarget {
    Entity(EntityId),
    Relationship(RelationshipId),
}

/// The discriminant of a [`MaintenanceMutation`], for per-kind result grouping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationKind {
    Archive,
    Restore,
    Delete,
    Merge,
    AddAlias,
    RemoveAlias,
    RewriteRelationship,
}

/// One mutation in a maintenance plan.
///
/// `Delete` is escalated and permanent; every other kind is reversible via
/// archive/restore (ADR-0027). `Merge` archives — does not hard-delete — its
/// absorbed entities and coalesced relationships.
#[derive(Debug, Clone, PartialEq, Hash, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum MaintenanceMutation {
    Archive {
        target: MaintenanceTarget,
    },
    Restore {
        target: MaintenanceTarget,
    },
    Delete {
        target: MaintenanceTarget,
    },
    Merge {
        survivor: EntityId,
        /// Entities folded into the survivor; archived (not hard-deleted) on apply.
        absorbed: Vec<EntityId>,
    },
    AddAlias {
        entity: EntityId,
        alias: String,
    },
    RemoveAlias {
        entity: EntityId,
        alias: String,
    },
    RewriteRelationship {
        relationship: RelationshipId,
        #[serde(skip_serializing_if = "Option::is_none")]
        new_predicate: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        new_subject: Option<EntityId>,
        #[serde(skip_serializing_if = "Option::is_none")]
        new_object: Option<EntityId>,
    },
}

impl MaintenanceMutation {
    /// The discriminant of this mutation, for per-kind result grouping.
    pub fn kind(&self) -> MutationKind {
        match self {
            Self::Archive { .. } => MutationKind::Archive,
            Self::Restore { .. } => MutationKind::Restore,
            Self::Delete { .. } => MutationKind::Delete,
            Self::Merge { .. } => MutationKind::Merge,
            Self::AddAlias { .. } => MutationKind::AddAlias,
            Self::RemoveAlias { .. } => MutationKind::RemoveAlias,
            Self::RewriteRelationship { .. } => MutationKind::RewriteRelationship,
        }
    }
}

// ── Dry-run preview ─────────────────────────────────────────────────────────

/// A before/after snapshot of a single knowledge record in a preview.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationSnapshot {
    Entity(KnowledgeEntity),
    Relationship(KnowledgeRelationship),
}

/// The exact before/after state a mutation produces, for dry-run inspection.
///
/// `before` / `after` hold snapshots of the records the mutation touches: a
/// simple Archive is one-in/one-out, a Merge is many-in/one-out, a Delete is
/// one-in/zero-out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceMutationPreview {
    pub mutation: MaintenanceMutation,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub before: Vec<MutationSnapshot>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub after: Vec<MutationSnapshot>,
}

// ── Policy and candidates ───────────────────────────────────────────────────

/// Policy a host supplies to drive candidate detection and plan generation.
///
/// Generic data-management knobs only — no product-specific rules, no ontology
/// content (ADR-0027). Candidate detection is deterministic given (graph, policy).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaintenancePolicy {
    /// Below this confidence, an entity/relationship is a low-confidence candidate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence_threshold: Option<f32>,
    #[serde(default = "default_true")]
    pub detect_orphans: bool,
    #[serde(default = "default_true")]
    pub detect_low_confidence: bool,
    #[serde(default = "default_true")]
    pub detect_unsupported: bool,
    #[serde(default = "default_true")]
    pub detect_duplicates: bool,
}

fn default_true() -> bool {
    true
}

impl Default for MaintenancePolicy {
    fn default() -> Self {
        Self {
            confidence_threshold: None,
            detect_orphans: true,
            detect_low_confidence: true,
            detect_unsupported: true,
            detect_duplicates: true,
        }
    }
}

/// The kind of maintenance candidate deterministic detection surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateKind {
    Orphan,
    LowConfidence,
    Unsupported,
    Duplicate,
}

/// Human review state on a candidate, when a reviewer has looked at it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    Pending,
    Accepted,
    Rejected,
}

/// A maintenance candidate surfaced by deterministic detection.
///
/// Carries enough provenance to explain and reverse the decision (ADR-0027).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceCandidate {
    pub kind: CandidateKind,
    pub target: MaintenanceTarget,
    /// Machine-readable reason (e.g. `no_incident_edges`, `confidence_below_threshold`).
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_refs: Vec<EvidenceRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review_status: Option<ReviewStatus>,
    /// The actor who reviewed the candidate, when a human reviewed it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reviewer: Option<Actor>,
}

// ── Apply outcome ───────────────────────────────────────────────────────────

/// The atomicity guarantee a maintenance apply realizes. Backend-dependent
/// (ADR-0022): the port declares intent; the adapter reports the realized level
/// in [`MaintenanceApplyResult`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Atomicity {
    /// The apply ran in a single backend transaction; all-or-nothing within it.
    SingleTransaction,
    /// Best-effort across adapters; per-step results surfaced, no cross-store ACID.
    BestEffort,
    /// The port-level declaration of intent, before an adapter reports the level.
    BackendDependent,
}

/// Severity of a post-apply integrity-verify finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerifySeverity {
    /// Trips rollback on a transactional backend.
    Error,
    /// Surfaced but does not block commit (e.g. an advisory ontology warning).
    Warning,
}

/// One finding from the post-apply integrity verify.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceVerifyFinding {
    pub severity: VerifySeverity,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<MaintenanceTarget>,
}

/// Per-kind counts from applying a plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyKindCount {
    pub kind: MutationKind,
    #[serde(default)]
    pub applied: u32,
    #[serde(default)]
    pub unchanged: u32,
    #[serde(default)]
    pub failed: u32,
}

/// The outcome of applying (or dry-run-previewing) a maintenance plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceApplyResult {
    pub applied: u32,
    pub unchanged: u32,
    pub failed: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub by_kind: Vec<ApplyKindCount>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub verify_findings: Vec<MaintenanceVerifyFinding>,
    pub atomicity: Atomicity,
    /// Echoes the applied plan's *mutations* digest, so the caller can confirm the
    /// mutation set it reviewed is the mutation set that was applied (stale/
    /// different-plan detection). Covers mutations only — scope and graph are
    /// apply-time routing the caller already holds, not part of the digest.
    pub plan_fingerprint: String,
}

// ── Port request/response supporting types ──────────────────────────────────

/// Whether a maintenance operation mutates. `Preview` (the default) stages and
/// reports without committing; `Apply` commits inside one backend transaction
/// where the backend supports it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplyMode {
    #[default]
    Preview,
    Apply,
}

/// Filter for listing entities. Active records only by default
/// (`include_archived = false`), per ADR-0027.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityFilter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graph_id: Option<KnowledgeGraphId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kinds: Vec<EntityKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<SourceId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_confidence: Option<f32>,
    #[serde(default)]
    pub include_archived: bool,
}

/// Filter for listing relationships. Active records only by default.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationshipFilter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graph_id: Option<KnowledgeGraphId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub predicate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<SourceId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_confidence: Option<f32>,
    #[serde(default)]
    pub include_archived: bool,
}

/// Request to build a dry-run maintenance plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaintenancePlanRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graph_id: Option<KnowledgeGraphId>,
    pub scope: Scope,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mutations: Vec<MaintenanceMutation>,
    #[serde(default)]
    pub policy: MaintenancePolicy,
    /// The actor applying the plan — stamped into each mutation's `Provenance`.
    pub actor: Actor,
}

/// Point-in-time graph-health aggregates for one scope (+ optional graph). The
/// adapter's `graph_health` computes these; a per-source breakdown may be added
/// there. All fields default so partial aggregates deserialize cleanly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceHealth {
    pub scope: Scope,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graph_id: Option<KnowledgeGraphId>,
    #[serde(default)]
    pub orphan_count: u32,
    #[serde(default)]
    pub low_confidence_count: u32,
    #[serde(default)]
    pub unsupported_count: u32,
    #[serde(default)]
    pub duplicate_count: u32,
    #[serde(default)]
    pub archived_entity_count: u32,
    #[serde(default)]
    pub archived_relationship_count: u32,
}

// ── Plan and fingerprint ────────────────────────────────────────────────────

/// A maintenance plan: the exact mutations to apply, the policy that generated
/// them, a deterministic fingerprint, and (when produced by the port's
/// `build_plan`) the before/after previews for dry-run review.
/// `MaintenancePlan::new` is the pure constructor (no previews); the maintenance
/// port's `build_plan` fills `previews` by reading current state. `fingerprint`
/// covers mutations only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaintenancePlan {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graph_id: Option<KnowledgeGraphId>,
    pub scope: Scope,
    pub mutations: Vec<MaintenanceMutation>,
    pub policy: MaintenancePolicy,
    /// Deterministic digest over the sorted mutations (see [`plan_fingerprint`]).
    pub fingerprint: String,
    /// Before/after previews for each mutation, filled by the port's `build_plan`.
    /// Empty for a pure-constructed plan; not part of the fingerprint.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previews: Vec<MaintenanceMutationPreview>,
    /// The actor applying the plan — stamped into each mutation's `Provenance`.
    pub actor: Actor,
}

impl MaintenancePlan {
    /// Build a plan, computing its deterministic fingerprint (no previews — the
    /// port's `build_plan` fills those by reading current state).
    pub fn new(
        graph_id: Option<KnowledgeGraphId>,
        scope: Scope,
        mutations: Vec<MaintenanceMutation>,
        policy: MaintenancePolicy,
        actor: Actor,
    ) -> Self {
        let fingerprint = plan_fingerprint(&mutations);
        Self {
            graph_id,
            scope,
            mutations,
            policy,
            fingerprint,
            previews: Vec::new(),
            actor,
        }
    }
}

/// Deterministic, stable, non-cryptographic digest over a plan's mutations.
///
/// Sorts each mutation's canonical JSON and folds it with FNV-1a 64-bit. Stable
/// across process restarts and Rust versions (no randomized hasher) and
/// order-independent (the sorted sequence defines the plan; multiplicity is
/// preserved — dedupe is the caller's responsibility). NOT cryptographic — the
/// stronger SHA-256 digest used by the migration layer lives behind
/// `engram-integration`, which may pull a crypto crate; the domain layer stays
/// dependency-free (AGENTS.md domain purity).
pub fn plan_fingerprint(mutations: &[MaintenanceMutation]) -> String {
    let mut serialized: Vec<String> = mutations
        .iter()
        .map(|m| {
            serde_json::to_string(m)
                .expect("MaintenanceMutation serializes — pure-data enum, no NaN/map keys")
        })
        .collect();
    serialized.sort();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325; // FNV-1a 64-bit offset basis
    for blob in &serialized {
        for byte in blob.as_bytes() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3); // FNV-1a 64-bit prime
        }
        hash ^= b'\n' as u64; // stable separator between entries
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EntityKind, Timestamp};
    use chrono::{DateTime, Utc};

    fn ts() -> Timestamp {
        "2026-01-01T00:00:00Z"
            .parse::<DateTime<Utc>>()
            .expect("rfc3339 timestamp")
    }

    fn entity(id: &str) -> KnowledgeEntity {
        KnowledgeEntity {
            id: EntityId::from(id),
            graph_id: None,
            kind: EntityKind::Concept,
            name: format!("entity-{id}"),
            aliases: Vec::new(),
            scope: crate::Scope {
                tenant: "t".to_string(),
                subject: None,
                workspace: None,
                session: None,
                environment: None,
            },
            source_refs: Vec::new(),
            concept_refs: Vec::new(),
            ontology_class_refs: Vec::new(),
            provenance: crate::Provenance {
                source: "test".to_string(),
                actor: crate::Actor {
                    id: crate::ActorId::from("tester"),
                    kind: crate::ActorKind::System,
                    display_name: None,
                    metadata: None,
                },
                observed_at: ts(),
                evidence: Vec::new(),
                derivations: Vec::new(),
                confidence: None,
                method: Some("maintenance".to_string()),
            },
            created_at: ts(),
            updated_at: None,
            valid_from: None,
            valid_until: None,
            archived_at: None,
            metadata: None,
        }
    }

    #[test]
    fn entity_archived_at_defaults_none_and_omits_when_absent() {
        // An existing record without `archivedAt` deserializes to None — the field
        // is additive, no value migration (ADR-0027).
        let minimal = serde_json::json!({
            "id": "e1",
            "kind": "concept",
            "name": "X",
            "scope": { "tenant": "t" },
            "provenance": {
                "source": "s",
                "actor": { "id": "a", "kind": "system" },
                "observedAt": "2026-01-01T00:00:00Z"
            },
            "createdAt": "2026-01-01T00:00:00Z"
        });
        let e: KnowledgeEntity = serde_json::from_value(minimal).expect("deserialize");
        assert_eq!(e.archived_at, None);
    }

    #[test]
    fn entity_archived_at_round_trips() {
        let mut e = entity("e1");
        let none_json = serde_json::to_string(&e).expect("serialize");
        assert!(
            !none_json.contains("archivedAt"),
            "None archived_at must be omitted"
        );

        e.archived_at = Some(ts());
        let some_json = serde_json::to_string(&e).expect("serialize");
        assert!(
            some_json.contains("archivedAt"),
            "Some archived_at must serialize"
        );

        let back: KnowledgeEntity = serde_json::from_str(&some_json).expect("deserialize");
        assert_eq!(back.archived_at, e.archived_at);
        assert_eq!(back, e);
    }

    #[test]
    fn relationship_archived_at_round_trips() {
        let mut r = crate::KnowledgeRelationship {
            id: RelationshipId::from("r1"),
            graph_id: None,
            subject: crate::EntityRef {
                id: Some(EntityId::from("e1")),
                kind: None,
                name: None,
                aliases: Vec::new(),
            },
            predicate: "calls".to_string(),
            object: crate::EntityRef {
                id: Some(EntityId::from("e2")),
                kind: None,
                name: None,
                aliases: Vec::new(),
            },
            scope: crate::Scope {
                tenant: "t".to_string(),
                subject: None,
                workspace: None,
                session: None,
                environment: None,
            },
            evidence: Vec::new(),
            confidence: None,
            provenance: entity("e1").provenance,
            created_at: ts(),
            updated_at: None,
            archived_at: None,
        };
        assert!(
            !serde_json::to_string(&r).unwrap().contains("archivedAt"),
            "None archived_at must be omitted"
        );
        r.archived_at = Some(ts());
        let json = serde_json::to_string(&r).unwrap();
        let back: crate::KnowledgeRelationship = serde_json::from_str(&json).unwrap();
        assert_eq!(back.archived_at, r.archived_at);
    }

    #[test]
    fn mutation_kind_discriminant_round_trips() {
        let mutations = vec![
            MaintenanceMutation::Archive {
                target: MaintenanceTarget::Entity(EntityId::from("e1")),
            },
            MaintenanceMutation::Merge {
                survivor: EntityId::from("e2"),
                absorbed: vec![EntityId::from("e3"), EntityId::from("e4")],
            },
            MaintenanceMutation::AddAlias {
                entity: EntityId::from("e1"),
                alias: "X".to_string(),
            },
        ];
        for m in &mutations {
            let json = serde_json::to_string(m).expect("serialize");
            let back: MaintenanceMutation = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(&back, m);
            assert_eq!(back.kind(), m.kind());
        }
        assert_eq!(mutations[0].kind(), MutationKind::Archive);
        assert_eq!(mutations[1].kind(), MutationKind::Merge);
        assert_eq!(mutations[2].kind(), MutationKind::AddAlias);
    }

    #[test]
    fn plan_fingerprint_is_stable_and_order_independent() {
        let archive = MaintenanceMutation::Archive {
            target: MaintenanceTarget::Entity(EntityId::from("e1")),
        };
        let merge = MaintenanceMutation::Merge {
            survivor: EntityId::from("e2"),
            absorbed: vec![EntityId::from("e3")],
        };

        // Stable across calls.
        let a = plan_fingerprint(&[archive.clone(), merge.clone()]);
        let b = plan_fingerprint(&[archive.clone(), merge.clone()]);
        assert_eq!(a, b);

        // Order-independent (sorted set defines the plan).
        let reversed = plan_fingerprint(&[merge.clone(), archive.clone()]);
        assert_eq!(a, reversed);

        // Fixed 16-hex-char width.
        assert_eq!(a.len(), 16);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));

        // Different mutation set ⇒ different fingerprint.
        let only_one = plan_fingerprint(&[archive]);
        assert_ne!(a, only_one);

        // Golden value pins the exact digest so impl drift (basis/prime/separator/
        // width) is caught — the relative assertions above cannot.
        assert_eq!(a, "8f804d185abd8f3e");
        // Empty plan: still a stable 16-hex digest.
        let empty = plan_fingerprint(&[]);
        assert_eq!(empty.len(), 16);
        assert!(empty.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn maintenance_plan_new_computes_fingerprint() {
        let mutations = vec![MaintenanceMutation::Restore {
            target: MaintenanceTarget::Relationship(RelationshipId::from("r1")),
        }];
        let plan = MaintenancePlan::new(
            None,
            crate::Scope {
                tenant: "t".to_string(),
                subject: None,
                workspace: None,
                session: None,
                environment: None,
            },
            mutations.clone(),
            MaintenancePolicy::default(),
            crate::Actor {
                id: crate::ActorId::from("tester"),
                kind: crate::ActorKind::System,
                display_name: None,
                metadata: None,
            },
        );
        assert_eq!(plan.fingerprint, plan_fingerprint(&mutations));
        assert!(!plan.fingerprint.is_empty());
    }

    #[test]
    fn apply_result_distinguishes_counts_and_carries_atomicity_and_fingerprint() {
        let result = MaintenanceApplyResult {
            applied: 2,
            unchanged: 1,
            failed: 0,
            by_kind: vec![ApplyKindCount {
                kind: MutationKind::Archive,
                applied: 2,
                unchanged: 1,
                failed: 0,
            }],
            verify_findings: vec![MaintenanceVerifyFinding {
                severity: VerifySeverity::Error,
                message: "dangling reference to absorbed id".to_string(),
                target: Some(MaintenanceTarget::Entity(EntityId::from("e9"))),
            }],
            atomicity: Atomicity::SingleTransaction,
            plan_fingerprint: "deadbeefdeadbeef".to_string(),
        };
        let json = serde_json::to_string(&result).expect("serialize");
        let back: MaintenanceApplyResult = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.applied, 2);
        assert_eq!(back.unchanged, 1);
        assert_eq!(back.failed, 0);
        assert_eq!(back.atomicity, Atomicity::SingleTransaction);
        assert_eq!(back.plan_fingerprint, "deadbeefdeadbeef");
        assert_eq!(back, result);
    }

    #[test]
    fn candidate_round_trips_with_and_without_review() {
        let target = MaintenanceTarget::Entity(EntityId::from("e1"));
        let without = MaintenanceCandidate {
            kind: CandidateKind::Orphan,
            target: target.clone(),
            reason: "no_incident_edges".to_string(),
            confidence: None,
            source_refs: Vec::new(),
            review_status: None,
            reviewer: None,
        };
        let json = serde_json::to_string(&without).expect("serialize");
        assert!(
            !json.contains("reviewStatus") && !json.contains("reviewer"),
            "None review fields must be omitted"
        );
        let back: MaintenanceCandidate = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, without);

        let with_review = MaintenanceCandidate {
            review_status: Some(ReviewStatus::Accepted),
            reviewer: Some(crate::Actor {
                id: crate::ActorId::from("reviewer"),
                kind: crate::ActorKind::User,
                display_name: None,
                metadata: None,
            }),
            ..without
        };
        let json2 = serde_json::to_string(&with_review).expect("serialize");
        let back2: MaintenanceCandidate = serde_json::from_str(&json2).expect("deserialize");
        assert_eq!(back2.review_status, Some(ReviewStatus::Accepted));
        assert_eq!(back2, with_review);
    }

    #[test]
    fn rewrite_relationship_round_trips_all_none_and_populated() {
        let minimal = MaintenanceMutation::RewriteRelationship {
            relationship: RelationshipId::from("r1"),
            new_predicate: None,
            new_subject: None,
            new_object: None,
        };
        let j = serde_json::to_string(&minimal).expect("serialize");
        assert!(j.contains(r#""kind":"rewrite_relationship""#));
        assert!(j.contains(r#""relationship":"r1""#));
        // skip_serializing_if omits the None payload fields
        assert!(!j.contains("newPredicate"));
        assert!(!j.contains("newSubject"));
        assert!(!j.contains("newObject"));
        let back: MaintenanceMutation = serde_json::from_str(&j).expect("deserialize");
        assert_eq!(back, minimal);

        let populated = MaintenanceMutation::RewriteRelationship {
            relationship: RelationshipId::from("r1"),
            new_predicate: Some("implements".to_string()),
            new_subject: Some(EntityId::from("e9")),
            new_object: Some(EntityId::from("e8")),
        };
        let j2 = serde_json::to_string(&populated).expect("serialize");
        // camelCase payload fields (rename_all_fields)
        assert!(j2.contains(r#""newPredicate":"implements""#));
        assert!(j2.contains(r#""newSubject":"e9""#));
        assert!(j2.contains(r#""newObject":"e8""#));
        let back2: MaintenanceMutation = serde_json::from_str(&j2).expect("deserialize");
        assert_eq!(back2, populated);
        assert_eq!(back2.kind(), MutationKind::RewriteRelationship);
    }

    fn scope_t() -> Scope {
        Scope {
            tenant: "t".to_string(),
            subject: None,
            workspace: None,
            session: None,
            environment: None,
        }
    }

    #[test]
    fn apply_mode_defaults_to_preview() {
        assert_eq!(ApplyMode::default(), ApplyMode::Preview);
    }

    #[test]
    fn filters_default_to_active_only() {
        assert!(!EntityFilter::default().include_archived);
        assert!(!RelationshipFilter::default().include_archived);
    }

    #[test]
    fn plan_request_and_health_round_trip_camel_case() {
        let request = MaintenancePlanRequest {
            graph_id: None,
            scope: scope_t(),
            mutations: Vec::new(),
            policy: MaintenancePolicy::default(),
            actor: crate::Actor {
                id: crate::ActorId::from("tester"),
                kind: crate::ActorKind::System,
                display_name: None,
                metadata: None,
            },
        };
        let j = serde_json::to_string(&request).expect("serialize");
        let back: MaintenancePlanRequest = serde_json::from_str(&j).expect("deserialize");
        assert_eq!(back, request);

        let health = MaintenanceHealth {
            scope: scope_t(),
            graph_id: None,
            orphan_count: 3,
            low_confidence_count: 1,
            unsupported_count: 0,
            duplicate_count: 2,
            archived_entity_count: 5,
            archived_relationship_count: 4,
        };
        let j2 = serde_json::to_string(&health).expect("serialize");
        assert!(j2.contains("orphanCount"), "camelCase field");
        let back2: MaintenanceHealth = serde_json::from_str(&j2).expect("deserialize");
        assert_eq!(back2, health);
    }

    #[test]
    fn pure_plan_has_no_previews() {
        let plan = MaintenancePlan::new(
            None,
            scope_t(),
            Vec::new(),
            MaintenancePolicy::default(),
            crate::Actor {
                id: crate::ActorId::from("tester"),
                kind: crate::ActorKind::System,
                display_name: None,
                metadata: None,
            },
        );
        assert!(plan.previews.is_empty(), "::new produces no previews");
    }
}
