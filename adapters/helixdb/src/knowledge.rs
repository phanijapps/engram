//! Source-grounded records with exact inherited parent ownership.
use crate::{
    HelixStore,
    error::{Error, Result},
    records::*,
};
use async_trait::async_trait;
use engram_domain::*;
use engram_knowledge::KnowledgeRepository;
use engram_runtime::CoreResult;
impl HelixStore {
    pub(crate) fn require_source(&self, id: &SourceId) -> Result<()> {
        self.get::<KnowledgeSource>(SOURCE, &id.to_string())?
            .ok_or(Error::InvalidConfig("source parent absent in partition"))?;
        Ok(())
    }
    pub(crate) fn write_entity(&self, record: &KnowledgeEntity) -> Result<KnowledgeEntity> {
        self.check_scope(&record.scope)?;
        self.atomic(|s| {
            let mut payload = value(record)?;
            if let Some(prior) = s.get::<serde_json::Value>(ENTITY, &record.id.to_string())? {
                if let Some(key) = prior.get("_identity_key") {
                    payload["_identity_key"] = key.clone();
                }
            }
            s.stage(ENTITY, &record.id.to_string(), Some(payload))?;
            Ok(record.clone())
        })
    }
    pub(crate) fn write_relationship(
        &self,
        record: &KnowledgeRelationship,
    ) -> Result<KnowledgeRelationship> {
        self.check_scope(&record.scope)?;
        self.atomic(|s| {
            let subject = s.endpoint(&record.subject)?;
            let object = s.endpoint(&record.object)?;
            let mut v = value(record)?;
            v["_subject_endpoint"] = subject.into();
            v["_object_endpoint"] = object.into();
            v["_relationship_key"] =
                engram_knowledge::identity::compute_relationship_key(record).into();
            s.stage(RELATIONSHIP, &record.id.to_string(), Some(v))?;
            Ok(record.clone())
        })
    }
}
#[async_trait]
impl KnowledgeRepository for HelixStore {
    async fn put_source(&self, v: KnowledgeSource) -> CoreResult<KnowledgeSource> {
        self.check_scope(&v.scope).map_err(core)?;
        self.put(SOURCE, &v).map_err(core)
    }
    async fn get_source(
        &self,
        id: &SourceId,
        scope: &Scope,
    ) -> CoreResult<Option<KnowledgeSource>> {
        if !self.visible_scope(scope) {
            return Ok(None);
        }
        self.get(SOURCE, &id.to_string()).map_err(core)
    }
    async fn list_sources(&self, scope: &Scope) -> CoreResult<Vec<KnowledgeSource>> {
        if !self.visible_scope(scope) {
            return Ok(vec![]);
        }
        self.all(SOURCE).map_err(core)
    }
    async fn list_documents(&self, scope: &Scope) -> CoreResult<Vec<SourceDocument>> {
        if !self.visible_scope(scope) {
            return Ok(vec![]);
        }
        let rows = self.all::<SourceDocument>(DOCUMENT).map_err(core)?;
        for v in &rows {
            self.require_source(&v.source_id).map_err(core)?;
        }
        Ok(rows)
    }
    async fn put_document(&self, v: SourceDocument) -> CoreResult<SourceDocument> {
        self.atomic(|s| {
            s.require_source(&v.source_id)?;
            s.put(DOCUMENT, &v)
        })
        .map_err(core)
    }
    async fn put_chunk(&self, v: KnowledgeChunk) -> CoreResult<KnowledgeChunk> {
        self.atomic(|s| {
            s.require_source(&v.source_id)?;
            let parent: SourceDocument = s
                .get(DOCUMENT, &v.document_id.to_string())?
                .ok_or(Error::InvalidConfig("document parent absent in partition"))?;
            if parent.source_id != v.source_id {
                return Err(Error::InvalidConfig("chunk source differs from document"));
            }
            s.put(CHUNK, &v)
        })
        .map_err(core)
    }
    async fn get_chunk(&self, id: &ChunkId, scope: &Scope) -> CoreResult<Option<KnowledgeChunk>> {
        if !self.visible_scope(scope) {
            return Ok(None);
        }
        let row = self
            .get::<KnowledgeChunk>(CHUNK, &id.to_string())
            .map_err(core)?;
        if let Some(v) = &row {
            self.require_source(&v.source_id).map_err(core)?;
        }
        Ok(row)
    }
    async fn put_entity(&self, v: KnowledgeEntity) -> CoreResult<KnowledgeEntity> {
        self.write_entity(&v).map_err(core)
    }
    async fn put_relationship(
        &self,
        v: KnowledgeRelationship,
    ) -> CoreResult<KnowledgeRelationship> {
        self.write_relationship(&v).map_err(core)
    }
    async fn get_entity(
        &self,
        id: &EntityId,
        scope: &Scope,
    ) -> CoreResult<Option<KnowledgeEntity>> {
        if !self.visible_scope(scope) {
            return Ok(None);
        }
        self.get(ENTITY, &id.to_string()).map_err(core)
    }
    async fn get_relationship(
        &self,
        id: &RelationshipId,
        scope: &Scope,
    ) -> CoreResult<Option<KnowledgeRelationship>> {
        if !self.visible_scope(scope) {
            return Ok(None);
        }
        self.get(RELATIONSHIP, &id.to_string()).map_err(core)
    }
    async fn delete_entity(&self, id: &EntityId, scope: &Scope) -> CoreResult<bool> {
        if !self.visible_scope(scope) {
            return Ok(false);
        }
        self.remove(ENTITY, &id.to_string()).map_err(core)
    }
    async fn delete_relationship(&self, id: &RelationshipId, scope: &Scope) -> CoreResult<bool> {
        if !self.visible_scope(scope) {
            return Ok(false);
        }
        self.remove(RELATIONSHIP, &id.to_string()).map_err(core)
    }
    async fn list_entities_by_graph(
        &self,
        id: &KnowledgeGraphId,
        scope: &Scope,
    ) -> CoreResult<Vec<KnowledgeEntity>> {
        if !self.visible_scope(scope) {
            return Ok(vec![]);
        }
        self.filtered(ENTITY, "graphId", &id.to_string())
            .map_err(core)
    }
    async fn list_chunks_by_document(
        &self,
        id: &DocumentId,
        scope: &Scope,
    ) -> CoreResult<Vec<KnowledgeChunk>> {
        if !self.visible_scope(scope) {
            return Ok(vec![]);
        }
        self.filtered(CHUNK, "documentId", &id.to_string())
            .map_err(core)
    }
    async fn delete_document(&self, id: &DocumentId, scope: &Scope) -> CoreResult<bool> {
        if !self.visible_scope(scope) {
            return Ok(false);
        }
        self.remove(DOCUMENT, &id.to_string()).map_err(core)
    }
    async fn delete_chunk(&self, id: &ChunkId, scope: &Scope) -> CoreResult<bool> {
        if !self.visible_scope(scope) {
            return Ok(false);
        }
        self.remove(CHUNK, &id.to_string()).map_err(core)
    }
    async fn list_entities_by_source_ref(
        &self,
        target: &str,
        scope: &Scope,
    ) -> CoreResult<Vec<KnowledgeEntity>> {
        if !self.visible_scope(scope) {
            return Ok(vec![]);
        }
        Ok(self
            .all::<KnowledgeEntity>(ENTITY)
            .map_err(core)?
            .into_iter()
            .filter(|v| {
                v.source_refs.iter().any(|r| {
                    r.target_type == EvidenceTargetType::Source
                        && r.target_id.as_deref() == Some(target)
                })
            })
            .collect())
    }
    async fn list_relationships_by_source_ref(
        &self,
        target: &str,
        scope: &Scope,
    ) -> CoreResult<Vec<KnowledgeRelationship>> {
        if !self.visible_scope(scope) {
            return Ok(vec![]);
        }
        Ok(self
            .all::<KnowledgeRelationship>(RELATIONSHIP)
            .map_err(core)?
            .into_iter()
            .filter(|v| {
                v.evidence.iter().any(|r| {
                    r.target_type == EvidenceTargetType::Source
                        && r.target_id.as_deref() == Some(target)
                })
            })
            .collect())
    }
    async fn restore_entity(&self, id: &EntityId, scope: &Scope) -> CoreResult<bool> {
        if !self.visible_scope(scope) {
            return Ok(false);
        }
        self.atomic(|s| {
            let Some(mut v) = s.get::<KnowledgeEntity>(ENTITY, &id.to_string())? else {
                return Ok(false);
            };
            if v.archived_at.is_none() {
                return Ok(false);
            }
            v.archived_at = None;
            s.write_entity(&v)?;
            Ok(true)
        })
        .map_err(core)
    }
    async fn restore_relationship(&self, id: &RelationshipId, scope: &Scope) -> CoreResult<bool> {
        if !self.visible_scope(scope) {
            return Ok(false);
        }
        self.atomic(|s| {
            let Some(mut v) = s.get::<KnowledgeRelationship>(RELATIONSHIP, &id.to_string())? else {
                return Ok(false);
            };
            if v.archived_at.is_none() {
                return Ok(false);
            }
            v.archived_at = None;
            s.write_relationship(&v)?;
            Ok(true)
        })
        .map_err(core)
    }
}
