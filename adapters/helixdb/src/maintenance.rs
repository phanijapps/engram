//! Storage-only maintenance bridge; one canonical composer owns mutation semantics.
use crate::{HelixStore, records::*};
use async_trait::async_trait;
use engram_domain::*;
use engram_knowledge::{
    EntityIdentityRepository, GraphMaintenanceRepository, OntologyRepository,
    maintenance_composition::{self as composition, MaintenanceStorage},
};
use engram_runtime::{CoreError, CoreResult};
use serde_json::Value;
use std::collections::BTreeMap;
struct Storage<'a>(&'a HelixStore);
impl MaintenanceStorage for Storage<'_> {
    fn visible(&self, a: &Scope, b: &Scope) -> bool {
        a == b && self.0.visible_scope(b)
    }
    fn entity(&self, id: &EntityId, scope: &Scope) -> CoreResult<Option<KnowledgeEntity>> {
        if !self.0.visible_scope(scope) {
            return Ok(None);
        }
        self.0.get(ENTITY, id.as_str()).map_err(core)
    }
    fn relationship(
        &self,
        id: &RelationshipId,
        scope: &Scope,
    ) -> CoreResult<Option<KnowledgeRelationship>> {
        if !self.0.visible_scope(scope) {
            return Ok(None);
        }
        self.0.get(RELATIONSHIP, id.as_str()).map_err(core)
    }
    fn entities(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeEntity>> {
        if !self.0.visible_scope(scope) {
            return Ok(vec![]);
        }
        self.0.all(ENTITY).map_err(core)
    }
    fn relationships(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeRelationship>> {
        if !self.0.visible_scope(scope) {
            return Ok(vec![]);
        }
        self.0.all(RELATIONSHIP).map_err(core)
    }
    fn write_entity(&self, v: &KnowledgeEntity) -> CoreResult<()> {
        self.0.check_scope(&v.scope).map_err(core)?;
        let mut payload = value(v).map_err(core)?;
        if let Some(prior) = self.0.get::<Value>(ENTITY, v.id.as_str()).map_err(core)? {
            if let Some(key) = prior.get("_identity_key") {
                payload["_identity_key"] = key.clone();
            }
        }
        self.0
            .stage(ENTITY, v.id.as_str(), Some(payload))
            .map_err(core)
    }
    fn write_relationship(&self, v: &KnowledgeRelationship) -> CoreResult<()> {
        self.0.check_scope(&v.scope).map_err(core)?;
        let mut payload = value(v).map_err(core)?;
        payload["_subject_endpoint"] = self.0.endpoint(&v.subject).map_err(core)?.into();
        payload["_object_endpoint"] = self.0.endpoint(&v.object).map_err(core)?.into();
        payload["_relationship_key"] =
            engram_knowledge::identity::compute_relationship_key(v).into();
        self.0
            .stage(RELATIONSHIP, v.id.as_str(), Some(payload))
            .map_err(core)
    }
    fn delete_entity(&self, id: &EntityId) -> CoreResult<()> {
        self.0.stage(ENTITY, id.as_str(), None).map_err(core)
    }
    fn delete_relationship(&self, id: &RelationshipId) -> CoreResult<()> {
        self.0.stage(RELATIONSHIP, id.as_str(), None).map_err(core)
    }
    fn duplicate_count(&self, scope: &Scope) -> CoreResult<u32> {
        if !self.0.visible_scope(scope) {
            return Ok(0);
        }
        let mut counts = BTreeMap::new();
        for v in self.0.all::<Value>(ENTITY).map_err(core)? {
            if let Some(key) = v.get("_identity_key").and_then(Value::as_str) {
                *counts.entry(key.to_owned()).or_insert(0u32) += 1
            }
        }
        Ok(counts.values().map(|n| n.saturating_sub(1)).sum())
    }
}
fn page<T>(
    mut rows: Vec<T>,
    after: Option<Cursor>,
    limit: usize,
    id: impl Fn(&T) -> String,
) -> CoreResult<Page<T>> {
    if limit == 0 || limit > 500 {
        return Err(CoreError::InvalidRequest {
            reason: "invalid bounded maintenance page".into(),
        });
    }
    rows.retain(|v| after.as_ref().is_none_or(|a| id(v).as_str() > a.as_str()));
    let more = rows.len() > limit;
    rows.truncate(limit);
    let next = if more {
        rows.last().map(|v| Cursor::new(id(v)))
    } else {
        None
    };
    Ok(Page::new(rows, next))
}
#[async_trait]
impl GraphMaintenanceRepository for HelixStore {
    async fn list_entities(
        &self,
        scope: &Scope,
        f: &EntityFilter,
        after: Option<Cursor>,
        limit: usize,
    ) -> CoreResult<Page<KnowledgeEntity>> {
        let mut rows = Storage(self).entities(scope)?;
        rows.retain(|v| {
            (f.include_archived || v.archived_at.is_none())
                && f.graph_id
                    .as_ref()
                    .is_none_or(|g| v.graph_id.as_ref() == Some(g))
                && (f.kinds.is_empty() || f.kinds.contains(&v.kind))
                && f.min_confidence
                    .is_none_or(|m| v.provenance.confidence.unwrap_or(0.0) >= m)
                && f.source_id.as_ref().is_none_or(|id| {
                    v.provenance.source == id.as_str()
                        || v.source_refs
                            .iter()
                            .any(|r| r.target_id.as_deref() == Some(id.as_str()))
                })
        });
        page(rows, after, limit, |v| v.id.to_string())
    }
    async fn list_relationships(
        &self,
        scope: &Scope,
        f: &RelationshipFilter,
        after: Option<Cursor>,
        limit: usize,
    ) -> CoreResult<Page<KnowledgeRelationship>> {
        let mut rows = Storage(self).relationships(scope)?;
        rows.retain(|v| {
            (f.include_archived || v.archived_at.is_none())
                && f.graph_id
                    .as_ref()
                    .is_none_or(|g| v.graph_id.as_ref() == Some(g))
                && f.predicate.as_ref().is_none_or(|p| &v.predicate == p)
                && f.min_confidence
                    .is_none_or(|m| v.confidence.unwrap_or(0.0) >= m)
                && f.source_id.as_ref().is_none_or(|id| {
                    v.provenance.source == id.as_str()
                        || v.evidence
                            .iter()
                            .any(|r| r.target_id.as_deref() == Some(id.as_str()))
                })
        });
        page(rows, after, limit, |v| v.id.to_string())
    }
    async fn detect_candidates(
        &self,
        scope: &Scope,
        graph: Option<&KnowledgeGraphId>,
        policy: &MaintenancePolicy,
    ) -> CoreResult<Vec<MaintenanceCandidate>> {
        let mut entities = Storage(self).entities(scope)?;
        entities.retain(|v| {
            v.archived_at.is_none() && graph.is_none_or(|g| v.graph_id.as_ref() == Some(g))
        });
        let mut relationships = Storage(self).relationships(scope)?;
        relationships.retain(|v| {
            v.archived_at.is_none() && graph.is_none_or(|g| v.graph_id.as_ref() == Some(g))
        });
        let collisions = if policy.detect_duplicates {
            self.discover_collisions(
                scope,
                &EntityIdentityMode::ScopedKindAndNormalizedName {
                    normalization_version: engram_domain::NORMALIZATION_VERSION.into(),
                    include_graph: true,
                    match_aliases: false,
                },
            )
            .await?
        } else {
            vec![]
        };
        let mut findings = vec![];
        if policy.detect_unsupported {
            for g in self.all::<KnowledgeGraph>(GRAPH).map_err(core)? {
                if graph.is_none_or(|id| &g.id == id) {
                    for ontology in &g.ontology_refs {
                        if let Some(id) = &ontology.id {
                            findings.extend(self.validate_graph(&g.id, id, scope).await?);
                        }
                    }
                }
            }
        }
        Ok(engram_knowledge::detect_candidates(
            &entities,
            &relationships,
            &collisions,
            &findings,
            policy,
        ))
    }
    async fn build_plan(&self, request: MaintenancePlanRequest) -> CoreResult<MaintenancePlan> {
        self.check_scope(&request.scope).map_err(core)?;
        composition::build_plan(&Storage(self), request)
    }
    async fn apply_plan(
        &self,
        plan: &MaintenancePlan,
        mode: ApplyMode,
    ) -> CoreResult<MaintenanceApplyResult> {
        self.check_scope(&plan.scope).map_err(core)?;
        if mode == ApplyMode::Preview {
            return composition::apply_plan(&Storage(self), plan, mode);
        }
        let mut failed = None;
        let result = self.domain_atomic(|s| {
            let result = composition::apply_plan(&Storage(s), plan, mode)?;
            if result.failed > 0 || !result.verify_findings.is_empty() {
                failed = Some(result);
                return Err(CoreError::InvalidRequest {
                    reason: "maintenance verification rolled back".into(),
                });
            }
            Ok(result)
        });
        if let Some(result) = failed {
            Ok(result)
        } else {
            result
        }
    }
    async fn graph_health(
        &self,
        scope: &Scope,
        graph: Option<&KnowledgeGraphId>,
    ) -> CoreResult<MaintenanceHealth> {
        composition::graph_health(&Storage(self), scope, graph)
    }
}
