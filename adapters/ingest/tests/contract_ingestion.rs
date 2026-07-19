//! Integration tests for the contract-first-ingestion spec (T5, T6, T8).
//!
//! T5 — cross-repo merge: two sources declaring the same normalized operation
//!      key resolve to one `EntityKind::Api` entity whose `source_refs` contain
//!      both sources, with two separate `exposes` edges.
//!
//! T6 — malformed skip: a file with an `openapi:` marker that fails to parse
//!      increments `ScanSummary.skipped`, leaves `errors` unchanged, and
//!      produces no contract entity.
//!
//! T8 — per-source convergence on re-ingest:
//!      - drop-op: source retracts an operation → `exposes` edge deleted,
//!        source_ref removed, entity deleted (only source).
//!      - shared-survives: one of two sources retracts → entity survives with
//!        the remaining source's `source_ref`.
//!      - last-retract-deletes: final source retracts → entity deleted.

use std::fs;
use std::path::PathBuf;

use engram_domain::*;
use engram_ingest::{ScanOptions, scan_repository};
use engram_store_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;

// ── helpers ───────────────────────────────────────────────────────────────────

fn scope() -> Scope {
    Scope {
        tenant: "tenant-cfi-test".to_owned(),
        subject: None,
        workspace: Some("test".to_owned()),
        session: None,
        environment: Some("test".to_owned()),
    }
}

fn policy() -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: None,
        allowed_uses: vec![AllowedUse::Retrieval],
        expires_at: None,
        delete_mode: None,
    }
}

fn actor() -> Actor {
    Actor {
        id: Id::from("cfi-test-agent"),
        kind: ActorKind::Agent,
        display_name: None,
        metadata: None,
    }
}

fn scan_opts(
    source_name: &str,
    manifest: std::collections::HashMap<String, String>,
) -> ScanOptions {
    ScanOptions {
        scope: scope(),
        policy: policy(),
        actor: actor(),
        source_name: source_name.to_owned(),
        max_bytes: 0,
        manifest,
    }
}

/// Creates a unique temp directory for a test (uses process id + label).
fn tmp_dir(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("engram-cfi-{}-{}", std::process::id(), label));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create temp dir");
    root
}

/// Returns all `EntityKind::Api` entities visible in scope.
fn api_entities(store: &SqlKnowledgeStore) -> Vec<KnowledgeEntity> {
    block_on(store.list_entities(&scope()))
        .unwrap_or_default()
        .into_iter()
        .filter(|e| e.kind == EntityKind::Api)
        .collect()
}

/// Returns all `exposes` relationships visible in scope.
fn exposes_rels(store: &SqlKnowledgeStore) -> Vec<KnowledgeRelationship> {
    block_on(store.list_relationships(&scope()))
        .unwrap_or_default()
        .into_iter()
        .filter(|r| r.predicate == "exposes")
        .collect()
}

const ORDERS_OPENAPI: &str = r#"
openapi: "3.0.0"
info:
  title: Orders API
  version: "1.0"
paths:
  /orders:
    post:
      summary: Create order
      responses:
        "201":
          description: created
  /orders/{id}:
    get:
      summary: Get order
      responses:
        "200":
          description: ok
"#;

const GET_ORDERS_ONLY: &str = r#"
openapi: "3.0.0"
info:
  title: Orders API
  version: "1.0"
paths:
  /orders/{id}:
    get:
      summary: Get order
      responses:
        "200":
          description: ok
"#;

const EMPTY_PATHS_OPENAPI: &str = r#"
openapi: "3.0.0"
info:
  title: Orders API
  version: "1.0"
paths: {}
"#;

// ── T5: cross-repo merge ──────────────────────────────────────────────────────

#[test]
fn two_sources_declaring_same_op_merge_to_one_entity_with_two_source_refs() {
    let store = SqlKnowledgeStore::open_in_memory().expect("open store");

    // Repo A — scan a temp dir with an OpenAPI spec.
    let dir_a = tmp_dir("t5-a");
    fs::write(dir_a.join("openapi.yaml"), ORDERS_OPENAPI).expect("write a");
    let (summary_a, _manifest_a) = scan_repository(
        &dir_a,
        &scan_opts("repo-t5-a", Default::default()),
        &store,
        |_| {},
    )
    .expect("scan a");
    assert_eq!(summary_a.errors, 0, "scan A must have no errors");

    // Repo B — different source, same OpenAPI ops.
    let dir_b = tmp_dir("t5-b");
    fs::write(dir_b.join("openapi.yaml"), ORDERS_OPENAPI).expect("write b");
    let (summary_b, _manifest_b) = scan_repository(
        &dir_b,
        &scan_opts("repo-t5-b", Default::default()),
        &store,
        |_| {},
    )
    .expect("scan b");
    assert_eq!(summary_b.errors, 0, "scan B must have no errors");

    // Expect exactly two Api entities (GET /orders/{} and POST /orders), each
    // with both source_refs accumulated.
    let apis = api_entities(&store);
    assert_eq!(
        apis.len(),
        2,
        "expected 2 Api entities (GET + POST), got: {:?}",
        apis.iter().map(|e| &e.name).collect::<Vec<_>>()
    );

    for entity in &apis {
        assert_eq!(
            entity.source_refs.len(),
            2,
            "entity '{}' must have 2 source_refs (one per repo), got {:?}",
            entity.name,
            entity.source_refs
        );
        let ids: Vec<_> = entity
            .source_refs
            .iter()
            .filter_map(|r| r.target_id.as_deref())
            .collect();
        // stable_source_key for a non-git dir = lowercase(source_name)
        assert!(
            ids.contains(&"repo-t5-a"),
            "source_refs must include repo-t5-a, got {ids:?}"
        );
        assert!(
            ids.contains(&"repo-t5-b"),
            "source_refs must include repo-t5-b, got {ids:?}"
        );
    }

    // Expect exactly four exposes edges (2 ops × 2 repos).
    let rels = exposes_rels(&store);
    assert_eq!(
        rels.len(),
        4,
        "expected 4 exposes edges (2 ops × 2 repos), got {} edges",
        rels.len()
    );
}

#[test]
fn reingest_unchanged_doc_is_idempotent() {
    let store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let dir = tmp_dir("t5-idem");
    fs::write(dir.join("openapi.yaml"), ORDERS_OPENAPI).expect("write");

    let (_, manifest1) = scan_repository(
        &dir,
        &scan_opts("repo-idem", Default::default()),
        &store,
        |_| {},
    )
    .expect("scan 1");

    // Second scan with the same content (unchanged) — pass the first manifest.
    let (_, _manifest2) =
        scan_repository(&dir, &scan_opts("repo-idem", manifest1), &store, |_| {}).expect("scan 2");

    // Still exactly 2 Api entities and 2 exposes edges — no duplicates.
    let apis = api_entities(&store);
    assert_eq!(
        apis.len(),
        2,
        "idempotent re-ingest must not duplicate entities"
    );
    for e in &apis {
        assert_eq!(
            e.source_refs.len(),
            1,
            "idempotent re-ingest must not duplicate source_refs for '{}'",
            e.name
        );
    }
    let rels = exposes_rels(&store);
    assert_eq!(
        rels.len(),
        2,
        "idempotent re-ingest must not duplicate exposes edges"
    );
}

// ── T6: malformed spec skipped, scan continues ────────────────────────────────

#[test]
fn malformed_openapi_increments_skipped_not_errors_and_produces_no_entity() {
    let store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let dir = tmp_dir("t6-malformed");

    // A file that starts with the openapi marker but is syntactically broken.
    let malformed = "openapi: 3.0.0\npaths:\n  /bad: [\n    garbage";
    fs::write(dir.join("openapi.yaml"), malformed).expect("write");

    let (summary, _) = scan_repository(
        &dir,
        &scan_opts("repo-malformed", Default::default()),
        &store,
        |_| {},
    )
    .expect("scan must complete — a bad spec never fails the job");

    assert_eq!(
        summary.errors, 0,
        "malformed spec must not increment errors"
    );
    assert!(
        summary.skipped >= 1,
        "malformed OpenAPI must increment skipped (got {})",
        summary.skipped
    );

    let apis = api_entities(&store);
    assert!(
        apis.is_empty(),
        "malformed OpenAPI must not produce any Api entity"
    );
}

// ── T8: per-source convergence on re-ingest ───────────────────────────────────

/// Re-ingesting a changed doc removes the `exposes` edge and entity for an
/// operation that the source no longer declares (single-source case).
#[test]
fn drop_op_retracts_exposes_edge_and_deletes_entity() {
    let store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let dir = tmp_dir("t8-drop");

    // First scan: GET + POST declared.
    fs::write(dir.join("openapi.yaml"), ORDERS_OPENAPI).expect("write v1");
    let (_, manifest1) = scan_repository(
        &dir,
        &scan_opts("repo-drop", Default::default()),
        &store,
        |_| {},
    )
    .expect("scan 1");
    assert_eq!(api_entities(&store).len(), 2, "pre: expect 2 Api entities");

    // Second scan: only GET remains.
    fs::write(dir.join("openapi.yaml"), GET_ORDERS_ONLY).expect("write v2");
    let (summary2, _) =
        scan_repository(&dir, &scan_opts("repo-drop", manifest1), &store, |_| {}).expect("scan 2");
    assert_eq!(summary2.errors, 0);

    let apis = api_entities(&store);
    assert_eq!(
        apis.len(),
        1,
        "only GET /orders/{{}} must remain after POST is retracted, got {:?}",
        apis.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
    assert_eq!(
        apis[0].name, "GET /orders/{}",
        "surviving entity must be GET /orders/{{}}"
    );

    let rels = exposes_rels(&store);
    assert_eq!(
        rels.len(),
        1,
        "only the GET exposes edge must survive; POST edge must be deleted"
    );
}

/// When two sources declare the same operation and one source retracts it,
/// the entity survives with only the remaining source's `source_ref`.
#[test]
fn shared_op_survives_when_one_source_retracts() {
    let store = SqlKnowledgeStore::open_in_memory().expect("open store");

    // Repo A: has GET /orders/{id}.
    let dir_a = tmp_dir("t8-shared-a");
    fs::write(dir_a.join("openapi.yaml"), GET_ORDERS_ONLY).expect("write a1");
    let (_, manifest_a1) = scan_repository(
        &dir_a,
        &scan_opts("repo-shared-a", Default::default()),
        &store,
        |_| {},
    )
    .expect("scan A1");

    // Repo B: also has GET /orders/{id}.
    let dir_b = tmp_dir("t8-shared-b");
    fs::write(dir_b.join("openapi.yaml"), GET_ORDERS_ONLY).expect("write b1");
    let (_, _manifest_b1) = scan_repository(
        &dir_b,
        &scan_opts("repo-shared-b", Default::default()),
        &store,
        |_| {},
    )
    .expect("scan B1");

    // Sanity: one entity, two source_refs.
    {
        let apis = api_entities(&store);
        assert_eq!(apis.len(), 1, "pre-retract: 1 Api entity");
        assert_eq!(
            apis[0].source_refs.len(),
            2,
            "pre-retract: entity must have both source_refs"
        );
    }

    // Repo A re-scans with empty paths — retracts GET /orders/{}.
    fs::write(dir_a.join("openapi.yaml"), EMPTY_PATHS_OPENAPI).expect("write a2");
    let (summary_a2, _) = scan_repository(
        &dir_a,
        &scan_opts("repo-shared-a", manifest_a1),
        &store,
        |_| {},
    )
    .expect("scan A2");
    assert_eq!(summary_a2.errors, 0);

    // Entity must survive with only repo-shared-b's source_ref.
    let apis = api_entities(&store);
    assert_eq!(
        apis.len(),
        1,
        "entity must survive while repo B still declares the op"
    );
    assert_eq!(
        apis[0].source_refs.len(),
        1,
        "only repo-shared-b's source_ref must remain, got {:?}",
        apis[0].source_refs
    );
    assert_eq!(
        apis[0].source_refs[0].target_id.as_deref(),
        Some("repo-shared-b"),
        "remaining source_ref must be repo-shared-b"
    );

    // exposes edges: repo-shared-b's edge survives; repo-shared-a's is deleted.
    let rels = exposes_rels(&store);
    assert_eq!(
        rels.len(),
        1,
        "only repo-shared-b's exposes edge must survive"
    );
}

/// BLOCKER regression: two files in the SAME repo both declare the same
/// operation. When one file drops it the operation MUST survive because the
/// other file still declares it. Only when BOTH files drop it should the entity
/// and edge be retracted.
///
/// This verifies per-SOURCE union retraction (not per-file).
#[test]
fn op_declared_by_two_files_survives_when_one_file_drops_it() {
    let store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let dir = tmp_dir("t8-two-file-union");

    // Scan 1: a.yaml (GET /orders/{}) + b.yaml (GET /orders/{})
    // Same source → same stable_source_key → one entity, one source_ref, one edge.
    fs::write(dir.join("a.yaml"), GET_ORDERS_ONLY).expect("write a1");
    fs::write(dir.join("b.yaml"), GET_ORDERS_ONLY).expect("write b1");
    let (_, manifest1) = scan_repository(
        &dir,
        &scan_opts("repo-two-file", Default::default()),
        &store,
        |_| {},
    )
    .expect("scan 1");

    let apis_after_scan1 = api_entities(&store);
    assert_eq!(
        apis_after_scan1.len(),
        1,
        "scan 1: both files declare the same op → one entity"
    );

    // Scan 2: a.yaml changed to empty paths; b.yaml unchanged (still declares
    // GET /orders/{}).  The entity MUST survive.
    fs::write(dir.join("a.yaml"), EMPTY_PATHS_OPENAPI).expect("write a2 (empty)");
    let (summary2, manifest2) =
        scan_repository(&dir, &scan_opts("repo-two-file", manifest1), &store, |_| {})
            .expect("scan 2");
    assert_eq!(summary2.errors, 0, "scan 2 must have no errors");

    let apis_after_scan2 = api_entities(&store);
    assert_eq!(
        apis_after_scan2.len(),
        1,
        "scan 2: b.yaml still declares GET /orders/{{}} → entity must survive"
    );

    // Scan 3: b.yaml also changed to empty paths → entity must now be deleted.
    fs::write(dir.join("b.yaml"), EMPTY_PATHS_OPENAPI).expect("write b2 (empty)");
    let (summary3, _) =
        scan_repository(&dir, &scan_opts("repo-two-file", manifest2), &store, |_| {})
            .expect("scan 3");
    assert_eq!(summary3.errors, 0, "scan 3 must have no errors");

    let apis_after_scan3 = api_entities(&store);
    assert!(
        apis_after_scan3.is_empty(),
        "scan 3: both files dropped the op → entity must be deleted, got: {:?}",
        apis_after_scan3.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
    let rels_after_scan3 = exposes_rels(&store);
    assert!(
        rels_after_scan3.is_empty(),
        "scan 3: exposes edge must also be deleted"
    );
}

/// Re-ingesting a changed OpenAPI document that adds an operation must add the
/// new entity/edge without duplicating the existing one (fewer → more).
#[test]
fn reingest_adding_op_adds_entity_without_duplicating_existing() {
    let store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let dir = tmp_dir("t8-fewer-more");

    // Scan 1: GET /orders/{} only.
    fs::write(dir.join("openapi.yaml"), GET_ORDERS_ONLY).expect("write v1 (GET only)");
    let (_, manifest1) = scan_repository(
        &dir,
        &scan_opts("repo-fewer-more", Default::default()),
        &store,
        |_| {},
    )
    .expect("scan 1");

    assert_eq!(api_entities(&store).len(), 1, "scan 1: 1 entity (GET only)");
    assert_eq!(exposes_rels(&store).len(), 1, "scan 1: 1 exposes edge");

    // Scan 2: GET /orders/{} + POST /orders (ORDERS_OPENAPI has both).
    fs::write(dir.join("openapi.yaml"), ORDERS_OPENAPI).expect("write v2 (GET+POST)");
    let (summary2, _) = scan_repository(
        &dir,
        &scan_opts("repo-fewer-more", manifest1),
        &store,
        |_| {},
    )
    .expect("scan 2");
    assert_eq!(summary2.errors, 0, "scan 2 must have no errors");

    let apis = api_entities(&store);
    assert_eq!(
        apis.len(),
        2,
        "scan 2: GET + POST must each have an entity (no duplicates), got: {:?}",
        apis.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
    let names: Vec<&str> = apis.iter().map(|e| e.name.as_str()).collect();
    assert!(
        names.contains(&"GET /orders/{}"),
        "GET /orders/{{}} entity must survive: {names:?}"
    );
    assert!(
        names.contains(&"POST /orders"),
        "POST /orders entity must be added: {names:?}"
    );

    // Each entity has exactly one source_ref — no duplicates.
    for entity in &apis {
        assert_eq!(
            entity.source_refs.len(),
            1,
            "entity '{}' must have exactly 1 source_ref (no dup from re-ingest)",
            entity.name
        );
    }

    let rels = exposes_rels(&store);
    assert_eq!(
        rels.len(),
        2,
        "scan 2: exactly 2 exposes edges (GET + POST), got {}",
        rels.len()
    );
}

/// When the only remaining source retracts an operation, the Api entity is
/// deleted (no orphaned contract nodes).
#[test]
fn last_source_retraction_deletes_api_entity() {
    let store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let dir = tmp_dir("t8-last");

    // Scan 1: declare GET /orders/{}.
    fs::write(dir.join("openapi.yaml"), GET_ORDERS_ONLY).expect("write v1");
    let (_, manifest1) = scan_repository(
        &dir,
        &scan_opts("repo-last", Default::default()),
        &store,
        |_| {},
    )
    .expect("scan 1");
    assert_eq!(api_entities(&store).len(), 1, "pre: 1 Api entity");

    // Scan 2: empty paths — retracts the operation.
    fs::write(dir.join("openapi.yaml"), EMPTY_PATHS_OPENAPI).expect("write v2");
    let (summary2, _) =
        scan_repository(&dir, &scan_opts("repo-last", manifest1), &store, |_| {}).expect("scan 2");
    assert_eq!(summary2.errors, 0);

    let apis = api_entities(&store);
    assert!(
        apis.is_empty(),
        "Api entity must be deleted when its last source retracts the op, got {:?}",
        apis.iter().map(|e| &e.name).collect::<Vec<_>>()
    );

    let rels = exposes_rels(&store);
    assert!(
        rels.is_empty(),
        "exposes edge must be deleted when the last source retracts"
    );
}

// ── FailingEntityWriteStore ───────────────────────────────────────────────────

/// A store wrapper whose `put_entity` returns an error for `EntityKind::Api`
/// entities, simulating a transient contract entity write failure.
///
/// All other methods delegate to the inner `SqlKnowledgeStore` unchanged, so
/// document-graph, source, chunk, and Repository-node writes all succeed.
/// `delete_graph` also delegates (not blocked), so the pre-pass graph deletion
/// runs normally and no duplicate graphs are produced.
struct FailingEntityWriteStore {
    inner: SqlKnowledgeStore,
}

#[async_trait::async_trait]
impl engram_knowledge::KnowledgeRepository for FailingEntityWriteStore {
    async fn put_source(
        &self,
        source: engram_domain::KnowledgeSource,
    ) -> engram_knowledge::CoreResult<engram_domain::KnowledgeSource> {
        self.inner.put_source(source).await
    }
    async fn put_document(
        &self,
        document: engram_domain::SourceDocument,
    ) -> engram_knowledge::CoreResult<engram_domain::SourceDocument> {
        self.inner.put_document(document).await
    }
    async fn put_chunk(
        &self,
        chunk: engram_domain::KnowledgeChunk,
    ) -> engram_knowledge::CoreResult<engram_domain::KnowledgeChunk> {
        self.inner.put_chunk(chunk).await
    }
    async fn get_chunk(
        &self,
        id: &engram_domain::ChunkId,
        scope: &engram_domain::Scope,
    ) -> engram_knowledge::CoreResult<Option<engram_domain::KnowledgeChunk>> {
        self.inner.get_chunk(id, scope).await
    }
    /// Fails for `EntityKind::Api`; delegates all other entity kinds to `inner`.
    ///
    /// `put_entity` is also called for `EntityKind::Repository` nodes and
    /// code-graph entities (Function, Module, etc.) — those delegate normally.
    async fn put_entity(
        &self,
        entity: engram_domain::KnowledgeEntity,
    ) -> engram_knowledge::CoreResult<engram_domain::KnowledgeEntity> {
        if entity.kind == engram_domain::EntityKind::Api {
            Err(engram_knowledge::CoreError::Adapter {
                adapter: "test".into(),
                message: "forced write failure".into(),
            })
        } else {
            self.inner.put_entity(entity).await
        }
    }
    async fn put_relationship(
        &self,
        relationship: engram_domain::KnowledgeRelationship,
    ) -> engram_knowledge::CoreResult<engram_domain::KnowledgeRelationship> {
        self.inner.put_relationship(relationship).await
    }
    async fn get_entity(
        &self,
        id: &engram_domain::EntityId,
        scope: &engram_domain::Scope,
    ) -> engram_knowledge::CoreResult<Option<engram_domain::KnowledgeEntity>> {
        self.inner.get_entity(id, scope).await
    }
    async fn get_relationship(
        &self,
        id: &engram_domain::RelationshipId,
        scope: &engram_domain::Scope,
    ) -> engram_knowledge::CoreResult<Option<engram_domain::KnowledgeRelationship>> {
        self.inner.get_relationship(id, scope).await
    }
    async fn delete_entity(
        &self,
        id: &engram_domain::EntityId,
        scope: &engram_domain::Scope,
    ) -> engram_knowledge::CoreResult<bool> {
        self.inner.delete_entity(id, scope).await
    }
    async fn delete_relationship(
        &self,
        id: &engram_domain::RelationshipId,
        scope: &engram_domain::Scope,
    ) -> engram_knowledge::CoreResult<bool> {
        self.inner.delete_relationship(id, scope).await
    }
}

#[async_trait::async_trait]
impl engram_knowledge::KnowledgeGraphRepository for FailingEntityWriteStore {
    async fn put_graph(
        &self,
        graph: engram_domain::KnowledgeGraph,
    ) -> engram_knowledge::CoreResult<engram_domain::KnowledgeGraph> {
        self.inner.put_graph(graph).await
    }
    async fn get_graph(
        &self,
        id: &engram_domain::KnowledgeGraphId,
        scope: &engram_domain::Scope,
    ) -> engram_knowledge::CoreResult<Option<engram_domain::KnowledgeGraph>> {
        self.inner.get_graph(id, scope).await
    }
    async fn neighbors(
        &self,
        graph_id: &engram_domain::KnowledgeGraphId,
        node_id: &engram_domain::EntityId,
        scope: &engram_domain::Scope,
        limit: Option<u32>,
    ) -> engram_knowledge::CoreResult<Vec<engram_domain::KnowledgeRelationship>> {
        self.inner.neighbors(graph_id, node_id, scope, limit).await
    }
    async fn delete_graph(
        &self,
        id: &engram_domain::KnowledgeGraphId,
        scope: &engram_domain::Scope,
    ) -> engram_knowledge::CoreResult<bool> {
        self.inner.delete_graph(id, scope).await
    }
    async fn list_graphs_by_source(
        &self,
        scope: &engram_domain::Scope,
        stable_source_key: &str,
    ) -> engram_knowledge::CoreResult<Vec<engram_domain::KnowledgeGraph>> {
        self.inner
            .list_graphs_by_source(scope, stable_source_key)
            .await
    }
}

// ── FIX: contract write failure must not retract a still-declared op ──────────

/// Variant of GET_ORDERS_ONLY with a different summary/version so its content
/// hash differs, forcing reprocessing on the second scan, while the declared
/// operation (`GET /orders/{id}` → normalized `GET /orders/{}`) is unchanged.
const GET_ORDERS_CHANGED: &str = r#"
openapi: "3.0.0"
info:
  title: Orders API
  version: "1.1"
paths:
  /orders/{id}:
    get:
      summary: Retrieve order by identifier
      responses:
        "200":
          description: ok
"#;

/// Validates the `write_error_rels` path in `scanner.rs`:
///
/// When a contract entity write fails (simulated by `FailingEntityWriteStore`),
/// the file's content hash is NOT recorded so it reprocesses next scan, and
/// the file's prior contract keys are folded into `current_union` so the
/// still-declared operation is never retracted.
///
/// Note on `put_entity` sharing: `FailingEntityWriteStore` discriminates by
/// `EntityKind::Api` so only contract entity writes are forced to fail;
/// `EntityKind::Repository` nodes and code-graph entities (Function, Module,
/// etc.) all delegate to `inner` and succeed normally.
#[test]
fn contract_write_failure_does_not_retract_still_declared_op() {
    let real_store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let dir = tmp_dir("cfi-write-fail");

    // ── Step 1: Scan with real store — entity must exist ────────────────────
    fs::write(dir.join("openapi.yaml"), GET_ORDERS_ONLY).expect("write v1");
    let (summary1, manifest1) = scan_repository(
        &dir,
        &scan_opts("repo-write-fail", Default::default()),
        &real_store,
        |_| {},
    )
    .expect("scan 1");
    assert_eq!(summary1.errors, 0, "scan 1 must have no errors");

    let apis_after_1 = api_entities(&real_store);
    assert_eq!(
        apis_after_1.len(),
        1,
        "scan 1: one Api entity expected, got: {:?}",
        apis_after_1.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
    assert_eq!(
        apis_after_1[0].name, "GET /orders/{}",
        "scan 1: entity must be GET /orders/{{}}"
    );

    // ── Step 2: Re-scan with FailingEntityWriteStore ─────────────────────────
    // Content changes (different hash) so the file is reprocessed, but the
    // contract entity write fails before the new entity can be persisted.
    // The still-declared op must NOT be retracted.
    fs::write(dir.join("openapi.yaml"), GET_ORDERS_CHANGED)
        .expect("write v2 (changed metadata, same op)");
    let failing_store = FailingEntityWriteStore { inner: real_store };
    let (summary2, manifest2) = scan_repository(
        &dir,
        &scan_opts("repo-write-fail", manifest1),
        &failing_store,
        |_| {},
    )
    .expect("scan 2 must not panic");

    // (a) Prior Api entity STILL PRESENT — write_error_rels carried its prior
    //     contract key into current_union, preventing retraction.
    //     EntityKind::Api has graph_id = None; delete_graph cascades only on
    //     graph_id-matched rows, so the Api entity is unaffected by the
    //     pre-pass graph deletion.
    let apis_after_2 = api_entities(&failing_store.inner);
    assert_eq!(
        apis_after_2.len(),
        1,
        "scan 2: Api entity must survive when contract write fails, got: {:?}",
        apis_after_2.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
    assert_eq!(
        apis_after_2[0].name, "GET /orders/{}",
        "scan 2: entity name must be preserved after write failure"
    );

    // (b) summary.skipped incremented: contract_had_write_error increments skipped.
    assert!(
        summary2.skipped >= 1,
        "scan 2: contract write failure must increment summary.skipped (got {})",
        summary2.skipped
    );

    // File hash not in manifest2 (not recorded on write error → reprocess next scan).
    assert!(
        !manifest2.contains_key("openapi.yaml"),
        "scan 2: plain path must not appear in manifest when contract write fails"
    );
    // Prior contract manifest entry carried forward via write_error_rels.
    assert!(
        manifest2.contains_key("contract:openapi.yaml"),
        "scan 2: prior contract manifest entry must be carried forward"
    );

    // ── Step 3: Re-scan with real store — self-healed ───────────────────────
    // openapi.yaml is absent from manifest2 so it reprocesses; the contract
    // write now succeeds and the Api entity is correct.
    let (summary3, _manifest3) = scan_repository(
        &dir,
        &scan_opts("repo-write-fail", manifest2),
        &failing_store.inner,
        |_| {},
    )
    .expect("scan 3");
    assert_eq!(summary3.errors, 0, "scan 3 must have no errors");

    let apis_after_3 = api_entities(&failing_store.inner);
    assert_eq!(
        apis_after_3.len(),
        1,
        "scan 3: Api entity must be present after self-heal, got: {:?}",
        apis_after_3.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
    assert_eq!(
        apis_after_3[0].name, "GET /orders/{}",
        "scan 3: entity name must be correct after self-heal"
    );
}
