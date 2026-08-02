//! Community-summary retrieval behind the `RetrievalIndex` port.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use engram_domain::{
    AllowedUse, DeleteMode, EntityRef, FusionStrategy, FusionTrace, KnowledgeEntity, Policy,
    RerankStrategy, Retention, RetrievalRequest, RetrievalResult, RetrievalScore,
    RetrievalTargetType, Sensitivity, Visibility,
};
use engram_graph_analytics::communities;
use engram_retrieval::{GraphCache, GraphSnapshot, RetrievalIndex};
use engram_runtime::CoreResult;
use engram_store_associative_graph::GraphRelationshipSource;
use futures::future::try_join;

const SOURCE: &str = "community_summary";

/// Community-summary retrieval: detects communities over the knowledge graph +
/// ranks them by lexical query relevance, returning the top community's members.
///
/// An optional shared [`GraphCache`] lets the lane skip the per-query store
/// reload on a cache hit (the in-scope entities + relationships are served from
/// a materialized snapshot). On a miss it loads from the source and populates
/// the cache, so the sibling graph lanes sharing the same cache hit next.
pub struct CommunitySummaryIndex {
    source: Arc<dyn GraphRelationshipSource>,
    default_limit: u32,
    cache: Option<Arc<dyn GraphCache>>,
}

impl CommunitySummaryIndex {
    pub fn new(source: Arc<dyn GraphRelationshipSource>) -> Self {
        Self::with_default_limit(source, 20)
    }
    pub fn with_default_limit(
        source: Arc<dyn GraphRelationshipSource>,
        default_limit: u32,
    ) -> Self {
        Self {
            source,
            default_limit,
            cache: None,
        }
    }

    /// Attaches a shared graph cache so the lane serves the in-scope entities +
    /// relationships from a materialized snapshot on a cache hit, populating it
    /// on a miss.
    pub fn with_cache(
        source: Arc<dyn GraphRelationshipSource>,
        default_limit: u32,
        cache: Arc<dyn GraphCache>,
    ) -> Self {
        Self {
            source,
            default_limit,
            cache: Some(cache),
        }
    }
}

#[async_trait]
impl RetrievalIndex for CommunitySummaryIndex {
    async fn retrieve_candidates(
        &self,
        request: &RetrievalRequest,
    ) -> CoreResult<Vec<RetrievalResult>> {
        let limit = request
            .limit
            .or_else(|| request.budget.as_ref().and_then(|b| b.max_items))
            .unwrap_or(self.default_limit);
        let scope = &request.scope;

        // Cache hit: serve the in-scope entities + relationships from the
        // materialized snapshot, skipping the per-query store reload. A miss
        // (or no cache) loads from the source as before and populates the cache
        // (this lane reads BOTH entities and relationships, so it always stores
        // a full snapshot).
        let (entities, relationships) = if let Some(cache) = &self.cache
            && let Some(snap) = cache.get(scope).await
        {
            (snap.entities.clone(), snap.relationships.clone())
        } else {
            let (ents, rels) = try_join(
                self.source.entities(scope),
                self.source.relationships(scope),
            )
            .await?;
            if let Some(cache) = &self.cache {
                cache
                    .put(
                        scope,
                        Arc::new(GraphSnapshot {
                            entities: ents.clone(),
                            relationships: rels.clone(),
                            community_labels: None,
                        }),
                    )
                    .await;
            }
            (ents, rels)
        };

        let by_key: HashMap<&str, &KnowledgeEntity> =
            entities.iter().map(|e| (e.id.as_str(), e)).collect();
        let display_names: HashMap<&str, &str> = entities
            .iter()
            .map(|e| (e.id.as_str(), e.name.as_str()))
            .collect();

        // Build directed edges over all predicates (entity_key = id only).
        let edges: Vec<(String, String)> = relationships
            .iter()
            .filter_map(|r| {
                let s = entity_key(&r.subject)?;
                let o = entity_key(&r.object)?;
                Some((s, o))
            })
            .collect();
        if edges.is_empty() {
            return Ok(Vec::new());
        }

        // Detect communities (deterministic single-level Louvain). The Louvain
        // pass over ~57k edges dominates recall latency (~3.6s) when run on
        // every query, so the labels are cached in the graph snapshot: the first
        // query after a miss (or invalidation) computes + stores them; every
        // subsequent query reuses them until the graph changes. Entities and
        // relationships are already cached (above), so this only adds the label
        // map to the snapshot on the warm-up query.
        let labels = if let Some(cache) = &self.cache
            && let Some(snap) = cache.get(scope).await
            && let Some(cached) = &snap.community_labels
        {
            cached.clone()
        } else {
            let computed = communities(&edges, 20);
            if let Some(cache) = &self.cache {
                cache
                    .put(
                        scope,
                        Arc::new(GraphSnapshot {
                            entities: entities.clone(),
                            relationships: relationships.clone(),
                            community_labels: Some(computed.clone()),
                        }),
                    )
                    .await;
            }
            computed
        };

        // Invert: label → sorted member keys.
        let mut groups: HashMap<usize, Vec<String>> = HashMap::new();
        for (key, label) in &labels {
            groups.entry(*label).or_default().push(key.clone());
        }
        for members in groups.values_mut() {
            members.sort();
        }

        // Collect intra-community predicates in a SINGLE pass over the
        // relationships (O(R)) instead of rescanning every relationship once
        // per community (O(communities × R)). On a ~57k-edge / ~4k-community
        // graph the previous per-community rescanning dominated recall latency
        // (~60s); this collapses it to milliseconds. Output is byte-for-byte
        // identical: a relationship contributes its predicate to a community's
        // summary exactly when both endpoints share that community's label,
        // which is the same predicate set the old per-community membership test
        // produced (a community's members ARE precisely its label's entities).
        let key_to_label: HashMap<&str, usize> =
            labels.iter().map(|(k, l)| (k.as_str(), *l)).collect();
        let mut predicates_by_comm: HashMap<usize, Vec<&str>> = HashMap::new();
        for r in relationships.iter() {
            let (Some(s_id), Some(o_id)) = (
                r.subject.id.as_ref().map(|id| id.as_str()),
                r.object.id.as_ref().map(|id| id.as_str()),
            ) else {
                continue;
            };
            let (Some(&s_label), Some(&o_label)) = (key_to_label.get(s_id), key_to_label.get(o_id))
            else {
                continue;
            };
            if s_label == o_label {
                predicates_by_comm
                    .entry(s_label)
                    .or_default()
                    .push(r.predicate.as_str());
            }
        }

        // Build a text summary per community + rank by query token overlap.
        let tokens = tokenize(&request.query);
        let mut ranked: Vec<(usize, Vec<String>, f64)> = groups
            .into_iter()
            .map(|(label, members)| {
                let names: Vec<&str> = members
                    .iter()
                    .filter_map(|k| display_names.get(k.as_str()).copied())
                    .collect();
                let mut predicates = predicates_by_comm.remove(&label).unwrap_or_default();
                predicates.sort();
                predicates.dedup();
                let summary = format!(
                    "Community: {}; edges: {}",
                    names.join(", "),
                    predicates.join(", ")
                );
                let score = if tokens.is_empty() {
                    0.0
                } else {
                    let lower = summary.to_ascii_lowercase();
                    tokens.iter().filter(|t| lower.contains(t.as_str())).count() as f64
                };
                (label, members, score)
            })
            .collect();
        ranked.sort_by(|a, b| {
            b.2.partial_cmp(&a.2)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });

        if ranked.is_empty() {
            return Ok(Vec::new());
        }
        let top_members = &ranked[0].1;
        let results = top_members
            .iter()
            .take(limit.max(1) as usize)
            .filter_map(|key| by_key.get(key.as_str()).map(|e| build_result(key, e)))
            .collect();
        Ok(results)
    }
}

/// Resolves an entity reference to its entity-id key, or `None` if no id.
fn entity_key(reference: &EntityRef) -> Option<String> {
    reference.id.as_ref().map(|id| id.as_str().to_owned())
}

/// Lowercased alphanumeric query tokens.
fn tokenize(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .map(|t| {
            t.trim_matches(|c: char| !c.is_alphanumeric())
                .to_ascii_lowercase()
        })
        .filter(|t| !t.is_empty())
        .collect()
}

/// Builds a `RetrievalResult` for a community-member entity.
fn build_result(key: &str, entity: &KnowledgeEntity) -> RetrievalResult {
    RetrievalResult {
        id: format!("community-summary-entity-{key}"),
        target_type: RetrievalTargetType::Entity,
        target_id: key.to_owned(),
        content: entity.name.clone(),
        score: RetrievalScore {
            total: 1.0,
            relevance: Some(1.0),
            recency: None,
            confidence: None,
            cue_match: None,
            hierarchical_fit: None,
            policy_fit: Some(1.0),
        },
        provenance: entity.provenance.clone(),
        policy: Policy {
            visibility: Visibility::Workspace,
            retention: Retention::Durable,
            sensitivity: Some(Sensitivity::Low),
            allowed_uses: vec![AllowedUse::Retrieval],
            expires_at: None,
            delete_mode: Some(DeleteMode::Tombstone),
        },
        explanation: None,
        fusion_trace: Some(FusionTrace {
            query_id: None,
            vector_index: None,
            embedding_time_ms: None,
            search_time_ms: None,
            source: SOURCE.to_owned(),
            source_rank: Some(1),
            source_score: Some(1.0),
            score: None,
            rank: None,
            fusion_strategy: Some(FusionStrategy::None),
            fusion_score: Some(1.0),
            rerank_strategy: Some(RerankStrategy::None),
            rerank_score: Some(1.0),
            discard_reason: None,
            deduplicated_with: Vec::new(),
        }),
        metadata: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engram_domain::{
        Actor, ActorKind, EntityKind, EntityRef, Id, KnowledgeEntity, KnowledgeRelationship,
        Provenance, Requester, RetrievalMode, RetrievalRequest, Scope,
    };
    use engram_retrieval::RetrievalIndex;
    use futures::executor::block_on;

    struct StubSource {
        entities: Vec<KnowledgeEntity>,
        relationships: Vec<KnowledgeRelationship>,
    }

    #[async_trait]
    impl GraphRelationshipSource for StubSource {
        async fn entities(&self, _scope: &Scope) -> CoreResult<Vec<KnowledgeEntity>> {
            Ok(self.entities.clone())
        }
        async fn relationships(&self, _scope: &Scope) -> CoreResult<Vec<KnowledgeRelationship>> {
            Ok(self.relationships.clone())
        }
    }

    fn scope() -> Scope {
        Scope {
            tenant: "t".to_owned(),
            subject: None,
            workspace: None,
            session: None,
            environment: None,
        }
    }

    fn ent(id: &str, name: &str) -> KnowledgeEntity {
        KnowledgeEntity {
            id: Id::from(id),
            graph_id: None,
            kind: EntityKind::Concept,
            name: name.to_owned(),
            aliases: Vec::new(),
            scope: scope(),
            source_refs: Vec::new(),
            concept_refs: Vec::new(),
            ontology_class_refs: Vec::new(),
            provenance: Provenance {
                source: "test".to_owned(),
                actor: Actor {
                    id: Id::from("test"),
                    kind: ActorKind::Agent,
                    display_name: None,
                    metadata: None,
                },
                observed_at: chrono::Utc::now(),
                evidence: Vec::new(),
                derivations: Vec::new(),
                confidence: Some(1.0),
                method: Some("test".to_owned()),
            },
            created_at: chrono::Utc::now(),
            updated_at: None,
            valid_from: None,
            valid_until: None,
            metadata: None,
        }
    }

    fn rel(s: &str, o: &str) -> KnowledgeRelationship {
        KnowledgeRelationship {
            id: Id::from(format!("r-{s}-{o}")),
            graph_id: None,
            subject: EntityRef {
                id: Some(Id::from(s)),
                kind: None,
                name: Some(s.to_owned()),
                aliases: Vec::new(),
            },
            predicate: "related_to".to_owned(),
            object: EntityRef {
                id: Some(Id::from(o)),
                kind: None,
                name: Some(o.to_owned()),
                aliases: Vec::new(),
            },
            scope: scope(),
            evidence: Vec::new(),
            confidence: None,
            provenance: Provenance {
                source: "test".to_owned(),
                actor: Actor {
                    id: Id::from("test"),
                    kind: ActorKind::Agent,
                    display_name: None,
                    metadata: None,
                },
                observed_at: chrono::Utc::now(),
                evidence: Vec::new(),
                derivations: Vec::new(),
                confidence: Some(1.0),
                method: Some("test".to_owned()),
            },
            created_at: chrono::Utc::now(),
            updated_at: None,
        }
    }

    fn request(query: &str) -> RetrievalRequest {
        RetrievalRequest {
            query: query.to_owned(),
            scope: scope(),
            requester: Requester {
                actor: Actor {
                    id: Id::from("test"),
                    kind: ActorKind::Agent,
                    display_name: None,
                    metadata: None,
                },
                roles: Vec::new(),
                permissions: Vec::new(),
                on_behalf_of: None,
            },
            modes: vec![RetrievalMode::Graph],
            filters: None,
            cues: Vec::new(),
            limit: None,
            budget: None,
            include_explanations: Some(true),
        }
    }

    #[test]
    fn returns_top_community_entities_for_query() {
        let source = Arc::new(StubSource {
            entities: vec![
                ent("a", "Alice"),
                ent("b", "Bob"),
                ent("c", "Carol"),
                ent("d", "Dave"),
                ent("e", "Eve"),
            ],
            relationships: vec![rel("a", "b"), rel("b", "c"), rel("d", "e")],
        });
        let index = CommunitySummaryIndex::new(source);

        // Query "Alice" → matches community {a,b,c}.
        let results = block_on(index.retrieve_candidates(&request("Alice"))).unwrap();
        let ids: Vec<&str> = results.iter().map(|r| r.target_id.as_str()).collect();
        assert!(ids.contains(&"a") && ids.contains(&"b") && ids.contains(&"c"));
        assert!(!ids.contains(&"d") && !ids.contains(&"e"));
        assert!(
            results
                .iter()
                .all(|r| r.fusion_trace.as_ref().unwrap().source == "community_summary")
        );
    }

    #[test]
    fn different_query_selects_different_community() {
        let source = Arc::new(StubSource {
            entities: vec![
                ent("a", "Alice"),
                ent("b", "Bob"),
                ent("d", "Dave"),
                ent("e", "Eve"),
            ],
            relationships: vec![rel("a", "b"), rel("d", "e")],
        });
        let index = CommunitySummaryIndex::new(source);

        let results = block_on(index.retrieve_candidates(&request("Dave"))).unwrap();
        let ids: Vec<&str> = results.iter().map(|r| r.target_id.as_str()).collect();
        assert!(ids.contains(&"d") && ids.contains(&"e"));
        assert!(!ids.contains(&"a"));
    }

    #[test]
    fn empty_graph_returns_empty() {
        let source = Arc::new(StubSource {
            entities: vec![ent("a", "Alice")],
            relationships: Vec::new(),
        });
        let index = CommunitySummaryIndex::new(source);
        let results = block_on(index.retrieve_candidates(&request("Alice"))).unwrap();
        assert!(results.is_empty());
    }
}
