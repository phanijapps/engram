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
    /// Bound-code eligibility (task_014): when wired, communities are detected
    /// over the eligible universe only — stale/retired/legacy-unbound code
    /// entities leave the member set BEFORE Louvain, so a summary that touched
    /// a retired generation is recomputed from the surviving eligible code and
    /// non-code inputs rather than served stale or dropped wholesale. The warm
    /// snapshot + cached labels are epoch-stamped: a retirement bumps the
    /// epoch, which invalidates both.
    eligibility: Option<Arc<dyn engram_retrieval::CodeEligibilityResolver>>,
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
            eligibility: None,
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
            eligibility: None,
        }
    }

    /// [`Self::with_cache`] with bound-code eligibility wired (task_014):
    /// summaries touching a retired generation are recomputed from the
    /// surviving inputs before serving, and warm snapshots/labels from a
    /// retired epoch are misses.
    pub fn with_cache_and_eligibility(
        source: Arc<dyn GraphRelationshipSource>,
        default_limit: u32,
        cache: Arc<dyn GraphCache>,
        eligibility: Arc<dyn engram_retrieval::CodeEligibilityResolver>,
    ) -> Self {
        Self {
            source,
            default_limit,
            cache: Some(cache),
            eligibility: Some(eligibility),
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

        // Bound-code eligibility (task_014): resolve the published generation
        // set once. The observed epoch gates the warm snapshot AND the cached
        // Louvain labels — a retirement bumps the epoch, so both recompute.
        let (view, observed_epoch) = match &self.eligibility {
            Some(resolver) => {
                let resolved = resolver.resolve(scope).await?;
                match resolved {
                    Some(eligibility) => {
                        let epoch = eligibility.epoch;
                        (
                            Some(engram_retrieval::BoundCodeView::from_eligibility(
                                &eligibility,
                            )),
                            Some(epoch),
                        )
                    }
                    None => (Some(engram_retrieval::BoundCodeView::none_bound()), None),
                }
            }
            None => (None, None),
        };

        // Cache hit: serve the in-scope entities + relationships from the
        // materialized snapshot, skipping the per-query store reload. A miss
        // (or no cache) loads from the source as before and populates the cache
        // (this lane reads BOTH entities and relationships, so it always stores
        // a full snapshot). An eligibility-wired lane only accepts a snapshot
        // stamped with the CURRENT epoch (see [`GraphSnapshot::current_for_epoch`]).
        let (entities, relationships) = if let Some(cache) = &self.cache
            && let Some(snap) = cache.get(scope).await
            && snap.current_for_epoch(observed_epoch)
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
                            eligibility_epoch: observed_epoch,
                        }),
                    )
                    .await;
            }
            (ents, rels)
        };

        // Lineage partition BEFORE community detection (task_014): ineligible
        // code entities leave the member set and every edge touching them is
        // dropped, so communities — and their summaries — are computed from
        // eligible code + surviving non-code inputs only. A mixed community is
        // recomputed (smaller), never served stale and never dropped wholesale.
        let (entities, relationships) = match &view {
            Some(view) => {
                let mut removed_ids: std::collections::HashSet<String> = Default::default();
                let entities = entities
                    .into_iter()
                    .filter(|entity| {
                        let allowed =
                            view.allows(entity.metadata.as_ref(), entity_is_unstamped_code(entity));
                        if !allowed {
                            removed_ids.insert(entity.id.to_string());
                        }
                        allowed
                    })
                    .collect::<Vec<_>>();
                let relationships = relationships
                    .into_iter()
                    .filter(|relationship| {
                        let touches_removed = [
                            relationship.subject.id.as_ref().map(|id| id.as_str()),
                            relationship.object.id.as_ref().map(|id| id.as_str()),
                        ]
                        .into_iter()
                        .flatten()
                        .any(|id| removed_ids.contains(id));
                        !touches_removed
                    })
                    .collect::<Vec<_>>();
                (entities, relationships)
            }
            None => (entities, relationships),
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
            && snap.current_for_epoch(observed_epoch)
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
                            eligibility_epoch: observed_epoch,
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

        // Result-eligibility filter (code artifacts): suppressed members are
        // dropped from the group membership AND therefore from the summary
        // text BEFORE the match gate — otherwise a file-path member would
        // pass the token gate and re-admit its whole community. Community
        // DETECTION (Louvain labels, cached snapshots) stays full-graph:
        // suppression never shapes community structure, only emission.
        if engram_retrieval::suppresses_code_artifacts(request) {
            for members in groups.values_mut() {
                members.retain(|key| {
                    by_key
                        .get(key.as_str())
                        .is_none_or(|e| !engram_retrieval::is_code_artifact(e))
                });
            }
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
        // MATCH GATE + honest scores (graph-lane contract: match first, then
        // expand): a community with ZERO token overlap contributes nothing —
        // there is no match to expand from — and every emitted member carries
        // a score derived from the actual match, never a fabricated 1.0.
        let tokens = engram_retrieval::query_tokens(&request.query);
        let mut ranked: Vec<(usize, Vec<String>, f64, f64)> = groups
            .into_iter()
            .map(|(label, members)| {
                let names: Vec<&str> = members
                    .iter()
                    .filter_map(|k| display_names.get(k.as_str()).copied())
                    .collect();
                let mut predicates = predicates_by_comm.remove(&label).unwrap_or_default();
                predicates.sort();
                predicates.dedup();
                // AUDIT 2026-10-20: the match haystack is NAMES + PREDICATES
                // only — the previous "Community: ...; edges: ..." template
                // made the boilerplate words themselves matchable, so queries
                // containing "community"/"edges" matched every community.
                let summary = format!("{} {}", names.join(", "), predicates.join(", "));
                let matched = if tokens.is_empty() {
                    0.0
                } else {
                    let lower = summary.to_lowercase();
                    tokens.iter().filter(|t| lower.contains(t.as_str())).count() as f64
                };
                // Specificity: what fraction of the community's summary the
                // matched tokens cover — an honest 0..1 relevance for the
                // community as a whole (a 1-token hit on a 12-token summary
                // is a weak match, not a perfect one).
                let summary_tokens = summary.split_whitespace().count().max(1) as f64;
                let specificity = (matched / summary_tokens).min(1.0);
                (label, members, matched, specificity)
            })
            .collect();
        ranked.sort_by(|a, b| {
            b.2.partial_cmp(&a.2)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });

        // Gate: with no matching community there is nothing legitimate to
        // return. Previously the all-zero tie-break (lowest label) emitted
        // the SAME community for every unrelated query at fabricated 1.0
        // scores — deterministic background noise on nearly every recall.
        let Some((_label, members, matched, specificity)) = ranked.into_iter().next() else {
            return Ok(Vec::new());
        };
        if matched <= 0.0 {
            return Ok(Vec::new());
        }

        // Match first, then expand: members whose own name matches a query
        // token rank above pure-expansion members, and expansion members
        // score at half the community specificity (they are connected to a
        // match, not a match themselves).
        let mut scored: Vec<(String, f32, bool)> = members
            .into_iter()
            .map(|key| {
                let direct = display_names
                    .get(key.as_str())
                    .map(|n| name_matches_tokens(n, &tokens))
                    .unwrap_or(false);
                let score = (specificity * if direct { 1.0 } else { 0.5 }).min(1.0) as f32;
                (key, score, direct)
            })
            .collect();
        scored.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.cmp(&b.0)));
        let results = scored
            .into_iter()
            .take(limit.max(1) as usize)
            .enumerate()
            .filter_map(|(rank, (key, score, _direct))| {
                by_key
                    .get(key.as_str())
                    .map(|e| build_result(&key, e, score, rank))
            })
            .collect();
        Ok(results)
    }
}

/// True when the entity name itself contains one of the query tokens — the
/// member is a direct match, not just a neighbor of one.
fn name_matches_tokens(name: &str, tokens: &[String]) -> bool {
    let lower = name.to_lowercase();
    tokens.iter().any(|t| lower.contains(t.as_str()))
}

/// Resolves an entity reference to its entity-id key, or `None` if no id.
/// True when an entity is a code symbol or structural code artifact WITHOUT a
/// generation stamp — legacy unbound code. Never eligible once a binding view
/// constrains the lane (task_014).
fn entity_is_unstamped_code(entity: &KnowledgeEntity) -> bool {
    let stamped = entity
        .metadata
        .as_ref()
        .and_then(|meta| meta.get(engram_retrieval::bound_filter::CODE_GENERATION_KEY))
        .is_some();
    if stamped {
        return false;
    }
    engram_retrieval::is_code_artifact(entity)
        || matches!(
            entity.kind,
            engram_domain::EntityKind::Class
                | engram_domain::EntityKind::Function
                | engram_domain::EntityKind::Method
                | engram_domain::EntityKind::Struct
                | engram_domain::EntityKind::Interface
                | engram_domain::EntityKind::Trait
                | engram_domain::EntityKind::TypeAlias
                | engram_domain::EntityKind::Enum
        )
}

fn entity_key(reference: &EntityRef) -> Option<String> {
    reference.id.as_ref().map(|id| id.as_str().to_owned())
}

/// Builds a `RetrievalResult` for a community-member entity.
///
/// `score` is the honest lane score (community specificity × direct-match
/// multiplier — see the match gate in `retrieve_candidates`); the fabricated
/// constant `1.0` that used to land here surfaced as "perfect" raw scores in
/// fused recall for items that never matched the query at all.
fn build_result(key: &str, entity: &KnowledgeEntity, score: f32, rank: usize) -> RetrievalResult {
    RetrievalResult {
        id: format!("community-summary-entity-{key}"),
        target_type: RetrievalTargetType::Entity,
        target_id: key.to_owned(),
        content: entity.name.clone(),
        score: RetrievalScore {
            total: score,
            relevance: Some(score),
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
            source_rank: Some((rank + 1) as u32),
            source_score: Some(score),
            score: None,
            rank: None,
            fusion_strategy: Some(FusionStrategy::None),
            fusion_score: Some(score),
            rerank_strategy: Some(RerankStrategy::None),
            rerank_score: Some(score),
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
        ent_kind(id, name, EntityKind::Concept)
    }

    fn ent_kind(id: &str, name: &str, kind: EntityKind) -> KnowledgeEntity {
        KnowledgeEntity {
            id: Id::from(id),
            graph_id: None,
            kind,
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

            archived_at: None,
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
            code_resolution: None,
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

            archived_at: None,
        }
    }

    fn request_with(query: &str, include_code_artifacts: Option<bool>) -> RetrievalRequest {
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
            filters: Some(engram_domain::QueryFilter {
                memory_kinds: Vec::new(),
                source_kinds: Vec::new(),
                chunk_kinds: Vec::new(),
                concept_ids: Vec::new(),
                entity_ids: Vec::new(),
                since: None,
                until: None,
                min_confidence: None,
                include_archived: None,
                as_of: None,
                include_code_artifacts,
                code_index_keys: None,
            }),
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

    fn request(query: &str) -> RetrievalRequest {
        request_with(query, None)
    }

    /// Code-artifact suppression (2026-10-20): suppressed members are dropped
    /// from group membership AND summary text before the match gate (a
    /// file-path member must not pass the token gate and re-admit its
    /// community); opt-in restores them. Community detection stays full-graph.
    #[test]
    fn code_artifact_members_suppressed_before_the_match_gate() {
        let source = Arc::new(StubSource {
            entities: vec![
                ent_kind("concept", "Payments", EntityKind::Concept),
                ent_kind("file", "billing.ts", EntityKind::File),
                ent_kind("fn", "charge_card", EntityKind::Function),
            ],
            relationships: vec![
                rel("concept", "file"),
                rel("file", "concept"),
                rel("concept", "fn"),
                rel("fn", "concept"),
            ],
        });
        let index = CommunitySummaryIndex::new(source);

        // Default: the File member is out of the membership AND the summary —
        // a query matching ONLY the file name must not pass the match gate.
        let results = block_on(index.retrieve_candidates(&request("billing"))).unwrap();
        assert!(
            results.is_empty(),
            "file-only match must not re-admit the community: {:?}",
            results
                .iter()
                .map(|r| r.target_id.clone())
                .collect::<Vec<_>>()
        );

        // A query matching the CONCEPT still returns the community minus the file.
        let results = block_on(index.retrieve_candidates(&request("payments"))).unwrap();
        let ids: Vec<&str> = results.iter().map(|r| r.target_id.as_str()).collect();
        assert!(
            ids.contains(&"concept") && ids.contains(&"fn"),
            "eligible members emitted: {ids:?}"
        );
        assert!(!ids.contains(&"file"), "suppressed member absent: {ids:?}");

        // Opt-in: the file member returns (and can match on its name).
        let results =
            block_on(index.retrieve_candidates(&request_with("billing", Some(true)))).unwrap();
        let ids: Vec<&str> = results.iter().map(|r| r.target_id.as_str()).collect();
        assert!(
            ids.contains(&"file"),
            "opt-in emits the file member: {ids:?}"
        );
    }
}
