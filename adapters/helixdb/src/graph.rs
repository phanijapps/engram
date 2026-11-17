//! Graph identity, directed traversal and atomic storage cascades.
use crate::{HelixStore, error::Error, records::*, transport::Operation};
use async_trait::async_trait;
use engram_domain::*;
use engram_knowledge::KnowledgeGraphRepository;
use engram_runtime::CoreResult;
use helix_db::dsl::prelude::*;
#[async_trait]
impl KnowledgeGraphRepository for HelixStore {
    async fn put_graph(&self, v: KnowledgeGraph) -> CoreResult<KnowledgeGraph> {
        self.check_scope(&v.scope).map_err(core)?;
        self.put(GRAPH, &v).map_err(core)
    }
    async fn get_graph(
        &self,
        id: &KnowledgeGraphId,
        scope: &Scope,
    ) -> CoreResult<Option<KnowledgeGraph>> {
        if !self.visible_scope(scope) {
            return Ok(None);
        }
        self.get(GRAPH, &id.to_string()).map_err(core)
    }
    async fn neighbors(
        &self,
        graph: &KnowledgeGraphId,
        node: &EntityId,
        scope: &Scope,
        limit: Option<u32>,
    ) -> CoreResult<Vec<KnowledgeRelationship>> {
        if !self.visible_scope(scope) {
            return Ok(vec![]);
        }
        if self
            .get::<KnowledgeGraph>(GRAPH, &graph.to_string())
            .map_err(core)?
            .is_none()
        {
            return Err(missing("knowledge_graph", graph));
        }
        let pending = self
            .session
            .as_ref()
            .is_some_and(|s| s.pending.lock().is_ok_and(|p| p.is_some()));
        let mut result = if pending {
            self.filtered::<KnowledgeRelationship>(RELATIONSHIP, "graphId", &graph.to_string())
                .map_err(core)?
                .into_iter()
                .filter(|r| r.subject.id.as_ref() == Some(node))
                .collect::<Vec<_>>()
        } else {
            let partition = self.inner.partition.discriminator().map_err(core)?;
            let query = QueryRequest::read(
                read_batch()
                    .var_as(
                        "edges",
                        g().n_with_label_where(
                            ENDPOINT,
                            SourcePredicate::and(vec![
                                SourcePredicate::eq("partition", partition.clone()),
                                SourcePredicate::eq("entity_id", node.to_string()),
                            ]),
                        )
                        .out_e(Some(EDGE))
                        .where_(Predicate::and(vec![
                            Predicate::eq("partition", partition.clone()),
                            Predicate::eq("graphId", graph.to_string()),
                            Predicate::eq("subject_id", node.to_string()),
                        ]))
                        .value_map(Some(vec!["partition", "relationship_id"])),
                    )
                    .returning(["edges"]),
            );
            let response = self
                .inner
                .transport
                .execute(&Operation::new(query).map_err(core)?)
                .map_err(core)?;
            let mut rows = vec![];
            for edge in crate::schema::rows(&response, "edges").map_err(core)? {
                if edge["partition"] != partition {
                    return Err(core(Error::IncompatibleSchema));
                }
                let id = edge["relationship_id"]
                    .as_str()
                    .ok_or_else(|| core(Error::IncompatibleSchema))?;
                let row: KnowledgeRelationship = self
                    .get(RELATIONSHIP, id)
                    .map_err(core)?
                    .ok_or_else(|| core(Error::IncompatibleSchema))?;
                if row.scope != *scope
                    || row.graph_id.as_ref() != Some(graph)
                    || row.subject.id.as_ref() != Some(node)
                {
                    return Err(core(Error::IncompatibleSchema));
                }
                rows.push(row);
            }
            rows
        };
        result.retain(|r| r.archived_at.is_none());
        result.sort_by_key(|r| r.id.to_string());
        if let Some(n) = limit {
            result.truncate(n as usize);
        }
        Ok(result)
    }
    async fn delete_graph(&self, id: &KnowledgeGraphId, scope: &Scope) -> CoreResult<bool> {
        if !self.visible_scope(scope) {
            return Ok(false);
        }
        self.atomic(|s| {
            let found = s.get::<KnowledgeGraph>(GRAPH, &id.to_string())?.is_some();
            for family in [ENTITY, RELATIONSHIP, UNRESOLVED] {
                for v in s.select_values(family, Some(("graphId", &id.to_string())))? {
                    s.stage(family, crate::records::id(&v)?, None)?;
                }
            }
            s.stage(GRAPH, &id.to_string(), None)?;
            Ok(found)
        })
        .map_err(core)
    }
    async fn list_graphs_by_source(
        &self,
        scope: &Scope,
        key: &str,
    ) -> CoreResult<Vec<KnowledgeGraph>> {
        if !self.visible_scope(scope) {
            return Ok(vec![]);
        }
        self.filtered(GRAPH, "stableSourceKey", key).map_err(core)
    }
    async fn put_unresolved_refs(&self, refs: Vec<UnresolvedReference>) -> CoreResult<()> {
        self.atomic(|s| {
            for v in &refs {
                s.check_scope(&v.scope)?;
                s.put(UNRESOLVED, v)?;
            }
            Ok(())
        })
        .map_err(core)
    }
    async fn list_unresolved_refs(
        &self,
        scope: &Scope,
        status: UnresolvedReferenceStatus,
    ) -> CoreResult<Vec<UnresolvedReference>> {
        if !self.visible_scope(scope) {
            return Ok(vec![]);
        }
        let status = serde_json::to_value(status).map_err(|_| core(Error::Storage))?;
        self.filtered(
            UNRESOLVED,
            "status",
            status.as_str().ok_or_else(|| core(Error::Storage))?,
        )
        .map_err(core)
    }
    async fn update_unresolved_status(
        &self,
        id: &Id,
        status: UnresolvedReferenceStatus,
        scope: &Scope,
    ) -> CoreResult<()> {
        if !self.visible_scope(scope) {
            return Ok(());
        }
        self.atomic(|s| {
            let Some(mut v) = s.get::<UnresolvedReference>(UNRESOLVED, &id.to_string())? else {
                return Ok(());
            };
            v.status = status;
            v.updated_at = Some(chrono::Utc::now());
            s.put(UNRESOLVED, &v)?;
            Ok(())
        })
        .map_err(core)
    }
    async fn delete_unresolved_for_graph(
        &self,
        id: &KnowledgeGraphId,
        scope: &Scope,
    ) -> CoreResult<()> {
        if !self.visible_scope(scope) {
            return Ok(());
        }
        self.atomic(|s| {
            for v in s.select_values(UNRESOLVED, Some(("graphId", &id.to_string())))? {
                s.stage(UNRESOLVED, crate::records::id(&v)?, None)?;
            }
            Ok(())
        })
        .map_err(core)
    }
}
