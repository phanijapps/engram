//! Codegraph query bindings.
//!
//! Exposes `engram-codegraph-queries` over the SQLite knowledge store as JSON,
//! mirroring the graph-fusion helpers in `knowledge_fusion.rs`. Each helper
//! reads a `scope` from the request JSON, enumerates that scope's
//! `KnowledgeRelationship`s, runs the query, and encodes the result. The
//! `NativeKnowledgeEngine` methods in `knowledge.rs` delegate here.

use std::sync::Arc;

use engram_codegraph_queries as cgq;
use engram_domain::{KnowledgeEntity, KnowledgeRelationship, Scope};
use engram_integration::code_binding::CodeBindingCoordinator;
use engram_retrieval::RecordEligibility;
use engram_store_sqlite::SqlKnowledgeStore;
use futures::executor::block_on;
use napi::bindgen_prelude::*;
use serde_json::Value;

use crate::{encode, to_napi_error};

fn json_error(error: serde_json::Error) -> Error {
    Error::from_reason(error.to_string())
}

fn scope_of(value: &Value) -> Result<Scope> {
    serde_json::from_value(value["scope"].clone()).map_err(json_error)
}

/// The actor attributed to code-graph reads through this bridge.
fn code_graph_actor() -> engram_domain::Actor {
    engram_domain::Actor {
        id: engram_domain::Id::from("engram-node-code-graph"),
        kind: engram_domain::ActorKind::Agent,
        display_name: None,
        metadata: None,
    }
}

/// The PINNED published-generation view (review r6 finding 3): fail closed
/// when the scope has no published code generation, authorize the caller for
/// EVERY published repository through the shared coordinator seam, and
/// return the scope's relationships + entities CONSTRAINED to the pinned
/// generation's durable lineage (stale/retired/unbound records never
/// participate). The epoch is revalidated before the window closes — a
/// publication that raced the read denies with `GenerationChanged`.
fn pinned_generation_records(
    coordinator: &Arc<CodeBindingCoordinator>,
    store: &Arc<SqlKnowledgeStore>,
    scope: &Scope,
) -> Result<(Vec<KnowledgeRelationship>, Vec<KnowledgeEntity>)> {
    let eligibility = block_on(coordinator.store().eligibility(scope)).map_err(to_napi_error)?;
    if eligibility.generations.is_empty() {
        return Err(Error::from_reason(
            "BindingRequired: code lane unavailable — no published code generation for this scope",
        ));
    }
    for _ in 0..2 {
        // Shared authorized seam: every published repository authorizes the
        // read (Inspect) — the same policy gate registration/publication use.
        for repo in &eligibility.repository_ids {
            block_on(coordinator.authorize(
                &code_graph_actor(),
                scope,
                &engram_domain::RepoId(repo.clone()),
            ))
            .map_err(to_napi_error)?;
        }
        let view = block_on(coordinator.eligibility_view(scope, None)).map_err(to_napi_error)?;
        let participates =
            |record: &str| matches!(view.classify(record, true), RecordEligibility::EligibleCode);
        let relationships = block_on(store.list_relationships(scope))
            .map_err(to_napi_error)?
            .into_iter()
            .filter(|relationship| participates(&relationship.id.to_string()))
            .collect::<Vec<_>>();
        let entities = block_on(store.list_entities(scope))
            .map_err(to_napi_error)?
            .into_iter()
            .filter(|entity| participates(&entity.id.to_string()))
            .collect::<Vec<_>>();
        if block_on(coordinator.store().eligibility(scope)).map_err(to_napi_error)? == eligibility {
            return Ok((relationships, entities));
        }
    }
    Err(to_napi_error(engram_runtime::CoreError::Conflict {
        reason: "GenerationChanged".into(),
    }))
}

/// `{scope}` -> dead-code symbol keys (sorted). Mirrors memtrace `find_dead_code`.
pub fn dead_code_json(
    store: &Arc<SqlKnowledgeStore>,
    coordinator: &Arc<CodeBindingCoordinator>,
    request_json: String,
) -> Result<String> {
    let value: Value = serde_json::from_str(&request_json).map_err(json_error)?;
    let scope = scope_of(&value)?;
    let (relationships, _entities) = pinned_generation_records(coordinator, store, &scope)?;
    encode(&cgq::dead_code(&cgq::AnalyticsGraph::from_relationships(
        &relationships,
    )))
}

/// `{scope, target, depth?}` -> transitive caller keys (blast radius).
pub fn blast_radius_json(
    store: &Arc<SqlKnowledgeStore>,
    coordinator: &Arc<CodeBindingCoordinator>,
    request_json: String,
) -> Result<String> {
    let value: Value = serde_json::from_str(&request_json).map_err(json_error)?;
    let scope = scope_of(&value)?;
    let (relationships, _entities) = pinned_generation_records(coordinator, store, &scope)?;
    let target = value["target"].as_str().unwrap_or("").to_owned();
    let depth = value["depth"].as_u64().unwrap_or(5) as usize;
    let mut callers: Vec<String> = cgq::blast_radius(&relationships, &target, depth)
        .into_iter()
        .collect();
    callers.sort();
    encode(&callers)
}

/// `{scope, from, to}` -> shortest call path (`[symbol, ...]`) or `null`.
pub fn dependency_path_json(
    store: &Arc<SqlKnowledgeStore>,
    coordinator: &Arc<CodeBindingCoordinator>,
    request_json: String,
) -> Result<String> {
    let value: Value = serde_json::from_str(&request_json).map_err(json_error)?;
    let scope = scope_of(&value)?;
    let (relationships, _entities) = pinned_generation_records(coordinator, store, &scope)?;
    let from = value["from"].as_str().unwrap_or("").to_owned();
    let to = value["to"].as_str().unwrap_or("").to_owned();
    encode(&cgq::dependency_path(&relationships, &from, &to))
}

/// `{scope, limit?}` -> `[[symbol, score], ...]` ranked by PageRank centrality.
pub fn central_symbols_json(
    store: &Arc<SqlKnowledgeStore>,
    coordinator: &Arc<CodeBindingCoordinator>,
    request_json: String,
) -> Result<String> {
    let value: Value = serde_json::from_str(&request_json).map_err(json_error)?;
    let scope = scope_of(&value)?;
    let (relationships, _entities) = pinned_generation_records(coordinator, store, &scope)?;
    let limit = value["limit"].as_u64().unwrap_or(20) as usize;
    encode(&cgq::central_symbols(
        &cgq::AnalyticsGraph::from_relationships(&relationships),
        limit,
    ))
}

/// `{scope, limit?}` -> `[[symbol, score], ...]` ranked by betweenness (bridges).
pub fn bridge_symbols_json(
    store: &Arc<SqlKnowledgeStore>,
    coordinator: &Arc<CodeBindingCoordinator>,
    request_json: String,
) -> Result<String> {
    let value: Value = serde_json::from_str(&request_json).map_err(json_error)?;
    let scope = scope_of(&value)?;
    let (relationships, _entities) = pinned_generation_records(coordinator, store, &scope)?;
    let limit = value["limit"].as_u64().unwrap_or(20) as usize;
    encode(&cgq::bridge_symbols(
        &cgq::AnalyticsGraph::from_relationships(&relationships),
        limit,
    ))
}

/// `{scope, maxPasses?}` -> `{symbol: label}` Louvain communities.
pub fn call_communities_json(
    store: &Arc<SqlKnowledgeStore>,
    coordinator: &Arc<CodeBindingCoordinator>,
    request_json: String,
) -> Result<String> {
    let value: Value = serde_json::from_str(&request_json).map_err(json_error)?;
    let scope = scope_of(&value)?;
    let (relationships, _entities) = pinned_generation_records(coordinator, store, &scope)?;
    let max_passes = value["maxPasses"].as_u64().unwrap_or(10) as usize;
    encode(&cgq::call_communities(
        &cgq::AnalyticsGraph::from_relationships(&relationships),
        max_passes,
    ))
}

/// `{source}` -> cyclomatic complexity (integer).
pub fn cyclomatic_complexity_json(request_json: String) -> Result<String> {
    let value: Value = serde_json::from_str(&request_json).map_err(json_error)?;
    let source = value["source"].as_str().unwrap_or("");
    Ok(cgq::cyclomatic_complexity(source).to_string())
}

/// `{source}` -> `[{method, path}, ...]` HTTP endpoints.
pub fn find_endpoints_json(request_json: String) -> Result<String> {
    let value: Value = serde_json::from_str(&request_json).map_err(json_error)?;
    let source = value["source"].as_str().unwrap_or("");
    encode(&cgq::find_endpoints(source))
}

/// `{source}` -> `["/path", ...]` HTTP call-site targets.
pub fn find_api_calls_json(request_json: String) -> Result<String> {
    let value: Value = serde_json::from_str(&request_json).map_err(json_error)?;
    let source = value["source"].as_str().unwrap_or("");
    encode(&cgq::find_api_calls(source))
}

/// `{source}` -> `["main", ...]` entry-point function names.
pub fn find_entry_points_json(request_json: String) -> Result<String> {
    let value: Value = serde_json::from_str(&request_json).map_err(json_error)?;
    let source = value["source"].as_str().unwrap_or("");
    encode(&cgq::find_entry_points(source))
}

/// `{scope, entryPoint, maxDepth?}` -> `[symbol, ...]` execution flow.
pub fn process_flow_json(
    store: &Arc<SqlKnowledgeStore>,
    coordinator: &Arc<CodeBindingCoordinator>,
    request_json: String,
) -> Result<String> {
    let value: Value = serde_json::from_str(&request_json).map_err(json_error)?;
    let scope = scope_of(&value)?;
    let (relationships, _entities) = pinned_generation_records(coordinator, store, &scope)?;
    let entry = value["entryPoint"].as_str().unwrap_or("");
    let max_depth = value["maxDepth"].as_u64().unwrap_or(10) as usize;
    encode(&cgq::process_flow(&relationships, entry, max_depth))
}

/// `{endpoints: [{method, path}], calls: ["/path"]}` -> cross-service topology.
pub fn match_api_topology_json(request_json: String) -> Result<String> {
    let value: Value = serde_json::from_str(&request_json).map_err(json_error)?;
    let endpoints: Vec<cgq::HttpEndpoint> =
        serde_json::from_value(value["endpoints"].clone()).map_err(json_error)?;
    let calls: Vec<String> = serde_json::from_value(value["calls"].clone()).map_err(json_error)?;
    encode(&cgq::match_api_topology(&endpoints, &calls))
}

/// `{scope}` -> file-level import graph (RFC-0020 Phase 2): files → imported
/// module paths, resolved to defining files by stem suffix.
pub fn file_dependencies_json(
    store: &Arc<SqlKnowledgeStore>,
    coordinator: &Arc<CodeBindingCoordinator>,
    request_json: String,
) -> Result<String> {
    let value: Value = serde_json::from_str(&request_json).map_err(json_error)?;
    let scope = scope_of(&value)?;
    let (relationships, entities) = pinned_generation_records(coordinator, store, &scope)?;
    let paths: Vec<String> = entities
        .into_iter()
        .filter(|e| matches!(e.kind, engram_domain::EntityKind::File))
        .map(|e| e.name)
        .collect();
    encode(&cgq::file_dependencies(&relationships, &paths))
}

/// `{scope, query, depth?, maxNodes?, maxEdges?}` -> relevance-seeded bounded
/// subgraph (RFC-0020 Phase 2 explore).
pub fn explore_json(
    store: &Arc<SqlKnowledgeStore>,
    coordinator: &Arc<CodeBindingCoordinator>,
    request_json: String,
) -> Result<String> {
    let value: Value = serde_json::from_str(&request_json).map_err(json_error)?;
    let scope = scope_of(&value)?;
    let query_text = value["query"]
        .as_str()
        .ok_or_else(|| Error::from_reason("explore: `query` must be a string"))?
        .to_owned();
    let depth = value["depth"].as_u64().unwrap_or(2) as u32;
    let max_nodes = value["maxNodes"].as_u64().unwrap_or(24) as usize;
    let max_edges = value["maxEdges"].as_u64().unwrap_or(64) as usize;
    let (relationships, entities) = pinned_generation_records(coordinator, store, &scope)?;
    let names: Vec<(String, Option<String>)> = entities
        .into_iter()
        .map(|e| (e.name, Some(format!("{:?}", e.kind).to_lowercase())))
        .collect();
    encode(&cgq::explore(
        &relationships,
        &names,
        &query_text,
        depth,
        max_nodes,
        max_edges,
    ))
}

#[cfg(test)]
mod pinned_view_tests {
    use super::*;
    use engram_domain::RepoId;
    use engram_integration::code_binding::{
        CodeAuthorityAction, CodeBindingAuthorizer, CodeIngestionLanes,
    };
    use engram_knowledge::KnowledgeRepository;
    use engram_runtime::CoreResult;

    struct AllowAuthorizer;
    #[async_trait::async_trait]
    impl CodeBindingAuthorizer for AllowAuthorizer {
        async fn authorize(
            &self,
            _actor: &engram_domain::Actor,
            _scope: &Scope,
            _repo: &RepoId,
            _action: CodeAuthorityAction,
        ) -> CoreResult<()> {
            Ok(())
        }
    }

    fn scope() -> Scope {
        Scope {
            tenant: "pinned-view".into(),
            subject: None,
            workspace: Some("gate".into()),
            session: None,
            environment: None,
        }
    }

    /// A real git fixture with ONE commit; returns (path, oid).
    fn git_fixture(tag: &str) -> (std::path::PathBuf, String) {
        let dir = std::env::temp_dir().join(format!("pinned-view-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let run = |args: &[&str]| {
            let status = std::process::Command::new("git")
                .args(args)
                .current_dir(&dir)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_SYSTEM", "/dev/null")
                .status()
                .expect("git");
            assert!(status.success(), "git {args:?}");
        };
        run(&["init", "--initial-branch=main", "."]);
        run(&["config", "user.email", "pinned@engram.test"]);
        run(&["config", "user.name", "pinned"]);
        std::fs::write(dir.join("lib.rs"), "pub fn alpha() -> u32 { 1 }\n").unwrap();
        run(&["add", "."]);
        run(&["commit", "--no-gpg-sign", "-m", "one"]);
        let oid = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&dir)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .output()
            .expect("rev-parse");
        (
            dir,
            String::from_utf8(oid.stdout).unwrap().trim().to_owned(),
        )
    }

    /// pinned_generation_view: with ONE eligible published repository, a
    /// COEXISTING unbound legacy relationship and (after a second publish)
    /// the FIRST generation's now-stale records are NEVER served — only the
    /// PINNED generation's records participate (review r6 finding 3).
    #[test]
    fn pinned_generation_view() {
        let dir = std::env::temp_dir().join(format!("pinned-view-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (repo_path, first_oid) = git_fixture("a");
        let knowledge = Arc::new(SqlKnowledgeStore::open_file(dir.join("knowledge.db")).unwrap());
        let cell = Arc::new(
            engram_store_sqlite::knowledge::SqlCodeBindingRepository::open(dir.join("bindings.db"))
                .unwrap(),
        );
        let coordinator = Arc::new(CodeBindingCoordinator::new(
            cell.clone(),
            Arc::new(AllowAuthorizer),
            Vec::new(),
        ));
        let lanes = CodeIngestionLanes {
            knowledge: knowledge.clone(),
            graph: knowledge.clone(),
            retained_root: dir.join("retained"),
        };
        let repo = RepoId("repo-pinned".to_owned());
        let actor = code_graph_actor();
        futures::executor::block_on(coordinator.register(
            &actor,
            &scope(),
            engram_knowledge::RepositoryRegistration {
                repo_id: repo.clone(),
                locator_id: repo_path.to_string_lossy().to_string(),
                acquisition: engram_knowledge::AcquisitionMode::LocalOnly,
            },
        ))
        .unwrap();
        futures::executor::block_on(coordinator.bind(&actor, &scope(), &repo, "refs/heads/main"))
            .unwrap();
        let first = futures::executor::block_on(coordinator.ingest_registered(
            &actor,
            &scope(),
            &repo,
            None,
            None,
            &lanes,
        ))
        .unwrap();
        assert_eq!(first.commit_oid, first_oid);
        let (_, first_entities) =
            pinned_generation_records(&coordinator, &knowledge, &scope()).expect("first view");
        assert!(
            !first_entities.is_empty(),
            "one-function fixture stages an entity"
        );

        let relationship =
            |id: &str, subject: &str, object: &str| engram_domain::KnowledgeRelationship {
                id: engram_domain::Id::from(id),
                graph_id: Some(engram_domain::Id::from("pinned-view-graph")),
                predicate: "calls".to_owned(),
                subject: engram_domain::EntityRef {
                    id: Some(engram_domain::Id::from(subject)),
                    name: Some(subject.to_owned()),
                    kind: Some("function".to_owned()),
                    aliases: Vec::new(),
                },
                object: engram_domain::EntityRef {
                    id: Some(engram_domain::Id::from(object)),
                    name: Some(object.to_owned()),
                    kind: Some("function".to_owned()),
                    aliases: Vec::new(),
                },
                scope: scope(),
                evidence: Vec::new(),
                confidence: None,
                code_resolution: None,
                provenance: engram_domain::Provenance {
                    source: "pinned-view-fixture".to_owned(),
                    actor: code_graph_actor(),
                    observed_at: chrono::Utc::now(),
                    evidence: Vec::new(),
                    derivations: Vec::new(),
                    confidence: Some(1.0),
                    method: Some("test".to_owned()),
                },
                created_at: chrono::Utc::now(),
                updated_at: None,
                archived_at: None,
            };
        let entity = |id: &str, name: &str| {
            let mut entity = first_entities[0].clone();
            entity.id = engram_domain::Id::from(id);
            entity.name = name.to_owned();
            entity
        };
        let put_relationship = |relationship| {
            futures::executor::block_on(knowledge.put_relationship(relationship))
                .expect("stage relationship");
        };
        let put_entity = |entity| {
            futures::executor::block_on(knowledge.put_entity(entity)).expect("stage entity");
        };
        let lineage = |record_id: &str, generation_id: &str| engram_domain::CodeLineage {
            repo_id: repo.clone(),
            generation_id: generation_id.to_owned(),
            source_path: "src/lib.rs".to_owned(),
            contributors: vec![record_id.to_owned()],
            span: None,
        };

        for (id, subject, object) in [
            ("stale-relationship", "stale_caller", "stale_target"),
            ("retired-relationship", "retired_caller", "retired_target"),
            ("unbound-relationship", "unbound_caller", "unbound_target"),
        ] {
            put_relationship(relationship(id, subject, object));
        }
        for (id, name) in [
            ("stale-entity", "stale_seed"),
            ("retired-entity", "retired_seed"),
            ("unbound-entity", "unbound_seed"),
        ] {
            put_entity(entity(id, name));
        }
        for record_id in ["stale-relationship", "stale-entity"] {
            futures::executor::block_on(coordinator.store().put_lineage(
                &scope(),
                record_id,
                lineage(record_id, &first.generation_id),
            ))
            .expect("stage first-generation lineage");
        }

        let run = |args: &[&str]| {
            let status = std::process::Command::new("git")
                .args(args)
                .current_dir(&repo_path)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_SYSTEM", "/dev/null")
                .status()
                .expect("git");
            assert!(status.success(), "git {args:?}");
        };
        std::fs::write(repo_path.join("lib.rs"), "pub fn beta() -> u32 { 2 }\n").unwrap();
        run(&["add", "."]);
        run(&["commit", "--no-gpg-sign", "-m", "two"]);
        let second = futures::executor::block_on(coordinator.ingest_registered(
            &actor,
            &scope(),
            &repo,
            None,
            None,
            &lanes,
        ))
        .unwrap();
        assert_ne!(second.generation_id, first.generation_id);

        let generated_current =
            futures::executor::block_on(coordinator.store().target_lineage(&scope()))
                .expect("current lineage")
                .into_iter()
                .filter(|target| target.generation_id == second.generation_id && !target.retired)
                .map(|target| engram_knowledge::RetirementTarget {
                    repo_id: target.repo_id,
                    generation_id: target.generation_id,
                    record_id: target.record_id,
                })
                .collect::<Vec<_>>();
        assert!(
            !generated_current.is_empty(),
            "the one-function fixture stages current records before explicit assertions"
        );
        futures::executor::block_on(coordinator.store().retire_lineage_targets(
            &scope(),
            &[(repo.clone(), second.binding.epoch)],
            &generated_current,
        ))
        .expect("retire generated current records");

        put_relationship(relationship(
            "eligible-relationship",
            "eligible_caller",
            "eligible_target",
        ));
        put_entity(entity("eligible-entity", "eligible_seed"));
        for record_id in [
            "eligible-relationship",
            "eligible-entity",
            "retired-relationship",
            "retired-entity",
        ] {
            futures::executor::block_on(coordinator.store().put_lineage(
                &scope(),
                record_id,
                lineage(record_id, &second.generation_id),
            ))
            .expect("stage current-generation lineage");
        }
        let binding = futures::executor::block_on(coordinator.store().get_binding(&scope(), &repo))
            .expect("read binding")
            .expect("binding exists");
        futures::executor::block_on(coordinator.store().retire_lineage_targets(
            &scope(),
            &[(repo.clone(), binding.epoch)],
            &[
                engram_knowledge::RetirementTarget {
                    repo_id: repo.0.clone(),
                    generation_id: second.generation_id.clone(),
                    record_id: "retired-relationship".to_owned(),
                },
                engram_knowledge::RetirementTarget {
                    repo_id: repo.0.clone(),
                    generation_id: second.generation_id.clone(),
                    record_id: "retired-entity".to_owned(),
                },
            ],
        ))
        .expect("retire explicit records");

        let (relationships, entities) =
            pinned_generation_records(&coordinator, &knowledge, &scope()).expect("pinned view");
        let relationship_ids = relationships
            .iter()
            .map(|relationship| relationship.id.to_string())
            .collect::<std::collections::BTreeSet<_>>();
        let entity_ids = entities
            .iter()
            .map(|entity| entity.id.to_string())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            relationship_ids,
            std::collections::BTreeSet::from(["eligible-relationship".to_owned()])
        );
        assert_eq!(
            entity_ids,
            std::collections::BTreeSet::from(["eligible-entity".to_owned()])
        );
        let view = futures::executor::block_on(coordinator.eligibility_view(&scope(), None))
            .expect("eligibility view");
        for record_id in ["eligible-relationship", "eligible-entity"] {
            assert_eq!(
                view.classify(record_id, true),
                RecordEligibility::EligibleCode
            );
        }
        for record_id in [
            "stale-relationship",
            "stale-entity",
            "retired-relationship",
            "retired-entity",
            "unbound-relationship",
            "unbound-entity",
        ] {
            assert_eq!(
                view.classify(record_id, true),
                RecordEligibility::IneligibleCode
            );
        }
        let traversal = blast_radius_json(
            &knowledge,
            &coordinator,
            serde_json::json!({ "scope": scope(), "target": "eligible_target" }).to_string(),
        )
        .expect("traversal uses pinned view");
        assert!(
            traversal.contains("eligible_caller"),
            "eligible traversal is served"
        );
        assert!(
            !traversal.contains("unbound"),
            "unbound traversal is denied"
        );
        let seeds = explore_json(
            &knowledge,
            &coordinator,
            serde_json::json!({ "scope": scope(), "query": "eligible_seed" }).to_string(),
        )
        .expect("seed selection uses pinned view");
        assert!(seeds.contains("eligible_seed"), "eligible seed is served");
        assert!(!seeds.contains("unbound"), "unbound seed is denied");

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&repo_path);
    }
}
