//! Integration tests for the structured-repo-identity spec (T4 / T5 / T6).
//!
//! Verifies that:
//! - A git-backed `DocumentIngestRequest` (with `stable_source_key` set and
//!   `source_kind = GitRepository`) produces a `KnowledgeSource` whose `metadata`
//!   carries the key, a `KnowledgeGraph` whose `metadata` carries both the key and
//!   the document path, a single `EntityKind::Repository` node with `graph_id =
//!   None`, and a `belongs_to` relationship whose `graph_id` equals the document
//!   graph id (AC-1, AC-3, AC-5).
//! - `list_graphs_by_source` returns exactly that repo's graphs; joined via
//!   `list_entities_by_source` / `list_relationships_by_source` it returns exactly
//!   that repo's document-level entities and relationships (AC-4).
//! - Filtering is correct across two repos in one scope (AC-4).

use engram_domain::*;
use engram_ingest::{
    CodeSymbolChunker, DocumentIngestRequest, DocumentMetadata, GraphExtractor, KnowledgeIngestor,
    SOURCE_PATH_KEY, STABLE_SOURCE_KEY,
};
use engram_knowledge::KnowledgeGraphRepository;
use engram_store_knowledge_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;

const REPO_A: &str = "github.com/acme/alpha";
const REPO_B: &str = "github.com/acme/beta";
const TENANT: &str = "tenant-ri-test";

fn scope() -> Scope {
    Scope {
        tenant: TENANT.to_owned(),
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
        id: Id::from("test-agent"),
        kind: ActorKind::Agent,
        display_name: None,
        metadata: None,
    }
}

fn git_request(repo_key: &str, path: &str, text: &str) -> DocumentIngestRequest {
    DocumentIngestRequest {
        source_kind: SourceKind::GitRepository,
        source_name: format!("repo-{repo_key}"),
        scope: scope(),
        document_kind: SourceDocumentKind::Code,
        document: DocumentMetadata {
            path: Some(path.to_owned()),
            ..Default::default()
        },
        text: text.to_owned(),
        policy: policy(),
        actor: actor(),
        stable_source_key: Some(repo_key.to_owned()),
    }
}

/// Ingests `request` into `store`, then extracts the graph, and returns the
/// `ExtractedGraph` for assertion.
fn ingest_and_extract(
    store: &SqlKnowledgeStore,
    request: DocumentIngestRequest,
) -> engram_ingest::ExtractedGraph {
    let ingestor = KnowledgeIngestor::new(CodeSymbolChunker);
    let ingested = block_on(ingestor.ingest(store, request)).expect("ingest");
    block_on(GraphExtractor::new().extract_into(
        store,
        &ingested.source,
        &ingested.document,
        &ingested.chunks,
        None,
    ))
    .expect("extract")
}

// ---------------------------------------------------------------------------
// T4 / T2: Source kind + graph metadata stamping
// ---------------------------------------------------------------------------

#[test]
fn git_backed_source_is_tagged_git_repository() {
    let store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let ingestor = KnowledgeIngestor::new(CodeSymbolChunker);
    let request = git_request(REPO_A, "src/main.rs", "fn run() {}\n");
    let ingested = block_on(ingestor.ingest(&store, request)).expect("ingest");

    assert_eq!(
        ingested.source.kind,
        SourceKind::GitRepository,
        "source should be tagged GitRepository"
    );
    let key_in_meta = ingested
        .source
        .metadata
        .as_ref()
        .and_then(|m| m.get(STABLE_SOURCE_KEY))
        .and_then(|v| v.as_str());
    assert_eq!(
        key_in_meta,
        Some(REPO_A),
        "stable_source_key must be threaded onto KnowledgeSource.metadata"
    );
}

#[test]
fn graph_metadata_carries_stable_source_key_and_path() {
    let store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let doc_path = "src/lib.rs";
    let extracted = ingest_and_extract(&store, git_request(REPO_A, doc_path, "fn alpha() {}\n"));

    let meta = extracted
        .graph
        .metadata
        .as_ref()
        .expect("graph must have metadata");
    assert_eq!(
        meta.get(STABLE_SOURCE_KEY).and_then(|v| v.as_str()),
        Some(REPO_A),
        "graph.metadata[stableSourceKey] must equal the repo key"
    );
    assert_eq!(
        meta.get(SOURCE_PATH_KEY).and_then(|v| v.as_str()),
        Some(doc_path),
        "graph.metadata[path] must equal the document path"
    );
}

// ---------------------------------------------------------------------------
// T5: Repository node + belongs_to edge
// ---------------------------------------------------------------------------

#[test]
fn exactly_one_repository_entity_with_belongs_to_edge() {
    let store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let extracted =
        ingest_and_extract(&store, git_request(REPO_A, "src/lib.rs", "fn alpha() {}\n"));

    // One EntityKind::Repository node.
    let repo_entities: Vec<_> = extracted
        .entities
        .iter()
        .filter(|e| e.kind == EntityKind::Repository)
        .collect();
    assert_eq!(repo_entities.len(), 1, "exactly one Repository entity");
    let repo = &repo_entities[0];
    assert_eq!(
        repo.name, REPO_A,
        "Repository entity name = stable_source_key"
    );
    assert!(
        repo.graph_id.is_none(),
        "Repository entity must have graph_id = None"
    );
    let key_in_meta = repo
        .metadata
        .as_ref()
        .and_then(|m| m.get(STABLE_SOURCE_KEY))
        .and_then(|v| v.as_str());
    assert_eq!(key_in_meta, Some(REPO_A));

    // One belongs_to relationship from the document graph to the repo entity.
    let belongs_to_rels: Vec<_> = extracted
        .relationships
        .iter()
        .filter(|r| r.predicate == "belongs_to")
        .collect();
    assert_eq!(belongs_to_rels.len(), 1, "exactly one belongs_to edge");
    let bt = &belongs_to_rels[0];
    assert_eq!(
        bt.graph_id.as_ref().map(|id| id.to_string()),
        Some(extracted.graph.id.to_string()),
        "belongs_to.graph_id must equal the document graph id"
    );
    assert_eq!(
        bt.object.name.as_deref(),
        Some(REPO_A),
        "belongs_to.object.name must be the repo key"
    );
    assert_eq!(
        bt.object.kind.as_deref(),
        Some("repository"),
        "belongs_to.object.kind must be 'repository'"
    );
    // Repository node and belongs_to carry no path (not file-scoped): verify
    // the Repository entity's metadata has no "path" key and that the
    // belongs_to relationship has no "path" key either (relationships have no
    // metadata field at all, which is enforced structurally by the domain type).
    let repo_entity = extracted
        .entities
        .iter()
        .find(|e| e.kind == EntityKind::Repository)
        .expect("Repository entity");
    assert!(
        repo_entity
            .metadata
            .as_ref()
            .and_then(|m| m.get("path"))
            .is_none(),
        "Repository entity must carry no 'path' in metadata (not file-scoped)"
    );
    // `KnowledgeRelationship` has no metadata field — the type itself enforces
    // this; the assertion is that the belongs_to edge is NOT about a specific
    // file path (checked via the subject/object refs carrying no path field).
    assert!(
        bt.subject.name.as_deref() != Some("src/lib.rs"),
        "belongs_to.subject.name should be the graph name, not a file path"
    );

    // Subject references the graph.
    assert_eq!(
        bt.subject.id.as_ref().map(|id| id.to_string()),
        Some(extracted.graph.id.to_string()),
        "belongs_to.subject.id must reference the document graph"
    );
    assert_eq!(bt.subject.kind.as_deref(), Some("graph"));
}

#[test]
fn re_ingesting_same_repo_second_document_produces_same_repository_entity() {
    let store = SqlKnowledgeStore::open_in_memory().expect("open store");

    let extracted1 = ingest_and_extract(&store, git_request(REPO_A, "src/a.rs", "fn one() {}\n"));
    let extracted2 = ingest_and_extract(&store, git_request(REPO_A, "src/b.rs", "fn two() {}\n"));

    let repo1 = extracted1
        .entities
        .iter()
        .find(|e| e.kind == EntityKind::Repository)
        .unwrap();
    let repo2 = extracted2
        .entities
        .iter()
        .find(|e| e.kind == EntityKind::Repository)
        .unwrap();
    assert_eq!(
        repo1.id, repo2.id,
        "Repository entity id must be the same for two documents of the same repo"
    );
}

// ---------------------------------------------------------------------------
// T6: Query by repo — list_graphs_by_source / entities / relationships
// ---------------------------------------------------------------------------

#[test]
fn list_graphs_by_source_returns_only_that_repos_graphs() {
    let store = SqlKnowledgeStore::open_in_memory().expect("open store");

    ingest_and_extract(&store, git_request(REPO_A, "src/a.rs", "fn one() {}\n"));
    ingest_and_extract(&store, git_request(REPO_A, "src/b.rs", "fn two() {}\n"));
    ingest_and_extract(&store, git_request(REPO_B, "src/c.rs", "fn three() {}\n"));

    let a_graphs =
        block_on(store.list_graphs_by_source(&scope(), REPO_A)).expect("list_graphs_by_source");
    let b_graphs =
        block_on(store.list_graphs_by_source(&scope(), REPO_B)).expect("list_graphs_by_source");

    assert_eq!(a_graphs.len(), 2, "REPO_A has two document graphs");
    assert_eq!(b_graphs.len(), 1, "REPO_B has one document graph");

    // All returned graphs carry the correct key in metadata.
    for g in &a_graphs {
        assert_eq!(
            g.metadata
                .as_ref()
                .and_then(|m| m.get(STABLE_SOURCE_KEY))
                .and_then(|v| v.as_str()),
            Some(REPO_A)
        );
    }
    for g in &b_graphs {
        assert_eq!(
            g.metadata
                .as_ref()
                .and_then(|m| m.get(STABLE_SOURCE_KEY))
                .and_then(|v| v.as_str()),
            Some(REPO_B)
        );
    }
}

#[test]
fn list_entities_by_source_returns_document_entities_not_repo_node() {
    let store = SqlKnowledgeStore::open_in_memory().expect("open store");

    let extracted = ingest_and_extract(
        &store,
        git_request(REPO_A, "src/a.rs", "fn alpha() { beta(); }\nfn beta() {}\n"),
    );
    let doc_entity_count = extracted
        .entities
        .iter()
        .filter(|e| e.kind != EntityKind::Repository)
        .count();

    let entities =
        block_on(store.list_entities_by_source(&scope(), REPO_A)).expect("list_entities_by_source");

    // All returned entities must be document-level (non-Repository).
    assert!(
        entities.iter().all(|e| e.kind != EntityKind::Repository),
        "list_entities_by_source must not include the Repository node"
    );
    // Count must match document-level entities (graph_id = Some).
    assert_eq!(
        entities.len(),
        doc_entity_count,
        "entity count via join must match document-level entities"
    );
}

#[test]
fn list_relationships_by_source_includes_belongs_to_edges() {
    let store = SqlKnowledgeStore::open_in_memory().expect("open store");

    ingest_and_extract(&store, git_request(REPO_A, "src/a.rs", "fn alpha() {}\n"));
    ingest_and_extract(&store, git_request(REPO_B, "src/b.rs", "fn beta() {}\n"));

    let a_rels = block_on(store.list_relationships_by_source(&scope(), REPO_A))
        .expect("list_relationships_by_source");
    let b_rels = block_on(store.list_relationships_by_source(&scope(), REPO_B))
        .expect("list_relationships_by_source");

    // Each repo must have exactly one belongs_to edge (from its document graph).
    let a_bt: Vec<_> = a_rels
        .iter()
        .filter(|r| r.predicate == "belongs_to")
        .collect();
    let b_bt: Vec<_> = b_rels
        .iter()
        .filter(|r| r.predicate == "belongs_to")
        .collect();
    assert_eq!(a_bt.len(), 1, "REPO_A has one belongs_to edge");
    assert_eq!(b_bt.len(), 1, "REPO_B has one belongs_to edge");

    // REPO_B's relationships must not appear in REPO_A's result set.
    let a_rel_ids: std::collections::HashSet<_> = a_rels.iter().map(|r| r.id.to_string()).collect();
    let b_rel_ids: std::collections::HashSet<_> = b_rels.iter().map(|r| r.id.to_string()).collect();
    assert!(
        a_rel_ids.is_disjoint(&b_rel_ids),
        "relationships for REPO_A and REPO_B must not overlap"
    );
}

#[test]
fn non_git_source_produces_no_repository_node() {
    let store = SqlKnowledgeStore::open_in_memory().expect("open store");
    let ingestor = KnowledgeIngestor::new(CodeSymbolChunker);
    // No stable_source_key → no Repository entity.
    let request = DocumentIngestRequest {
        source_kind: SourceKind::Filesystem,
        source_name: "local-files".to_owned(),
        scope: scope(),
        document_kind: SourceDocumentKind::Code,
        document: DocumentMetadata {
            path: Some("main.rs".to_owned()),
            ..Default::default()
        },
        text: "fn main() {}\n".to_owned(),
        policy: policy(),
        actor: actor(),
        stable_source_key: None,
    };
    let ingested = block_on(ingestor.ingest(&store, request)).expect("ingest");
    let extracted = block_on(GraphExtractor::new().extract_into(
        &store,
        &ingested.source,
        &ingested.document,
        &ingested.chunks,
        None,
    ))
    .expect("extract");

    assert!(
        extracted
            .entities
            .iter()
            .all(|e| e.kind != EntityKind::Repository),
        "no Repository entity expected when stable_source_key is absent"
    );
    assert!(
        extracted
            .relationships
            .iter()
            .all(|r| r.predicate != "belongs_to"),
        "no belongs_to edge expected when stable_source_key is absent"
    );
    assert!(
        extracted
            .graph
            .metadata
            .as_ref()
            .and_then(|m| m.get(STABLE_SOURCE_KEY))
            .is_none(),
        "graph.metadata must not have stableSourceKey when key is absent"
    );
}
