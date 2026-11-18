//! Recipe conformance against actual isolated service and embedded stores.
//! Session races and exact-call fault/receipt assertions live in source_sessions.
#![cfg(any(feature = "embedded", feature = "service"))]
#[path = "../../../adapters/helixdb/tests/support/data_fixture.rs"]
mod fixture;

use engram_backend_helixdb::compose;
use engram_domain::*;
use engram_integration::{
    CapabilityPolicy, EmbeddingProviderConfig, EngramConfig, EngramProvider, MigrationManifest,
    MigrationMode, TransactionGuarantee,
};
use engram_memory::PolicyAuthorizer;
use engram_runtime::{CoreError, CoreResult};
use engram_store_helixdb::{HelixStore, codec::Partition};
use fixture::{Fixture, Mode, entity, make, memory, relation, scope};
use futures::executor::block_on;
use serde_json::json;
use std::{path::Path, sync::Arc};

struct Permit;
impl PolicyAuthorizer for Permit {
    fn can_write(&self, _: &Requester, _: &Scope, _: &Policy) -> CoreResult<()> {
        Ok(())
    }
    fn can_retrieve(&self, _: &Requester, _: &Scope, _: &Policy) -> CoreResult<()> {
        Ok(())
    }
    fn can_forget(&self, _: &Requester, _: &Scope, _: &Policy) -> CoreResult<()> {
        Ok(())
    }
}
struct Deny;
impl PolicyAuthorizer for Deny {
    fn can_write(&self, _: &Requester, _: &Scope, _: &Policy) -> CoreResult<()> {
        denied()
    }
    fn can_retrieve(&self, _: &Requester, _: &Scope, _: &Policy) -> CoreResult<()> {
        denied()
    }
    fn can_forget(&self, _: &Requester, _: &Scope, _: &Policy) -> CoreResult<()> {
        denied()
    }
}
fn denied<T>() -> CoreResult<T> {
    Err(CoreError::PolicyDenied {
        reason: "recipe conformance denies authority".into(),
    })
}

/// Preserve cache/store roots on assertion failure rather than erasing failed state.
struct OwnedRoot(Option<tempfile::TempDir>);
impl OwnedRoot {
    fn new(label: &str) -> Self {
        let root = tempfile::Builder::new().prefix(label).tempdir().unwrap();
        eprintln!(
            "HELIX_RECIPE_ROOT {}",
            json!({"phase":"before-acquisition","root":root.path(),"label":label})
        );
        Self(Some(root))
    }
    fn path(&self) -> &Path {
        self.0.as_ref().unwrap().path()
    }
}
impl Drop for OwnedRoot {
    fn drop(&mut self) {
        if std::thread::panicking() {
            if let Some(root) = self.0.take() {
                eprintln!(
                    "HELIX_RECIPE_ROOT {}",
                    json!({"phase":"failure-retain","root":root.keep(),"data_retained":true})
                );
            }
        }
    }
}
fn config(root: &Path) -> EngramConfig {
    EngramConfig::new(
        root.join("cache"),
        root,
        ScopeMappingStrategy::Strict,
        EmbeddingProviderConfig {
            provider_type: "none".into(),
            dimensions: 4,
            ..Default::default()
        },
        MigrationMode::DryRun,
        CapabilityPolicy::FailClosed,
    )
    .with_enable_vector(true)
}
fn space() -> EmbeddingSpace {
    EmbeddingSpace::new("explicit-test-input", "fixture", 4, "query", Some("l2"))
}
fn requester() -> Requester {
    serde_json::from_value(json!({"actor":{"id":"actor","kind":"agent"}})).unwrap()
}
fn write_request(text: &str) -> WriteMemoryRequest {
    let template = memory("template");
    WriteMemoryRequest {
        kind: MemoryKind::Fact,
        content: MemoryContent {
            text: text.into(),
            ..template.content
        },
        scope: scope(),
        requester: requester(),
        provenance: template.provenance,
        policy: template.policy,
        links: vec![],
        verdict: None,
        idempotency_key: Some("recipe-conformance-write".into()),
    }
}
fn retrieval(query: &str) -> RetrievalRequest {
    RetrievalRequest {
        query: query.into(),
        scope: scope(),
        requester: requester(),
        modes: vec![],
        filters: None,
        cues: vec![],
        limit: Some(20),
        budget: None,
        include_explanations: Some(true),
    }
}
fn required_handles(provider: &EngramProvider) {
    provider.require_memory().expect("governed memory");
    provider.require_knowledge().expect("knowledge");
    provider.require_graph().expect("graph");
    provider
        .require_graph_maintenance()
        .expect("graph maintenance");
    provider.require_ontology().expect("ontology");
    provider.require_taxonomy().expect("taxonomy");
    provider.require_beliefs().expect("beliefs");
    provider.require_hierarchy().expect("hierarchy");
    provider.require_retrieval().expect("retrieval");
    provider.require_vectors().expect("vectors");
    provider.require_procedures().expect("procedures");
    provider.require_proposals().expect("proposals");
    provider.require_outcomes().expect("outcomes");
    provider.identity().expect("identity");
    provider.require_change_log().expect("change log");
    provider.require_checkpoints().expect("checkpoints");
    provider.require_provenance().expect("provenance");
    provider.require_batch().expect("batch");
    provider.require_recall().expect("recall");
    provider.require_export_import().expect("archive export");
    provider.require_migration().expect("archive validation");
    provider.require_observability().expect("observability");
    provider
        .require_consolidation()
        .expect("explicit consolidation");
    provider.require_scope_inventory().expect("scope inventory");
    provider.require_knowledge_query().expect("knowledge query");
    provider.require_lexical_feed().expect("lexical feed");
    assert!(provider.community_query().is_some(), "community query");
    provider
        .code_bindings()
        .expect("canonical code coordinator");
    provider.code_ingestion().expect("owned code ingestion");
    provider
        .retained_code_source()
        .expect("owned retained-source reader");
    let caps = provider.capabilities();
    for (name, state) in [
        ("memory", &caps.memory),
        ("knowledge", &caps.knowledge),
        ("graph", &caps.graph),
        ("ontology", &caps.ontology),
        ("taxonomy", &caps.taxonomy),
        ("beliefs", &caps.beliefs),
        ("hierarchy", &caps.hierarchy),
        ("retrieval", &caps.retrieval),
        ("vectors", &caps.vectors),
        ("batch", &caps.atomic_batch),
        ("recall", &caps.unified_recall),
        ("archive", &caps.export_import),
        ("validation", &caps.migration),
        ("observability", &caps.observability),
        ("consolidation", &caps.consolidation),
    ] {
        assert_eq!(
            *state,
            CapabilityState::Supported,
            "{name} must describe the selected recipe"
        );
    }
    assert_eq!(
        provider.require_batch().unwrap().transaction_guarantee(),
        TransactionGuarantee::BestEffort
    );
}

async fn families(provider: &EngramProvider) {
    let s = scope();
    let knowledge = provider.require_knowledge().unwrap();
    knowledge
        .put_source(make("source", json!({"kind":"generated"})))
        .await
        .unwrap();
    knowledge
        .put_document(make(
            "document",
            json!({"sourceId":"source","kind":"text","contentHash":"hash"}),
        ))
        .await
        .unwrap();
    knowledge.put_chunk(make("chunk",json!({"sourceId":"source","documentId":"document","kind":"paragraph","text":"recipe anchor chunk","contentHash":"hash"}))).await.unwrap();
    let invalid_child: KnowledgeChunk = make(
        "orphan-chunk",
        json!({"sourceId":"missing-source","documentId":"document","kind":"paragraph","text":"must not land","contentHash":"hash"}),
    );
    assert!(
        knowledge.put_chunk(invalid_child).await.is_err(),
        "missing inherited parent must reject without a child write"
    );
    let graph = provider.require_graph().unwrap();
    graph
        .put_graph(make("graph", json!({"name":"recipe graph"})))
        .await
        .unwrap();
    knowledge
        .put_entity(entity("left", &["source"]))
        .await
        .unwrap();
    knowledge
        .put_entity(entity("right", &["source"]))
        .await
        .unwrap();
    knowledge
        .put_relationship(relation("edge", &["source"]))
        .await
        .unwrap();
    assert_eq!(
        graph
            .neighbors(&"graph".into(), &"left".into(), &s, None)
            .await
            .unwrap()
            .len(),
        1
    );
    let identity = provider.identity().unwrap();
    assert!(matches!(
        identity
            .resolve_or_put_entity(EntityWriteRequest {
                entity: entity("identified", &[]),
                identity: EntityIdentityMode::StableKey {
                    key: "recipe-identity".into()
                },
                merge_policy: Default::default()
            })
            .await
            .unwrap(),
        EntityWriteOutcome::Created { .. }
    ));
    let ontology = provider.require_ontology().unwrap();
    let ont: Ontology = make(
        "ontology",
        json!({"uri":"urn:recipe:ontology","language":"custom","version":"1","status":"active"}),
    );
    ontology.put_ontology(ont.clone()).await.unwrap();
    assert_eq!(ontology.get_ontology(&ont.id, &s).await.unwrap(), Some(ont));
    let taxonomy = provider.require_taxonomy().unwrap();
    taxonomy
        .put_concept_scheme(make(
            "scheme",
            json!({"uri":"urn:recipe:scheme","version":"1"}),
        ))
        .await
        .unwrap();
    taxonomy.put_concept(make("concept",json!({"schemeId":"scheme","uri":"urn:recipe:concept","prefLabel":{"value":"recipe anchor"},"status":"active"}))).await.unwrap();
    assert_eq!(
        taxonomy
            .list_concepts(&"scheme".into(), &s)
            .await
            .unwrap()
            .len(),
        1
    );
    let orphan: Concept = make(
        "orphan-concept",
        json!({"schemeId":"missing-scheme","uri":"urn:orphan","prefLabel":{"value":"must not land"},"status":"active"}),
    );
    assert!(
        taxonomy.put_concept(orphan).await.is_err(),
        "concept authority must come from the retained scheme"
    );
    let beliefs = provider.require_beliefs().unwrap();
    let start: Timestamp = "2026-11-17T00:00:00Z".parse().unwrap();
    let end = start + chrono::Duration::hours(1);
    let belief: Belief = make(
        "belief",
        json!({"subject":{"key":"anchor"},"content":"recipe anchor belief","status":"active","confidence":0.8,"validFrom":start,"validUntil":end}),
    );
    beliefs.put_belief(belief.clone()).await.unwrap();
    assert!(
        beliefs
            .get_belief(engram_belief::BeliefQuery::live_subject(
                s.clone(),
                "anchor",
                start
            ))
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        beliefs
            .get_belief(engram_belief::BeliefQuery::live_subject(
                s.clone(),
                "anchor",
                end
            ))
            .await
            .unwrap()
            .is_none()
    );
    let mut history = engram_belief::BeliefQuery::live_subject(s.clone(), "anchor", start);
    history.recorded_at = Some(start);
    assert!(matches!(
        beliefs.get_belief(history).await,
        Err(CoreError::InvalidRequest { .. })
    ));
    let hierarchy = provider.require_hierarchy().unwrap();
    hierarchy
        .put_node(make(
            "root",
            json!({"kind":"aggregate","layer":0,"status":"active"}),
        ))
        .await
        .unwrap();
    hierarchy.put_node(make("child",json!({"kind":"base","layer":1,"parentId":"root","sourceTargetId":"seed","status":"active"}))).await.unwrap();
    hierarchy
        .put_relation(make(
            "hierarchy-edge",
            json!({"sourceId":"child","targetId":"root","predicate":"part_of"}),
        ))
        .await
        .unwrap();
    assert_eq!(
        hierarchy
            .path_for(&["seed".into()], &s, None)
            .await
            .unwrap()
            .nodes
            .len(),
        2
    );
    let procedures = provider.require_procedures().unwrap();
    let procedure: Procedure = make(
        "procedure",
        json!({"name":"recipe anchor runbook","steps":["check recipe anchor"]}),
    );
    procedures
        .upsert_procedure(procedure.clone())
        .await
        .unwrap();
    procedures
        .increment_success(&procedure.id, &s)
        .await
        .unwrap();
    assert_eq!(
        procedures.procedure_stats(&s).await.unwrap().total_success,
        1
    );
    let outcomes = provider.require_outcomes().unwrap();
    let request:ReportOutcomeRequest=serde_json::from_value(json!({"queryId":"recipe-query","scope":s,"recalledTargets":["belief"],"outcome":{"success":true,"verifier":{"id":"host","kind":"system"}},"occurredAt":start})).unwrap();
    outcomes.report_outcome(request).await.unwrap();
    assert_eq!(
        outcomes
            .list_outcomes(&OutcomeWindow {
                scope: s.clone(),
                since: start,
                until: end
            })
            .await
            .unwrap()
            .len(),
        1
    );
    let provenance = provider.require_provenance().unwrap();
    assert!(
        provenance
            .provenance_for(EvidenceTargetType::Entity, "left", &s)
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(
        provider
            .require_knowledge_query()
            .unwrap()
            .list_chunks(&s)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        provider
            .community_query()
            .unwrap()
            .overview(&s, 10)
            .await
            .unwrap()
            .total_communities
            > 0
    );
    provider
        .require_lexical_feed()
        .unwrap()
        .upsert("left", "recipe anchor")
        .await
        .unwrap();
    let vectors = provider.require_vectors().unwrap();
    vectors
        .insert(&"chunk".into(), &space(), vec![1.0, 0.0, 0.0, 0.0])
        .await
        .unwrap();
    assert_eq!(
        vectors.vector_for_target(&"chunk".into()).await.unwrap(),
        Some(vec![1.0, 0.0, 0.0, 0.0])
    );
    assert!(
        vectors
            .insert(&"invalid-vector".into(), &space(), vec![1.0])
            .await
            .is_err()
    );
    let candidates = provider
        .require_retrieval()
        .unwrap()
        .retrieve_candidates(&retrieval("anchor"))
        .await
        .unwrap();
    assert!(
        !candidates.is_empty(),
        "real retrieval contributor must return seeded content"
    );
    let mut temporal = retrieval("anchor");
    temporal.modes = vec![RetrievalMode::Temporal];
    temporal.filters = Some(
        serde_json::from_value(
            json!({"since":"2026-11-16T00:00:00Z","until":"2026-11-18T00:00:00Z"}),
        )
        .unwrap(),
    );
    assert!(
        !provider
            .require_retrieval()
            .unwrap()
            .retrieve_candidates(&temporal)
            .await
            .unwrap()
            .is_empty(),
        "governed temporal candidate path"
    );
    temporal.filters =
        Some(serde_json::from_value(json!({"since":"2099-01-01T00:00:00Z"})).unwrap());
    assert!(
        provider
            .require_retrieval()
            .unwrap()
            .retrieve_candidates(&temporal)
            .await
            .unwrap()
            .is_empty(),
        "time filtering precedes candidate selection"
    );
    assert!(
        !provider
            .require_recall()
            .unwrap()
            .recall(retrieval("anchor"))
            .await
            .unwrap()
            .items
            .is_empty()
    );
    let changes = provider
        .require_change_log()
        .unwrap()
        .changes_since(&KnowledgeChangesRequest {
            scope: s.clone(),
            source_key: None,
            after: None,
            limit: 100,
        })
        .await
        .unwrap();
    assert!(!changes.items.is_empty());
    let checkpoint:ScanCheckpoint=serde_json::from_value(json!({"scope":s,"stableSourceKey":"repo@main","scanRunId":"recipe-run","startedAt":start,"status":"completed","indexedRevision":"a".repeat(40),"errors":0})).unwrap();
    provider
        .require_checkpoints()
        .unwrap()
        .put_checkpoint(checkpoint.clone())
        .await
        .unwrap();
    assert_eq!(
        provider
            .require_change_log()
            .unwrap()
            .latest_checkpoints(&s)
            .await
            .unwrap(),
        vec![checkpoint]
    );
}

fn conformance(mode: Mode) {
    block_on(async {
        let root = OwnedRoot::new("helix-recipe-conformance-");
        let mut f = Fixture::with_space(mode, Some(space()));
        let config = config(root.path());
        let id;
        {
            let provider =
                compose(f.store.clone(), &config, Arc::new(Permit)).expect("complete recipe");
            required_handles(&provider);
            let memory = provider.require_memory().unwrap();
            let request = write_request("recipe anchor memory");
            let written = memory.write_memory(request.clone()).await.unwrap();
            id = written.record.id.clone();
            let retry = memory.write_memory(request).await.unwrap();
            assert_eq!(retry.record, written.record);
            assert_eq!(retry.event, written.event);
            assert_eq!(retry.deduplicated, Some(true));
            assert_eq!(
                memory
                    .list_events_for_memory(&id, &scope())
                    .await
                    .unwrap()
                    .len(),
                1
            );
            assert!(
                memory
                    .retrieve(retrieval("anchor"))
                    .await
                    .unwrap()
                    .items
                    .iter()
                    .any(|item| item.target_id == id.to_string())
            );
            for dimension in ["tenant", "subject", "workspace", "session", "environment"] {
                let mut foreign = serde_json::to_value(scope()).unwrap();
                foreign[dimension] = json!("foreign");
                let foreign: Scope = serde_json::from_value(foreign).unwrap();
                assert!(memory.get_memory(&id, &foreign).await.unwrap().is_none());
                assert!(f.store.composition_inventory(&foreign).is_err());
            }
            families(&provider).await;
            let archive = provider.require_export_import().unwrap();
            let before = archive.export(&scope()).await.unwrap();
            assert_eq!(before.memories.len(), 1);
            assert_eq!(before.knowledge_sources.len(), 1);
            assert_eq!(before.knowledge_documents.len(), 1);
            assert_eq!(before.knowledge_chunks.len(), 1);
            assert_eq!(before.concept_schemes.len(), 1);
            assert_eq!(before.concepts.len(), 1);
            assert_eq!(before.beliefs.len(), 1);
            assert_eq!(before.hierarchy_nodes.len(), 2);
            assert!(before.vectors.is_empty());
            for inherited in [
                &before.knowledge_documents[0].scope,
                &before.knowledge_chunks[0].scope,
                &before.concepts[0].scope,
            ] {
                assert_eq!(serde_json::from_str::<Scope>(inherited).unwrap(), scope());
            }
            let migration = provider.require_migration().unwrap();
            let report = migration.dry_run_import(&before).unwrap();
            assert_eq!(report.row_counts.memory, 1);
            let valid = MigrationManifest {
                import_data: before.clone(),
                validation_report: report,
            };
            let mut stale = valid.clone();
            stale.validation_report.manifest_fingerprint = "stale".into();
            let mut invalid = valid.clone();
            invalid.import_data.memories[0].scope.clear();
            for manifest in [valid, stale, invalid] {
                assert!(matches!(
                    migration.apply_import(&manifest),
                    Err(CoreError::CapabilityUnsupported { .. })
                ));
            }
            let snapshot = serde_json::to_value(&before).unwrap();
            assert_eq!(
                serde_json::to_value(archive.export(&scope()).await.unwrap()).unwrap(),
                snapshot
            );
            for dry_run in [None, Some(true)] {
                provider
                    .require_consolidation()
                    .unwrap()
                    .consolidate(ConsolidationRequest {
                        scope: scope(),
                        requester: requester(),
                        since: None,
                        until: None,
                        strategy: Some(ConsolidationStrategy::Manual),
                        dry_run,
                    })
                    .await
                    .unwrap();
                assert_eq!(
                    serde_json::to_value(archive.export(&scope()).await.unwrap()).unwrap(),
                    snapshot,
                    "preview must not mutate families"
                );
            }
            let diagnostics = provider
                .require_observability()
                .unwrap()
                .diagnostics()
                .await
                .unwrap();
            assert_eq!(diagnostics.capabilities, *provider.capabilities());
            assert_eq!(diagnostics.record_counts.entities, 3);
            assert_eq!(diagnostics.record_counts.beliefs, 1);
            let inventory = provider
                .require_scope_inventory()
                .unwrap()
                .inventory()
                .await
                .unwrap();
            assert!(inventory.iter().any(|row| row.scope == scope()
                && row.memory_count == 1
                && row.entity_count == 3
                && row.belief_count == 1));
            assert!(
                matches!(
                    provider
                        .code_bindings()
                        .unwrap()
                        .authorize(&requester().actor, &scope(), &RepoId("repo".into()))
                        .await,
                    Err(CoreError::PolicyDenied { .. })
                ),
                "default code coordinator must deny caller authority"
            );
            let proposal_payload:ProposalMemoryWrite=serde_json::from_value(json!({"kind":"fact","content":{"text":"inert proposal"},"policy":{"visibility":"workspace","retention":"durable","allowedUses":["retrieval"]}})).unwrap();
            assert!(
                matches!(
                    provider
                        .require_proposals()
                        .unwrap()
                        .submit(ProposalSubmitRequest {
                            scope: scope(),
                            payload: proposal_payload,
                            submitted_by: requester().actor,
                            submission_key: None
                        })
                        .await,
                    Err(CoreError::PolicyDenied { .. })
                ),
                "proposal authority remains separately default-deny"
            );
            assert!(matches!(
                provider
                    .require_proposals()
                    .unwrap()
                    .list(ProposalListRequest {
                        scope: scope(),
                        status: None,
                        after: None,
                        limit: 10
                    })
                    .await,
                Err(CoreError::PolicyDenied { .. })
            ));
            let applied = provider
                .require_consolidation()
                .unwrap()
                .consolidate(ConsolidationRequest {
                    scope: scope(),
                    requester: requester(),
                    since: None,
                    until: None,
                    strategy: Some(ConsolidationStrategy::Manual),
                    dry_run: Some(false),
                })
                .await
                .unwrap();
            assert!(matches!(
                applied.status,
                ConsolidationRunStatus::Completed | ConsolidationRunStatus::CompletedWithErrors
            ));
            let mut denied_config = config.clone();
            denied_config.storage_path = root.path().join("denied-cache");
            let denied_provider = compose(f.store.clone(), &denied_config, Arc::new(Deny)).unwrap();
            assert!(matches!(
                denied_provider
                    .require_memory()
                    .unwrap()
                    .write_memory(write_request("denied"))
                    .await,
                Err(CoreError::PolicyDenied { .. })
            ));
            assert!(
                denied_provider
                    .require_memory()
                    .unwrap()
                    .retrieve(retrieval("anchor"))
                    .await
                    .unwrap()
                    .items
                    .is_empty()
            );
            assert!(matches!(
                denied_provider
                    .require_memory()
                    .unwrap()
                    .forget(ForgetRequest {
                        target_type: ForgetTargetType::Memory,
                        target_id: id.to_string(),
                        scope: scope(),
                        requester: requester(),
                        mode: DeleteMode::Delete,
                        reason: None
                    })
                    .await,
                Err(CoreError::PolicyDenied { .. })
            ));
        }
        f.reopen();
        let provider = compose(f.store.clone(), &config, Arc::new(Permit)).unwrap();
        let memory = provider.require_memory().unwrap();
        assert!(memory.get_memory(&id, &scope()).await.unwrap().is_some());
        memory
            .forget(ForgetRequest {
                target_type: ForgetTargetType::Memory,
                target_id: id.to_string(),
                scope: scope(),
                requester: requester(),
                mode: DeleteMode::Delete,
                reason: None,
            })
            .await
            .unwrap();
        assert!(memory.get_memory(&id, &scope()).await.unwrap().is_none());
        assert_eq!(
            memory
                .list_events_for_memory(&id, &scope())
                .await
                .unwrap()
                .len(),
            2
        );
    });
}
#[cfg(feature = "embedded")]
#[test]
fn embedded_recipe_conformance() {
    conformance(Mode::Embedded)
}
#[cfg(feature = "service")]
#[test]
fn service_recipe_conformance() {
    conformance(Mode::Service)
}

fn open_owned(
    mode: Mode,
    root: &Path,
    partition: &Partition,
    read_only: bool,
) -> Result<HelixStore, engram_store_helixdb::error::Error> {
    eprintln!(
        "HELIX_RECIPE_ROOT {}",
        json!({"phase":"opening-owned-store","root":root,"mode":format!("{mode:?}"),"scope":partition.scope,"logical_store":partition.logical_store,"read_only":read_only})
    );
    match mode {
        #[cfg(feature = "embedded")]
        Mode::Embedded => HelixStore::open_embedded(
            engram_store_helixdb::config::EmbeddedConfig {
                trusted_root: root.into(),
                path: root.join("store"),
                read_only,
                queue_capacity: 8,
            },
            partition.clone(),
        ),
        #[cfg(feature = "service")]
        Mode::Service => HelixStore::open_service(
            engram_store_helixdb::config::ServiceConfig {
                endpoint: std::env::var("HELIX_TEST_ENDPOINT")
                    .expect("actual service endpoint required"),
                credential_env: std::env::var("HELIX_TEST_CREDENTIAL_ENV")
                    .expect("explicit credential environment reference required"),
                vectors_enabled: true,
                read_only,
                timeout: std::time::Duration::from_secs(120),
            },
            partition.clone(),
        ),
    }
}
fn resources(mode: Mode) {
    let root = OwnedRoot::new("helix-recipe-resource-");
    let partition = Partition {
        logical_store: format!("recipe-conformance-{}", uuid::Uuid::new_v4()),
        scope: scope(),
        embedding_space: Some(space()),
    };
    let initial = open_owned(mode, root.path(), &partition, false).expect("initial owned writer");
    let provider =
        compose(initial, &config(root.path()), Arc::new(Permit)).expect("resource recipe");
    let retained = provider.require_memory().unwrap().clone();
    drop(provider);
    assert!(
        matches!(
            open_owned(mode, root.path(), &partition, false),
            Err(engram_store_helixdb::error::Error::Contention)
        ),
        "retained capability must retain exact writer ownership"
    );
    drop(retained);
    let reopened = open_owned(mode, root.path(), &partition, false)
        .expect("final capability release closes owned runtime and lease");
    reopened
        .put_sentinel("resource-control", "still-available")
        .unwrap();
    if matches!(mode_name(mode), "service") {
        let second =
            open_owned(mode, root.path(), &partition, true).expect("independent readonly client");
        reopened.close().expect("close only this service client");
        assert_eq!(
            second.get_sentinel("resource-control").unwrap().as_deref(),
            Some("still-available"),
            "closing one client must leave service/other client available"
        );
        second.close().expect("second client close");
    } else {
        reopened.close().expect("owned embedded close");
    }
    drop(reopened);
    let startup =
        open_owned(mode, root.path(), &partition, false).expect("startup-failure candidate writer");
    let mut escaped = config(root.path());
    escaped.storage_path = root.path().join(format!(
        "../forbidden-recipe-cache-{}",
        uuid::Uuid::new_v4()
    ));
    assert!(!escaped.storage_path.exists());
    assert!(
        compose(startup, &escaped, Arc::new(Permit)).is_err(),
        "escaping cache path must fail before creation"
    );
    assert!(!escaped.storage_path.exists());
    let recovered = open_owned(mode, root.path(), &partition, false)
        .expect("failed recipe startup releases its exact owned writer");
    recovered.close().expect("confirmed final owned cleanup");
}
fn mode_name(mode: Mode) -> &'static str {
    match mode {
        #[cfg(feature = "embedded")]
        Mode::Embedded => "embedded",
        #[cfg(feature = "service")]
        Mode::Service => "service",
    }
}

fn readonly_archive(mode: Mode) {
    block_on(async {
        let root = OwnedRoot::new("helix-recipe-readonly-archive-");
        let partition = Partition {
            logical_store: format!("readonly-archive-{}", uuid::Uuid::new_v4()),
            scope: scope(),
            embedding_space: Some(space()),
        };
        let cfg = config(root.path());
        let writer = open_owned(mode, root.path(), &partition, false).unwrap();
        writer.put_sentinel("readonly-control", "present").unwrap();
        let provider = compose(writer.clone(), &cfg, Arc::new(Permit)).unwrap();
        let knowledge = provider.require_knowledge().unwrap();
        knowledge
            .put_source(make("ro-source", json!({"kind":"generated"})))
            .await
            .unwrap();
        knowledge
            .put_document(make(
                "ro-document",
                json!({"sourceId":"ro-source","kind":"text","contentHash":"hash"}),
            ))
            .await
            .unwrap();
        knowledge.put_chunk(make("ro-chunk", json!({"sourceId":"ro-source","documentId":"ro-document","kind":"paragraph","text":"read-only archive","contentHash":"hash"}))).await.unwrap();
        provider
            .require_memory()
            .unwrap()
            .write_memory(write_request("read-only archive memory"))
            .await
            .unwrap();
        provider
            .require_taxonomy()
            .unwrap()
            .put_concept_scheme(make(
                "ro-scheme",
                json!({"uri":"urn:ro:scheme","version":"1"}),
            ))
            .await
            .unwrap();
        provider.require_taxonomy().unwrap().put_concept(make("ro-concept", json!({"schemeId":"ro-scheme","uri":"urn:ro:concept","prefLabel":{"value":"read-only"},"status":"active"}))).await.unwrap();
        let expected = provider
            .require_export_import()
            .unwrap()
            .export(&scope())
            .await
            .unwrap();
        drop(provider);
        writer.close().unwrap();
        drop(writer);
        eprintln!(
            "HELIX_READONLY_ARCHIVE mode={mode:?} writer_close=CONFIRMED phase=valid-reader-open"
        );

        let mut reader = open_owned(mode, root.path(), &partition, true).unwrap();
        assert_eq!(
            reader.get_sentinel("readonly-control").unwrap().as_deref(),
            Some("present")
        );
        #[cfg(feature = "test-controls")]
        let control = reader.instrument_native_test_control().unwrap();
        assert!(
            reader.native_session(&scope()).is_err(),
            "read snapshot must not acquire writer authority"
        );
        let provider = compose(reader.clone(), &cfg, Arc::new(Permit)).unwrap();
        #[cfg(feature = "test-controls")]
        let before = control.snapshot();
        let exported = provider
            .require_export_import()
            .unwrap()
            .export(&scope())
            .await
            .unwrap();
        assert_eq!(
            serde_json::to_value(&exported).unwrap(),
            serde_json::to_value(&expected).unwrap()
        );
        assert_eq!(exported.memories.len(), 1);
        assert_eq!(exported.knowledge_sources.len(), 1);
        assert_eq!(exported.knowledge_documents.len(), 1);
        assert_eq!(exported.knowledge_chunks.len(), 1);
        assert_eq!(exported.concept_schemes.len(), 1);
        assert_eq!(exported.concepts.len(), 1);
        for inherited in [
            &exported.knowledge_documents[0].scope,
            &exported.knowledge_chunks[0].scope,
            &exported.concepts[0].scope,
        ] {
            assert_eq!(serde_json::from_str::<Scope>(inherited).unwrap(), scope());
        }
        #[cfg(feature = "test-controls")]
        {
            let after = control.snapshot();
            assert_eq!(
                after.all_sends,
                before.all_sends + 1,
                "one pure native read batch"
            );
            assert!(
                after.mutation_attempts.is_empty() && after.landed_commits.is_empty(),
                "read operations are not storage commits"
            );
        }
        let mut foreign = scope();
        foreign.subject = Some(String::new());
        assert!(
            provider
                .require_export_import()
                .unwrap()
                .export(&foreign)
                .await
                .is_err()
        );
        assert!(
            provider
                .require_knowledge()
                .unwrap()
                .put_source(make("ro-denied", json!({"kind":"generated"})))
                .await
                .is_err()
        );
        let orphan: KnowledgeChunk = make(
            "ro-denied-child",
            json!({"sourceId":"missing","documentId":"missing","kind":"paragraph","text":"denied","contentHash":"hash"}),
        );
        assert!(
            provider
                .require_knowledge()
                .unwrap()
                .put_chunk(orphan)
                .await
                .is_err()
        );
        assert!(reader.scope_session().is_err());
        #[cfg(feature = "test-controls")]
        assert!(control.snapshot().mutation_attempts.is_empty());
        drop(provider);
        reader.close().unwrap();
        drop(reader);
        #[cfg(feature = "test-controls")]
        drop(control);
        eprintln!(
            "HELIX_READONLY_ARCHIVE mode={mode:?} reader_close=CONFIRMED nonempty_export=PASS native_write_steps=0 commit_ids=0 writer_authority=false"
        );

        // A legal raw parent deletion leaves an orphan child. The same snapshot
        // validation must fail closed on a read-only owner, never omit the child.
        let writer = open_owned(mode, root.path(), &partition, false).unwrap();
        let provider = compose(writer.clone(), &cfg, Arc::new(Permit)).unwrap();
        assert!(
            provider
                .require_knowledge()
                .unwrap()
                .delete_document(&"ro-document".into(), &scope())
                .await
                .unwrap()
        );
        drop(provider);
        writer.close().unwrap();
        drop(writer);
        let reader = open_owned(mode, root.path(), &partition, true).unwrap();
        let provider = compose(reader.clone(), &cfg, Arc::new(Permit)).unwrap();
        assert!(
            provider
                .require_export_import()
                .unwrap()
                .export(&scope())
                .await
                .is_err(),
            "missing inherited parent must reject the complete snapshot"
        );
        drop(provider);
        reader.close().unwrap();
        eprintln!(
            "HELIX_READONLY_ARCHIVE mode={mode:?} missing_parent=DENIED final_reader_close=CONFIRMED"
        );
    });
}

#[cfg(feature = "embedded")]
#[test]
fn embedded_recipe_readonly_archive_preserves_authority() {
    readonly_archive(Mode::Embedded)
}
#[cfg(feature = "service")]
#[test]
fn service_recipe_readonly_archive_preserves_authority() {
    readonly_archive(Mode::Service)
}
#[cfg(feature = "embedded")]
#[test]
fn embedded_recipe_owned_resources_and_path_rejection() {
    resources(Mode::Embedded)
}
#[cfg(feature = "service")]
#[test]
fn service_recipe_owned_resources_and_path_rejection() {
    resources(Mode::Service)
}

#[cfg(unix)]
fn symlink_escape(mode: Mode) {
    let root = OwnedRoot::new("helix-recipe-symlink-");
    let outside = root.path().join("outside");
    std::fs::create_dir(&outside).unwrap();
    let trusted = root.path().join("trusted");
    std::fs::create_dir(&trusted).unwrap();
    std::os::unix::fs::symlink(&outside, trusted.join("escape")).unwrap();
    let partition = Partition {
        logical_store: format!("recipe-symlink-{}", uuid::Uuid::new_v4()),
        scope: scope(),
        embedding_space: Some(space()),
    };
    let store = open_owned(mode, &trusted, &partition, false).unwrap();
    let mut escaped = config(&trusted);
    escaped.storage_path = trusted.join("escape/cache");
    assert!(compose(store, &escaped, Arc::new(Permit)).is_err());
    assert_eq!(
        std::fs::read_dir(&outside).unwrap().count(),
        0,
        "escaped target remains untouched"
    );
    let reopened = open_owned(mode, &trusted, &partition, false)
        .expect("symlink rejection releases owned startup writer");
    reopened.close().unwrap();
}
#[cfg(all(unix, feature = "embedded"))]
#[test]
fn embedded_recipe_symlink_escape_does_not_touch_target() {
    symlink_escape(Mode::Embedded)
}
#[cfg(all(unix, feature = "service"))]
#[test]
fn service_recipe_symlink_escape_does_not_touch_target() {
    symlink_escape(Mode::Service)
}
