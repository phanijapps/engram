//! Graph maintenance port — reversible, plan/apply graph repair (ADR-0027).
//!
//! Storage-neutral contract; the SQLite apply (transactional, archive-disposition
//! merge) lands in the adapter. Sits alongside [`EntityIdentityRepository`] as a
//! focused port, composing the existing identity/knowledge primitives behind a
//! plan/apply surface rather than folding them into `KnowledgeRepository` or the
//! `consolidation` crate.
//!
//! Reversibility is archive/restore only (ADR-0027); atomicity is backend-
//! dependent (ADR-0022). The default is non-mutating — every mutating path
//! requires an explicit [`ApplyMode::Apply`].

use std::collections::HashSet;

use async_trait::async_trait;
use engram_domain::*;
use engram_runtime::{CoreError, CoreResult};

/// Reversible, plan/apply maintenance over knowledge entities and relationships.
///
/// All methods default to "not supported" (or empty, for reads) so a stub/test
/// backend overrides only what it exercises; the real behavior is the SQLite
/// adapter. A host never needs direct store access — this port is the only path.
#[async_trait]
pub trait GraphMaintenanceRepository: Send + Sync {
    /// List entities matching a filter with cursor pagination. Active records
    /// only unless `filter.include_archived`.
    async fn list_entities(
        &self,
        _scope: &Scope,
        _filter: &EntityFilter,
        _after: Option<Cursor>,
        _limit: usize,
    ) -> CoreResult<Page<KnowledgeEntity>> {
        Err(unsupported("list_entities"))
    }

    /// List relationships matching a filter with cursor pagination. Active only
    /// unless `filter.include_archived`.
    async fn list_relationships(
        &self,
        _scope: &Scope,
        _filter: &RelationshipFilter,
        _after: Option<Cursor>,
        _limit: usize,
    ) -> CoreResult<Page<KnowledgeRelationship>> {
        Err(unsupported("list_relationships"))
    }

    /// Deterministic candidate detection (orphan / low-confidence / unsupported /
    /// duplicate). No LLM; composes identity/ontology primitives.
    async fn detect_candidates(
        &self,
        _scope: &Scope,
        _graph_id: Option<&KnowledgeGraphId>,
        _policy: &MaintenancePolicy,
    ) -> CoreResult<Vec<MaintenanceCandidate>> {
        Ok(Vec::new())
    }

    /// Build a dry-run plan: fills `before`/`after` previews by reading current
    /// state. Never mutates.
    async fn build_plan(&self, _request: MaintenancePlanRequest) -> CoreResult<MaintenancePlan> {
        Err(unsupported("build_plan"))
    }

    /// Apply (or dry-run-preview) a reviewed plan. [`ApplyMode::Preview`] stages
    /// without committing; [`ApplyMode::Apply`] commits inside one backend
    /// transaction where supported, with a referential-integrity verify before
    /// commit. Idempotent by per-variant target-state.
    async fn apply_plan(
        &self,
        _plan: &MaintenancePlan,
        _mode: ApplyMode,
    ) -> CoreResult<MaintenanceApplyResult> {
        Err(unsupported("apply_plan"))
    }

    /// Point-in-time graph-health aggregates for a scope (+ optional graph).
    async fn graph_health(
        &self,
        _scope: &Scope,
        _graph_id: Option<&KnowledgeGraphId>,
    ) -> CoreResult<MaintenanceHealth> {
        Err(unsupported("graph_health"))
    }
}

fn unsupported(op: &str) -> CoreError {
    CoreError::Adapter {
        adapter: "graph_maintenance".to_owned(),
        message: format!("{op} is not supported by this backend"),
    }
}

// ── Deterministic candidate detection (pure) ────────────────────────────────
//
// Pure over pre-fetched graph data: the adapter (T4/T5) does the async reads
// (`list_entities`/`list_relationships`, `EntityIdentityRepository::discover_collisions`,
// `OntologyRepository::validate_graph`) and passes the slices here. Same inputs ⇒
// the same candidate set; no LLM (ADR-0027).

/// Surface maintenance candidates from pre-fetched graph data, gated by `policy`.
pub fn detect_candidates(
    entities: &[KnowledgeEntity],
    relationships: &[KnowledgeRelationship],
    collisions: &[CollisionGroup],
    findings: &[OntologyValidationFinding],
    policy: &MaintenancePolicy,
) -> Vec<MaintenanceCandidate> {
    let mut out = Vec::new();
    if policy.detect_orphans {
        out.extend(detect_orphans(entities, relationships));
    }
    if policy.detect_low_confidence {
        out.extend(detect_low_confidence(
            entities,
            relationships,
            policy.confidence_threshold,
        ));
    }
    if policy.detect_duplicates {
        out.extend(detect_duplicates(collisions));
    }
    if policy.detect_unsupported {
        out.extend(detect_unsupported(findings));
    }
    out
}

/// Orphan: an active entity with no incident relationship OR no source_refs
/// (AC10). The reason distinguishes which condition held.
pub fn detect_orphans(
    entities: &[KnowledgeEntity],
    relationships: &[KnowledgeRelationship],
) -> Vec<MaintenanceCandidate> {
    let connected: HashSet<&EntityId> = relationships
        .iter()
        .filter(|r| r.archived_at.is_none())
        .filter_map(|r| r.subject.id.as_ref())
        .chain(
            relationships
                .iter()
                .filter(|r| r.archived_at.is_none())
                .filter_map(|r| r.object.id.as_ref()),
        )
        .collect();
    entities
        .iter()
        .filter(|e| e.archived_at.is_none())
        .filter_map(|e| {
            let no_edges = !connected.contains(&e.id);
            let no_sources = e.source_refs.is_empty();
            let reason = match (no_edges, no_sources) {
                (true, true) => "no_incident_edges_and_no_source_refs",
                (true, false) => "no_incident_edges",
                (false, true) => "no_source_refs",
                (false, false) => return None,
            };
            Some(MaintenanceCandidate {
                kind: CandidateKind::Orphan,
                target: MaintenanceTarget::Entity(e.id.clone()),
                reason: reason.to_string(),
                confidence: e.provenance.confidence,
                source_refs: e.source_refs.clone(),
                review_status: None,
                reviewer: None,
            })
        })
        .collect()
}

/// Low-confidence: an active entity/relationship whose confidence is below the
/// policy threshold. No threshold ⇒ the detector is a no-op.
pub fn detect_low_confidence(
    entities: &[KnowledgeEntity],
    relationships: &[KnowledgeRelationship],
    threshold: Option<f32>,
) -> Vec<MaintenanceCandidate> {
    let Some(threshold) = threshold else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in entities.iter().filter(|e| e.archived_at.is_none()) {
        if let Some(c) = e.provenance.confidence {
            if c < threshold {
                out.push(MaintenanceCandidate {
                    kind: CandidateKind::LowConfidence,
                    target: MaintenanceTarget::Entity(e.id.clone()),
                    reason: "entity_confidence_below_threshold".to_string(),
                    confidence: Some(c),
                    source_refs: e.source_refs.clone(),
                    review_status: None,
                    reviewer: None,
                });
            }
        }
    }
    for r in relationships.iter().filter(|r| r.archived_at.is_none()) {
        if let Some(c) = r.confidence.or(r.provenance.confidence) {
            if c < threshold {
                out.push(MaintenanceCandidate {
                    kind: CandidateKind::LowConfidence,
                    target: MaintenanceTarget::Relationship(r.id.clone()),
                    reason: "relationship_confidence_below_threshold".to_string(),
                    confidence: Some(c),
                    source_refs: r.evidence.clone(),
                    review_status: None,
                    reviewer: None,
                });
            }
        }
    }
    out
}

/// Duplicate: every entity after the first in each collision group (the first is
/// the canonical survivor; the rest merge into it).
pub fn detect_duplicates(collisions: &[CollisionGroup]) -> Vec<MaintenanceCandidate> {
    collisions
        .iter()
        .filter(|g| g.entity_ids.len() > 1)
        .flat_map(|g| {
            // Deterministic canonical choice: the survivor is the lexicographically
            // smallest id (order-independent — `discover_collisions` does not
            // guarantee `entity_ids` order); the rest are duplicates.
            let mut ids: Vec<&EntityId> = g.entity_ids.iter().collect();
            ids.sort();
            ids.into_iter().skip(1).map(|id| MaintenanceCandidate {
                kind: CandidateKind::Duplicate,
                target: MaintenanceTarget::Entity(id.clone()),
                reason: format!("duplicate_identity:{}", g.identity_key),
                confidence: None,
                source_refs: Vec::new(),
                review_status: None,
                reviewer: None,
            })
        })
        .collect()
}

/// Unsupported: ontology-validation findings that target an entity.
pub fn detect_unsupported(findings: &[OntologyValidationFinding]) -> Vec<MaintenanceCandidate> {
    findings
        .iter()
        .filter_map(|f| {
            f.target
                .as_ref()
                .and_then(|t| t.id.clone())
                .map(|id| MaintenanceCandidate {
                    kind: CandidateKind::Unsupported,
                    target: MaintenanceTarget::Entity(id),
                    reason: format!("ontology_violation:{}:{}", f.code, f.id),
                    confidence: None,
                    source_refs: Vec::new(),
                    review_status: None,
                    reviewer: None,
                })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Test stub that records writes so the non-mutating defaults are checkable.
    /// Overrides only `build_plan`, `apply_plan`, and `detect_candidates`.
    #[derive(Default)]
    struct StubBackend {
        writes: Mutex<u32>,
    }

    #[async_trait]
    impl GraphMaintenanceRepository for StubBackend {
        async fn build_plan(&self, request: MaintenancePlanRequest) -> CoreResult<MaintenancePlan> {
            let previews = request
                .mutations
                .iter()
                .map(|m| MaintenanceMutationPreview {
                    mutation: m.clone(),
                    before: Vec::new(),
                    after: Vec::new(),
                })
                .collect::<Vec<_>>();
            let mut plan = MaintenancePlan::new(
                request.graph_id,
                request.scope,
                request.mutations,
                request.policy,
            );
            plan.previews = previews;
            Ok(plan)
        }

        async fn apply_plan(
            &self,
            plan: &MaintenancePlan,
            mode: ApplyMode,
        ) -> CoreResult<MaintenanceApplyResult> {
            let applied = if matches!(mode, ApplyMode::Apply) {
                *self.writes.lock().expect("writes lock") += 1;
                1
            } else {
                0
            };
            Ok(MaintenanceApplyResult {
                applied,
                unchanged: 0,
                failed: 0,
                by_kind: Vec::new(),
                verify_findings: Vec::new(),
                atomicity: Atomicity::BackendDependent,
                plan_fingerprint: plan.fingerprint.clone(),
            })
        }

        async fn detect_candidates(
            &self,
            _scope: &Scope,
            _graph_id: Option<&KnowledgeGraphId>,
            _policy: &MaintenancePolicy,
        ) -> CoreResult<Vec<MaintenanceCandidate>> {
            Ok(vec![MaintenanceCandidate {
                kind: CandidateKind::Orphan,
                target: MaintenanceTarget::Entity(EntityId::from("e1")),
                reason: "no_incident_edges".to_string(),
                confidence: None,
                source_refs: Vec::new(),
                review_status: None,
                reviewer: None,
            }])
        }
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

    #[tokio::test]
    async fn build_plan_is_non_mutating_and_carries_previews() {
        let backend = StubBackend::default();
        let request = MaintenancePlanRequest {
            graph_id: None,
            scope: scope_t(),
            mutations: vec![
                MaintenanceMutation::Archive {
                    target: MaintenanceTarget::Entity(EntityId::from("e1")),
                },
                MaintenanceMutation::Restore {
                    target: MaintenanceTarget::Entity(EntityId::from("e2")),
                },
            ],
            policy: MaintenancePolicy::default(),
        };
        let plan = backend.build_plan(request).await.expect("build_plan");
        assert_eq!(plan.mutations.len(), 2);
        assert_eq!(plan.previews.len(), 2, "one preview per mutation");
        assert_eq!(plan.previews[0].mutation, plan.mutations[0]);
    }

    #[tokio::test]
    async fn apply_only_commits_in_apply_mode() {
        let backend = StubBackend::default();
        let plan = MaintenancePlan::new(None, scope_t(), Vec::new(), MaintenancePolicy::default());

        let preview = backend
            .apply_plan(&plan, ApplyMode::Preview)
            .await
            .expect("preview");
        assert_eq!(preview.applied, 0);
        assert_eq!(*backend.writes.lock().expect("writes lock"), 0);

        let apply = backend
            .apply_plan(&plan, ApplyMode::Apply)
            .await
            .expect("apply");
        assert_eq!(apply.applied, 1);
        assert_eq!(*backend.writes.lock().expect("writes lock"), 1);
    }

    #[tokio::test]
    async fn detect_candidates_returns_typed_set() {
        let backend = StubBackend::default();
        let cands = backend
            .detect_candidates(&scope_t(), None, &MaintenancePolicy::default())
            .await
            .expect("detect");
        assert_eq!(cands.len(), 1);
        assert_eq!(cands[0].kind, CandidateKind::Orphan);
    }

    #[tokio::test]
    async fn unimplemented_ops_default_to_unsupported() {
        let backend = StubBackend::default();
        let page = backend
            .list_entities(&scope_t(), &EntityFilter::default(), None, 10)
            .await;
        assert!(page.is_err(), "list_entities defaults to unsupported");
        let health = backend.graph_health(&scope_t(), None).await;
        assert!(health.is_err(), "graph_health defaults to unsupported");
    }

    /// Compile-time check: the trait is reachable as
    /// `engram_knowledge::GraphMaintenanceRepository` (re-exported at the root).
    #[allow(dead_code)]
    fn _trait_is_reachable<T: GraphMaintenanceRepository + ?Sized>(_t: &T) {}
}

#[cfg(test)]
mod detector_tests {
    use super::*;
    use std::collections::HashMap;

    fn ts() -> Timestamp {
        "2026-01-01T00:00:00Z".parse().unwrap()
    }
    fn scope_t() -> Scope {
        Scope {
            tenant: "t".into(),
            subject: None,
            workspace: None,
            session: None,
            environment: None,
        }
    }
    fn eref(id: &str) -> EntityRef {
        EntityRef {
            id: Some(EntityId::from(id)),
            kind: None,
            name: None,
            aliases: Vec::new(),
        }
    }
    fn evref() -> EvidenceRef {
        EvidenceRef {
            target_type: EvidenceTargetType::Document,
            target_id: Some("d".into()),
            uri: None,
            quote: None,
            location: None,
        }
    }
    fn prov(confidence: Option<f32>) -> Provenance {
        Provenance {
            source: "test".into(),
            actor: Actor {
                id: ActorId::from("a"),
                kind: ActorKind::System,
                display_name: None,
                metadata: None,
            },
            observed_at: ts(),
            evidence: Vec::new(),
            derivations: Vec::new(),
            confidence,
            method: None,
        }
    }
    fn ent(id: &str) -> KnowledgeEntity {
        KnowledgeEntity {
            id: EntityId::from(id),
            graph_id: None,
            kind: EntityKind::Concept,
            name: id.to_string(),
            aliases: Vec::new(),
            scope: scope_t(),
            source_refs: Vec::new(),
            concept_refs: Vec::new(),
            ontology_class_refs: Vec::new(),
            provenance: prov(None),
            created_at: ts(),
            updated_at: None,
            valid_from: None,
            valid_until: None,
            archived_at: None,
            metadata: None,
        }
    }
    fn rel(id: &str, subj: &str, obj: &str, confidence: Option<f32>) -> KnowledgeRelationship {
        KnowledgeRelationship {
            id: RelationshipId::from(id),
            graph_id: None,
            subject: eref(subj),
            predicate: "calls".into(),
            object: eref(obj),
            scope: scope_t(),
            evidence: Vec::new(),
            confidence,
            provenance: prov(None),
            created_at: ts(),
            updated_at: None,
            archived_at: None,
        }
    }
    fn finding(id: &str, code: &str, target: Option<&str>) -> OntologyValidationFinding {
        OntologyValidationFinding {
            id: id.to_string(),
            ontology_id: OntologyId::from("ont"),
            severity: OntologyValidationSeverity::Error,
            code: code.to_string(),
            message: "m".into(),
            target: target.map(eref),
            axiom_id: None,
            provenance: prov(None),
            detected_at: ts(),
        }
    }
    fn target_id(c: &MaintenanceCandidate) -> &str {
        match &c.target {
            MaintenanceTarget::Entity(id) | MaintenanceTarget::Relationship(id) => id.as_str(),
        }
    }

    #[test]
    fn orphan_uses_or_semantics_with_precise_reasons() {
        // e1: connected (object of r1) but no source_refs -> orphan (no_source_refs)
        let e1 = ent("e1");
        // e2: not connected, has source_refs -> orphan (no_incident_edges)
        let mut e2 = ent("e2");
        e2.source_refs.push(evref());
        // e3: connected (subject of r1) AND has source_refs -> NOT an orphan
        let mut e3 = ent("e3");
        e3.source_refs.push(evref());
        let rels = vec![rel("r1", "e3", "e1", None)];
        let cands = detect_orphans(&[e1, e2, e3], &rels);
        let by_id: HashMap<&str, &str> = cands
            .iter()
            .map(|c| (target_id(c), c.reason.as_str()))
            .collect();
        assert_eq!(by_id.get("e1").copied(), Some("no_source_refs"));
        assert_eq!(by_id.get("e2").copied(), Some("no_incident_edges"));
        assert!(
            !by_id.contains_key("e3"),
            "connected + sourced entity is not an orphan"
        );
    }

    #[test]
    fn low_confidence_respects_threshold_and_covers_entities_and_relationships() {
        let mut ea = ent("a");
        ea.provenance.confidence = Some(0.2);
        let mut eb = ent("b");
        eb.provenance.confidence = Some(0.9);
        let r_low = rel("r1", "a", "b", Some(0.1));
        let r_ok = rel("r2", "b", "a", Some(0.8));
        // top-level confidence None but provenance.confidence low -> exercises the
        // `r.confidence.or(r.provenance.confidence)` fallback.
        let mut r_prov = rel("r3", "a", "b", None);
        r_prov.provenance.confidence = Some(0.1);

        // No threshold -> the detector is a no-op.
        assert!(
            detect_low_confidence(
                &[ea.clone(), eb.clone()],
                &[r_low.clone(), r_ok.clone(), r_prov.clone()],
                None
            )
            .is_empty()
        );

        // Threshold 0.5 -> entity `a` + relationships `r1`, `r3`.
        let cands = detect_low_confidence(&[ea, eb], &[r_low, r_ok, r_prov], Some(0.5));
        let ids: Vec<&str> = cands.iter().map(target_id).collect();
        assert_eq!(ids, vec!["a", "r1", "r3"]);
        assert!(cands.iter().all(|c| c.kind == CandidateKind::LowConfidence));
    }

    #[test]
    fn duplicates_flag_non_canonical_members_only() {
        let groups = vec![
            CollisionGroup {
                identity_key: "k1".into(),
                entity_ids: vec![
                    EntityId::from("canon"),
                    EntityId::from("dup1"),
                    EntityId::from("dup2"),
                ],
            },
            CollisionGroup {
                identity_key: "k2".into(),
                entity_ids: vec![EntityId::from("solo")], // single member -> not a duplicate
            },
        ];
        let cands = detect_duplicates(&groups);
        let ids: Vec<&str> = cands.iter().map(target_id).collect();
        assert_eq!(ids, vec!["dup1", "dup2"]);
        assert!(cands.iter().all(|c| c.reason == "duplicate_identity:k1"));
    }

    #[test]
    fn unsupported_only_maps_entity_targeted_findings() {
        let findings = vec![
            finding("f1", "bad_edge", Some("e1")),
            finding("f2", "no_target", None), // no entity target -> skipped
        ];
        let cands = detect_unsupported(&findings);
        let ids: Vec<&str> = cands.iter().map(target_id).collect();
        assert_eq!(ids, vec!["e1"]);
        assert_eq!(cands[0].kind, CandidateKind::Unsupported);
    }

    #[test]
    fn detect_candidates_respects_policy_toggles() {
        let mut ea = ent("a");
        ea.provenance.confidence = Some(0.1); // low-confidence
        let eb = ent("b"); // orphan (no edges, no sources)
        let groups = vec![CollisionGroup {
            identity_key: "k".into(),
            entity_ids: vec![EntityId::from("b"), EntityId::from("c")],
        }];
        let findings = vec![finding("f", "x", Some("a"))];

        let mut policy = MaintenancePolicy {
            confidence_threshold: Some(0.5),
            ..MaintenancePolicy::default()
        };
        let all = detect_candidates(&[ea.clone(), eb.clone()], &[], &groups, &findings, &policy);
        let kinds: Vec<_> = all.iter().map(|c| c.kind).collect();
        assert!(kinds.contains(&CandidateKind::Orphan));
        assert!(kinds.contains(&CandidateKind::LowConfidence));
        assert!(kinds.contains(&CandidateKind::Duplicate));
        assert!(kinds.contains(&CandidateKind::Unsupported));

        // Single-toggle isolation: each toggle removes only its own kind.
        let present = |p: &MaintenancePolicy| -> Vec<CandidateKind> {
            detect_candidates(&[ea.clone(), eb.clone()], &[], &groups, &findings, p)
                .into_iter()
                .map(|c| c.kind)
                .collect()
        };
        let mut p = policy.clone();
        p.detect_orphans = false;
        assert!(!present(&p).contains(&CandidateKind::Orphan));
        let mut p = policy.clone();
        p.detect_low_confidence = false;
        assert!(!present(&p).contains(&CandidateKind::LowConfidence));
        let mut p = policy.clone();
        p.detect_duplicates = false;
        assert!(!present(&p).contains(&CandidateKind::Duplicate));
        let mut p = policy.clone();
        p.detect_unsupported = false;
        assert!(!present(&p).contains(&CandidateKind::Unsupported));

        // Disable everything -> empty.
        policy.detect_orphans = false;
        policy.detect_low_confidence = false;
        policy.detect_duplicates = false;
        policy.detect_unsupported = false;
        assert!(detect_candidates(&[ea, eb], &[], &groups, &findings, &policy).is_empty());
    }
}
