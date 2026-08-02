//! Materialized graph snapshot cache for the graph retrieval lanes.
//!
//! The three graph retrieval lanes (`GraphRetrievalIndex`,
//! `AssociativeGraphIndex`, `CommunitySummaryIndex`) each reload the full
//! in-scope knowledge graph — all entities and relationships — from the store
//! on every query. On a populated project that is ~36k entities + ~57k
//! relationships per lane, dominated by JSON deserialization (~300k
//! deserializations across the three lanes). Caching the materialized snapshot
//! per scope eliminates that reload on every query after the first.
//!
//! The cache is a **pluggable trait** ([`GraphCache`]). [`InMemoryGraphCache`]
//! is the default, process-local implementation; a future Redis or disk-backed
//! cache implements the same trait. The trait is `async` + `Send` + `Sync` and
//! keyed by a scope-derived [`String`] so a network-backed implementation fits
//! the same contract without changing call sites.
//!
//! Scope isolation is inherited from whatever populated the snapshot: the lanes
//! scope-filter before building it, so a cached entry never leaks across scope
//! boundaries. [`GraphCache::invalidate`] must be called after a write that
//! changes the graph for a scope (e.g. `scan_repo`) so stale entries do not
//! serve wrong results.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use engram_domain::{KnowledgeEntity, KnowledgeRelationship, Scope};
use tokio::sync::RwLock;

/// A materialized graph snapshot for one scope — the data the graph lanes
/// reload per query.
///
/// Caching this eliminates ~300k JSON deserializations on every query after the
/// first (entities + relationships across the three graph lanes). The snapshot
/// is immutable once stored; lanes read it through `Arc<GraphSnapshot>` so
/// concurrent readers never copy.
///
/// Scope isolation: a snapshot is only ever populated from scope-filtered reads,
/// so its contents are exactly what the requesting `scope` is allowed to see.
/// Callers MUST invalidate after a graph-mutating write (`scan_repo`) so a
/// stale snapshot never serves a newer graph.
#[derive(Debug, Clone)]
pub struct GraphSnapshot {
    /// All knowledge-graph entities visible to the snapshot's scope.
    pub entities: Vec<KnowledgeEntity>,
    /// All knowledge-graph relationships visible to the snapshot's scope
    /// (the directed edge set the graph lanes traverse). Empty when the
    /// populating lane only reads entities (e.g. the lexical graph lane).
    pub relationships: Vec<KnowledgeRelationship>,
    /// Cached Louvain community labels (entity-key → community-id), populated
    /// by the community-summary lane after its first detection pass. `None`
    /// until computed; `Some` lets every subsequent community/associative query
    /// skip the ~57k-edge Louvain recompute. Refreshed together with the
    /// entities/relationships on a cache miss or invalidation.
    pub community_labels: Option<HashMap<String, usize>>,
}

/// Pluggable graph cache: stores materialized [`GraphSnapshot`]s keyed by
/// scope so the graph retrieval lanes skip the per-query store reload.
///
/// In-memory now; Redis/disk behind the same trait later. The trait is `async`
/// + `Send` + `Sync` and uses a scope-derived [`String`] key (not `Scope`
/// itself, which carries no `Hash`/`Eq` in the domain model — see
/// [`scope_key`]) so a network-backed implementation fits the same contract.
///
/// Implementations MUST be safe to call concurrently from the three graph
/// lanes (which run together inside unified recall). [`InMemoryGraphCache`]
/// satisfies this with a `tokio::sync::RwLock`.
#[async_trait]
pub trait GraphCache: Send + Sync {
    /// Returns the cached snapshot for the scope, if present.
    ///
    /// A hit lets a lane skip its store reads entirely; a miss (or `None`
    /// cache) falls back to the lane's existing load path, which then populates
    /// the cache for subsequent queries.
    async fn get(&self, scope: &Scope) -> Option<Arc<GraphSnapshot>>;

    /// Stores a snapshot for the scope.
    ///
    /// Only lanes that have read BOTH entities and relationships should
    /// populate the cache, so a stored snapshot never carries an empty edge set
    /// that would degrade a later associative/community lane hit. Last writer
    /// wins; concurrent puts for the same scope write equivalent data.
    async fn put(&self, scope: &Scope, snapshot: Arc<GraphSnapshot>);

    /// Invalidates the cached snapshot for the scope.
    ///
    /// Call after a write that changes the graph for this scope so the next
    /// query reloads fresh data.
    async fn invalidate(&self, scope: &Scope);

    /// Invalidates all cached snapshots.
    ///
    /// Heavier than per-scope [`invalidate`](Self::invalidate); use when a
    /// write may have touched multiple scopes (e.g. a bulk re-index).
    async fn invalidate_all(&self);
}

/// Process-local in-memory graph cache: the default [`GraphCache`].
///
/// Backed by a `tokio::sync::RwLock<HashMap<String, Arc<GraphSnapshot>>>` keyed
/// by [`scope_key`]. Reads take a cheap read-lock and clone only the `Arc`;
/// writes take a brief write-lock. One `Arc<InMemoryGraphCache>` is shared
/// across all three graph lanes so a cache miss in one lane benefits the others
/// on the next query (they all need the same scope's entities/relationships).
pub struct InMemoryGraphCache {
    entries: RwLock<HashMap<String, Arc<GraphSnapshot>>>,
}

impl InMemoryGraphCache {
    /// Creates an empty in-memory graph cache.
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
        }
    }

    /// Returns the number of cached scopes (diagnostic / test helper).
    pub async fn len(&self) -> usize {
        self.entries.read().await.len()
    }

    /// Returns `true` when no scopes are cached.
    pub async fn is_empty(&self) -> bool {
        self.entries.read().await.is_empty()
    }
}

impl Default for InMemoryGraphCache {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl GraphCache for InMemoryGraphCache {
    async fn get(&self, scope: &Scope) -> Option<Arc<GraphSnapshot>> {
        self.entries.read().await.get(&scope_key(scope)).cloned()
    }

    async fn put(&self, scope: &Scope, snapshot: Arc<GraphSnapshot>) {
        self.entries
            .write()
            .await
            .insert(scope_key(scope), snapshot);
    }

    async fn invalidate(&self, scope: &Scope) {
        self.entries.write().await.remove(&scope_key(scope));
    }

    async fn invalidate_all(&self) {
        self.entries.write().await.clear();
    }
}

/// Derives a stable cache key from a [`Scope`].
///
/// `Scope` carries no `Hash`/`Eq` in the domain model (adding them would be a
/// domain change), so the cache is keyed by a deterministic string built from
/// the scope's fields. Optional fields render as the empty string, so a `None`
/// and a `Some("")` field are indistinguishable — acceptable here because both
/// denote the same effective scope partition for caching purposes.
pub fn scope_key(scope: &Scope) -> String {
    format!(
        "{}|{}|{}|{}|{}",
        scope.tenant,
        scope.subject.as_deref().unwrap_or(""),
        scope.workspace.as_deref().unwrap_or(""),
        scope.session.as_deref().unwrap_or(""),
        scope.environment.as_deref().unwrap_or(""),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope(tenant: &str) -> Scope {
        Scope {
            tenant: tenant.to_owned(),
            subject: None,
            workspace: None,
            session: None,
            environment: None,
        }
    }

    fn snapshot() -> Arc<GraphSnapshot> {
        Arc::new(GraphSnapshot {
            entities: Vec::new(),
            relationships: Vec::new(),
            community_labels: None,
        })
    }

    #[tokio::test]
    async fn miss_then_hit_round_trip() {
        let cache = InMemoryGraphCache::new();
        let scope = scope("t1");
        assert!(cache.get(&scope).await.is_none(), "fresh cache misses");
        cache.put(&scope, snapshot()).await;
        assert!(cache.get(&scope).await.is_some(), "populated cache hits");
        assert_eq!(cache.len().await, 1);
    }

    #[tokio::test]
    async fn invalidate_removes_only_that_scope() {
        let cache = InMemoryGraphCache::new();
        let a = scope("a");
        let b = scope("b");
        cache.put(&a, snapshot()).await;
        cache.put(&b, snapshot()).await;
        cache.invalidate(&a).await;
        assert!(cache.get(&a).await.is_none(), "invalidated scope gone");
        assert!(cache.get(&b).await.is_some(), "other scope preserved");
    }

    #[tokio::test]
    async fn invalidate_all_clears_everything() {
        let cache = InMemoryGraphCache::new();
        cache.put(&scope("a"), snapshot()).await;
        cache.put(&scope("b"), snapshot()).await;
        cache.invalidate_all().await;
        assert!(cache.is_empty().await);
    }

    #[tokio::test]
    async fn distinct_scopes_do_not_collide() {
        let cache = InMemoryGraphCache::new();
        let a = scope("a");
        let b = scope("b");
        cache.put(&a, snapshot()).await;
        assert!(cache.get(&b).await.is_none(), "different tenant misses");
    }

    #[test]
    fn scope_key_round_trips_fields() {
        let full = Scope {
            tenant: "t".to_owned(),
            subject: Some("s".to_owned()),
            workspace: Some("w".to_owned()),
            session: None,
            environment: Some("e".to_owned()),
        };
        assert_eq!(scope_key(&full), "t|s|w||e");
    }
}
