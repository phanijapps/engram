//! Request-bound source ownership and canonical segment resolution.
use crate::EngramProvider;
use engram_domain::*;
use engram_knowledge::{
    ResolvedCodeSegment, code_source_retrieval_authorized, document_head_retrieval_authorized,
};
use engram_runtime::{CoreError, CoreResult};
use std::collections::{HashMap, HashSet};

pub(super) struct Resolver<'a> {
    pub provider: &'a EngramProvider,
    pub request: &'a RetrievalRequest,
    pub representation: &'a str,
    sources: Vec<CodeSourceGeneration>,
    allowed: HashSet<usize>,
    owners: HashMap<String, Vec<usize>>,
    file_segments: HashMap<usize, Vec<CodeEmbeddingSegment>>,
    documents: HashMap<String, Vec<KnowledgeChunk>>,
    pub reindex_required: bool,
    pub unavailable_sources: u32,
}
fn key(kind: CodeSourceRecordKind, id: &str) -> String {
    format!("{kind:?}:{id}")
}
impl<'a> Resolver<'a> {
    pub async fn new(
        provider: &'a EngramProvider,
        request: &'a RetrievalRequest,
        representation: &'a str,
        repository: Option<&str>,
    ) -> CoreResult<Self> {
        let knowledge = provider.require_knowledge()?;
        let port = knowledge
            .code_sources()
            .ok_or_else(|| CoreError::ProviderUnavailable {
                provider: "code source authority".into(),
            })?;
        let heads = knowledge
            .document_heads()
            .ok_or_else(|| CoreError::ProviderUnavailable {
                provider: "current document authority".into(),
            })?;
        let sources = port.list_code_sources(&request.scope).await?;
        let mut allowed = HashSet::new();
        let mut owners: HashMap<String, Vec<usize>> = HashMap::new();
        let mut reindex_required = false;
        let mut unavailable_sources = 0;
        let now = chrono::Utc::now();
        for (index, source) in sources.iter().enumerate() {
            for record in &source.records {
                owners
                    .entry(key(record.kind, &record.id))
                    .or_default()
                    .push(index);
            }
            if source.file.scope != request.scope
                || repository.is_some_and(|repo| repo != source.file.repository)
                || !code_source_retrieval_authorized(
                    source,
                    &request.requester,
                    &request.scope,
                    now,
                )
            {
                continue;
            }
            if source.state != CodeSourceState::Active {
                unavailable_sources += 1;
                continue;
            }
            let Some(head) = heads
                .get_document_head(&DocumentHeadKey {
                    scope: source.file.scope.clone(),
                    repository: source.file.repository.clone(),
                    source_key: source.file.path.clone(),
                })
                .await?
            else {
                unavailable_sources += 1;
                continue;
            };
            if head.generation == source.generation
                && head.attempt_id == source.attempt_id
                && head.source_hash == source.source_hash
                && head.document_id == source.document_id
                && head.policy == source.policy
                && document_head_retrieval_authorized(
                    &head,
                    &request.scope,
                    &request.requester,
                    now,
                )
            {
                allowed.insert(index);
                let inventory = provider
                    .code_segments()
                    .ok_or_else(|| CoreError::ProviderUnavailable {
                        provider: "code segment authority".into(),
                    })?
                    .code_segment_inventory(&source.file, representation)
                    .await?;
                if !inventory.is_some_and(|lease| {
                    lease.inventory.state == CodeSourceState::Active
                        && lease.inventory.attempt_id == source.attempt_id
                        && lease.inventory.generation == source.generation
                }) {
                    reindex_required = true;
                }
            } else {
                unavailable_sources += 1;
            }
        }
        Ok(Self {
            provider,
            request,
            representation,
            sources,
            allowed,
            owners,
            file_segments: HashMap::new(),
            documents: HashMap::new(),
            reindex_required,
            unavailable_sources,
        })
    }
    pub fn owns(&self, kind: CodeSourceRecordKind, id: &str) -> bool {
        self.owners
            .get(&key(kind, id))
            .is_some_and(|owners| owners.iter().any(|i| self.allowed.contains(i)))
    }
    pub async fn segment(&self, id: &str) -> CoreResult<Option<ResolvedCodeSegment>> {
        let Some(resolved) = self
            .provider
            .code_segments()
            .ok_or_else(|| CoreError::ProviderUnavailable {
                provider: "code segment authority".into(),
            })?
            .resolve_code_segment(
                id,
                &self.request.scope,
                &self.request.requester,
                self.representation,
            )
            .await?
        else {
            return Ok(None);
        };
        Ok(self
            .owns(CodeSourceRecordKind::Chunk, resolved.parent.id.as_str())
            .then_some(resolved))
    }
    pub async fn parent(&mut self, id: &str, cap: usize) -> CoreResult<Vec<ResolvedCodeSegment>> {
        if cap == 0 {
            return Ok(vec![]);
        }
        if !self.owns(CodeSourceRecordKind::Chunk, id) {
            return Ok(vec![]);
        }
        let owners = self.owners[&key(CodeSourceRecordKind::Chunk, id)].clone();
        let mut result = Vec::new();
        let mut seen = HashSet::new();
        for owner in owners
            .into_iter()
            .filter(|i| self.allowed.contains(i))
            .collect::<Vec<_>>()
        {
            if !self.file_segments.contains_key(&owner) {
                let set = self
                    .provider
                    .code_segments()
                    .ok_or_else(|| CoreError::ProviderUnavailable {
                        provider: "code segment authority".into(),
                    })?
                    .load_current_code_segments(
                        &self.sources[owner].file,
                        self.representation,
                        &self.request.requester,
                    )
                    .await?;
                let segments = match set {
                    Some(set) if set.lease.inventory.state == CodeSourceState::Active => {
                        set.segments
                    }
                    _ => {
                        self.reindex_required = true;
                        vec![]
                    }
                };
                self.file_segments.insert(owner, segments);
            }
            let ids: Vec<_> = self.file_segments[&owner]
                .iter()
                .filter(|s| s.parent_chunk_id.as_str() == id)
                .take(cap)
                .map(|s| s.id.clone())
                .collect();
            for id in ids {
                if seen.insert(id.clone())
                    && let Some(resolved) = self.segment(&id).await?
                {
                    result.push(resolved);
                    if result.len() == cap {
                        return Ok(result);
                    }
                }
            }
        }
        Ok(result)
    }
    pub async fn entity(&mut self, id: &str, cap: usize) -> CoreResult<Vec<ResolvedCodeSegment>> {
        if cap == 0 {
            return Ok(vec![]);
        }
        if !self.owns(CodeSourceRecordKind::Entity, id) {
            return Ok(vec![]);
        }
        let owners = self.owners[&key(CodeSourceRecordKind::Entity, id)].clone();
        let mut parents = Vec::new();
        for owner in owners
            .into_iter()
            .filter(|i| self.allowed.contains(i))
            .collect::<Vec<_>>()
        {
            if let Some(document) = self.sources[owner].document_id.clone() {
                if !self.documents.contains_key(document.as_str()) {
                    let chunks = self
                        .provider
                        .require_knowledge()?
                        .list_chunks_by_document(&document, &self.request.scope)
                        .await?;
                    self.documents.insert(document.to_string(), chunks);
                }
                parents.extend(
                    self.documents[document.as_str()]
                        .iter()
                        .filter(|c| {
                            c.entities
                                .iter()
                                .any(|e| e.id.as_ref().is_some_and(|entity| entity.as_str() == id))
                        })
                        .map(|c| c.id.to_string()),
                );
            }
        }
        parents.sort();
        parents.dedup();
        let mut result = Vec::new();
        for parent in parents {
            result.extend(
                self.parent(&parent, cap.saturating_sub(result.len()))
                    .await?,
            );
            if result.len() == cap {
                break;
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identical_shared_record_remains_available_through_an_independent_current_owner() {
        let provider = crate::EngramProviderBuilder::new(crate::CapabilityReport::new(
            CapabilityState::Supported,
        ))
        .build();
        let request: RetrievalRequest = serde_json::from_value(serde_json::json!({
            "query":"q", "scope":{"tenant":"t"}, "requester":{"actor":{"id":"a","kind":"agent"}}
        }))
        .unwrap();
        // Owner 1 represents a private, updating or different-repository source.
        // Only owner 0 supplied an independently authorized current head.
        let resolver = Resolver {
            provider: &provider,
            request: &request,
            representation: "r",
            sources: vec![],
            allowed: HashSet::from([0]),
            owners: HashMap::from([
                (key(CodeSourceRecordKind::Entity, "shared"), vec![0, 1]),
                (key(CodeSourceRecordKind::Entity, "denied-only"), vec![1]),
            ]),
            file_segments: HashMap::new(),
            documents: HashMap::new(),
            reindex_required: false,
            unavailable_sources: 0,
        };
        assert!(resolver.owns(CodeSourceRecordKind::Entity, "shared"));
        assert!(!resolver.owns(CodeSourceRecordKind::Entity, "denied-only"));
        assert!(!resolver.owns(CodeSourceRecordKind::Entity, "unknown"));
    }
}
