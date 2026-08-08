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
