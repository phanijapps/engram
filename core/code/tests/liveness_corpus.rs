//! Liveness evidence corpus (spec: code-liveness T1).
//!
//! Four verified real-world false-verdict shapes, pinned as regression
//! fixtures. Assertions describe the POST-fix contract; each test names the
//! task that makes it green. These cases came from the 2026-09-03 dogfood
//! run: engram's dead-code output reported live code dead (agentzero
//! `DelegateToZbotTool`, mem-alpha `FastEmbedEmbeddingProvider` via
//! qualified constructors; `MemoryLifecycleExecutor::execute` via
//! `Arc<dyn _>` dispatch).

use engram_code::{ChunkCandidate, SymbolCandidate, SymbolIndex, TreeSitterChunker, resolve_refs};
use engram_domain::*;
use std::collections::HashSet;

const DECL_FILE: &str = r#"
pub struct DelegateToZbotTool { service: Arc<ZbotService> }
impl DelegateToZbotTool {
    pub fn new(service: Arc<ZbotService>) -> Self { Self { service } }
}
pub fn unrelated_entry() {}
"#;

const CALL_FILE: &str = r#"
use crate::tools::DelegateToZbotTool;
pub fn build_registry(service: Arc<ZbotService>) -> ToolRegistry {
    tool_registry.register(Arc::new(crate::tools::DelegateToZbotTool::new(
        service.clone(),
    )));
    tool_registry
}
"#;

fn chunker() -> TreeSitterChunker {
    TreeSitterChunker::new().expect("chunker boot")
}

/// Anchor names for one source file ("fn Foo::bar" anchors → `Foo::bar`).
fn anchor_names(chunker: &TreeSitterChunker, source: &str) -> Vec<String> {
    chunker
        .chunk_with_ext(source, "rs")
        .expect("chunk")
        .iter()
        .filter_map(|c: &ChunkCandidate| {
            c.location
                .as_ref()?
                .anchor
                .as_deref()?
                .split_once(' ')
                .map(|(_, name)| name.to_owned())
        })
        .collect()
}

/// Build the scope-wide symbol index from anchor names across files.
fn symbol_index(names: &[String]) -> SymbolIndex {
    let mut index = SymbolIndex::new();
    for name in names {
        index.register(
            name,
            SymbolCandidate {
                id: format!("id-{name}"),
                name: name.clone(),
                repo: Some("repo".to_owned()),
                path: Some(format!("{}.rs", name.to_lowercase().replace("::", "_"))),
            },
        );
    }
    index
}

fn entity_names(names: &[String]) -> HashSet<String> {
    let mut set: HashSet<String> = names.iter().cloned().collect();
    for n in names {
        if let Some(tail) = engram_code::bare_tail(n) {
            set.insert(tail.to_owned());
        }
    }
    set
}

/// Case 1 — qualified constructor call (agentzero `DelegateToZbotTool`
/// shape, executor.rs:1689): a deep qualified path `a::b::Type::name`
/// resolves to the declared `Type::name` entity. [T2: declaration-side
/// qualified relaxation + call-side qualified tail]
#[test]
fn qualified_constructor_declaration_survives_and_resolves() {
    let ck = chunker();
    let decl_names = anchor_names(&ck, DECL_FILE);
    assert!(
        decl_names.iter().any(|n| n == "DelegateToZbotTool::new"),
        "impl-block constructor must be a declared symbol: {decl_names:?}"
    );

    let names: Vec<String> = decl_names.clone();
    let index = symbol_index(&names);
    let entities = entity_names(&names);

    let call_tree = ck.parse(CALL_FILE, "rs").expect("parse");
    let calls = ck
        .extract_calls_tree(&call_tree, CALL_FILE, "rs", &entities)
        .expect("extract calls");
    assert!(
        calls
            .iter()
            .any(|(_, callee)| callee.contains("DelegateToZbotTool::new")),
        "qualified constructor call must be extracted: {calls:?}"
    );

    // Resolve the call edge against the scope-wide index.
    let mut rels: Vec<KnowledgeRelationship> = calls
        .iter()
        .map(|(caller, callee)| KnowledgeRelationship {
            id: RelationshipId::from(format!("rel-{caller}-{callee}")),
            graph_id: Some(KnowledgeGraphId::from("g1")),
            subject: EntityRef {
                id: Some(EntityId::from(format!("id-{caller}"))),
                kind: None,
                name: Some(caller.clone()),
                aliases: Vec::new(),
            },
            predicate: "calls".to_owned(),
            object: EntityRef {
                id: None,
                kind: None,
                name: Some(callee.clone()),
                aliases: Vec::new(),
            },
            scope: corpus_scope(),
            evidence: Vec::new(),
            confidence: None,
            provenance: corpus_provenance(),
            created_at: chrono::Utc::now(),
            updated_at: None,
            archived_at: None,
        })
        .collect();
    let ledger = resolve_refs(&index, &mut rels, Some("repo"), Some("call.rs"));
    let resolved = rels.iter().any(|r| {
        r.object
            .name
            .as_deref()
            .is_some_and(|n| n == "DelegateToZbotTool::new")
            && r.object.id.is_some()
    });
    assert!(
        resolved,
        "qualified constructor edge must resolve (ledger: {:?})",
        ledger
            .iter()
            .map(|l| l.reference_name.clone())
            .collect::<Vec<_>>()
    );
}

/// Case 2 — registration site (executor.rs:1689 register(Arc::new(..))):
/// extraction emits a (registration-site, constructed-type) fact the ingest
/// adapter persists as a `registers` edge. [T3]
#[test]
fn registration_site_yields_registers_fact() {
    let ck = chunker();
    let edges = ck.extract_structural(CALL_FILE, "rs").expect("structural");
    assert!(
        edges
            .registers
            .iter()
            .any(|(_site, ty)| ty == "DelegateToZbotTool"),
        "register(Arc::new(T::new(..))) must yield a registers fact: {:?}",
        edges.registers
    );
}

/// Case 3 — `Arc<dyn Trait>` factory shape (mem-alpha
/// `MemoryLifecycleExecutor::execute` shape, bootstrap.rs:505): the
/// constructor call inside the `Arc::new` wrapper extracts as a call
/// reference (`MemoryLifecycleExecutor::new`) so T2's resolved ctor edge
/// makes the type live; the impl methods' `dynamic` bucketing is the
/// classifier's job (codegraph-queries T4/T5). [T2]
#[test]
fn dyn_dispatch_factory_constructor_call_extracts() {
    let ck = chunker();
    let source = r#"
pub struct MemoryLifecycleExecutor { store: Arc<Store> }
impl MemoryLifecycleExecutor {
    pub fn with_default_threshold(store: Arc<Store>) -> Self { Self { store } }
}
pub fn build(store: Arc<Store>) -> Arc<dyn MutationExecutor> {
    Arc::new(MemoryLifecycleExecutor::with_default_threshold(store.clone()))
}
"#;
    let names = anchor_names(&ck, source);
    let entities = entity_names(&names);
    let tree = ck.parse(source, "rs").expect("parse");
    let calls = ck
        .extract_calls_tree(&tree, source, "rs", &entities)
        .expect("extract calls");
    assert!(
        calls
            .iter()
            .any(|(_, callee)| callee == "MemoryLifecycleExecutor::with_default_threshold"),
        "factory ctor inside Arc::new must extract as a call ref: {calls:?}"
    );
}

/// Case 4 — test doubles classify `test-only`, not `confirmed-unused`
/// (mem-alpha `FailingMemory` shape). [T5 — classifier test lives in
/// codegraph-queries; here we pin that a `mod tests` declaration is
/// distinguishable by containment.]
#[test]
fn test_double_is_contained_in_tests_module() {
    let ck = chunker();
    let source = r#"
pub fn real() {}
#[cfg(test)]
mod tests {
    struct FailingMemory;
    impl FailingMemory { pub fn get(&self) -> Option<Mem> { None } }
}
"#;
    let edges = ck.extract_structural(source, "rs").expect("structural");
    assert!(
        edges
            .contains
            .iter()
            .any(|(parent, child)| parent.contains("tests") && child.contains("FailingMemory")),
        "test module containment must be extractable: {:?}",
        edges.contains
    );
}

fn corpus_scope() -> engram_domain::Scope {
    engram_domain::Scope {
        tenant: "corpus".to_owned(),
        subject: None,
        workspace: None,
        session: None,
        environment: None,
    }
}

fn corpus_provenance() -> engram_domain::Provenance {
    engram_domain::Provenance {
        source: "corpus".to_owned(),
        actor: engram_domain::Actor {
            id: Id::from("corpus"),
            kind: engram_domain::ActorKind::Agent,
            display_name: None,
            metadata: None,
        },
        observed_at: chrono::Utc::now(),
        method: None,
        confidence: None,
        derivations: Vec::new(),
        evidence: Vec::new(),
    }
}
