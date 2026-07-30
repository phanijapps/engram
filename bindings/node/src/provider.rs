//! Provider-pattern N-API surface (T5 of the integration-host-facade-v2 spec).
//!
//! [`NativeProvider`] wraps [`EngramProvider::open`] and exposes the provider
//! pattern to TypeScript consumers: open one provider from a config, read its
//! capability report, and reach each wired service through a typed handle proxy
//! (`NativeMemoryApi`, `NativeGraphApi`, …). Each proxy holds an
//! `Arc<dyn Trait>` cloned out of the provider and exposes its key operations
//! as JSON-in / JSON-out methods — the same transport used by
//! [`crate::NativeKnowledgeEngine`], but discovered through the provider facade
//! with explicit capability gating (`require_*` throws a typed error when a
//! family is not wired, instead of returning empty results).
//!
//! This module is additive: [`crate::NativeKnowledgeEngine`] is untouched and
//! remains the flat engine that `engram-viz` and the MCP server use today.
//!
//! N-API passes strings (JSON), not Rust trait objects, so the "typed" methods
//! are still JSON at the boundary. The win is discoverability (TypeScript sees
//! the provider pattern, not a flat 47-method engine) and capability gating.
//! Async trait methods are driven synchronously via `futures::executor::block_on`
//! — the established binding pattern, matching `NativeKnowledgeEngine`.

use engram_belief::{BeliefQuery, BeliefRepository};
use engram_domain::{
    Belief, ConsolidationRequest, ContextPayload, EvidenceRef, EvidenceTargetType, ForgetRequest,
    ForgetResult, Id, KnowledgeEntity, Procedure, Provenance, RetrievalRequest, Scope,
    WriteMemoryRequest, WriteMemoryResponse,
};
use engram_hierarchy::HierarchyRepository;
use engram_integration::{
    BatchIngest, BatchIngestRequest, BatchOutcome, BatchStatus, BatchStep, EmbeddingProvider,
    EngramConfig, EngramProvider, ExportImport, KnowledgeQuery, LexicalFeed, MigrationService,
    Observability, ProvenanceQuery, StepStatus, TransactionGuarantee, UnifiedRecall,
};
use engram_knowledge::{
    KnowledgeGraphRepository, KnowledgeRepository, OntologyRepository, TaxonomyRepository,
};
use engram_memory::MemoryService;
use engram_procedures::ProcedureRepository;
use engram_retrieval::{RetrievalIndex, VectorIndex};
use futures::executor::block_on;
use napi::bindgen_prelude::*;
use napi_derive::napi;
use serde::Serialize;
use std::sync::Arc;

use crate::{decode, encode, id_field, scope_field, to_napi_error};

// ---------------------------------------------------------------------------
// NativeProvider — constructor + capability gateway
// ---------------------------------------------------------------------------

/// Provider-pattern entry point for TypeScript consumers.
///
/// Open one from an `EngramConfig` JSON string (or a profile file), read its
/// capability report via [`Self::capabilities_json`], and reach each wired
/// service through a typed handle proxy (`require_memory_api`,
/// `require_graph_api`, …). Each `require_*` method throws a typed N-API error
/// when that capability family is not wired by the active backend.
#[napi]
pub struct NativeProvider {
    inner: EngramProvider,
}

#[napi]
impl NativeProvider {
    /// Opens a provider from an `EngramConfig` JSON string.
    ///
    /// The config must match `EngramConfig`'s serialized shape (or include a
    /// backend profile). With the `sqlite` feature enabled on the binding, this
    /// constructs every file-backed store and returns a wired provider; throws
    /// `CapabilityUnsupported` when no backend feature is enabled.
    #[napi(constructor)]
    pub fn new(config_json: String) -> Result<Self> {
        let config: EngramConfig = decode(&config_json)?;
        let provider = EngramProvider::open(&config).map_err(to_napi_error)?;
        Ok(Self { inner: provider })
    }

    /// Opens a provider from a profile file path (e.g. `semantic-engine.toml`).
    #[napi(js_name = "fromProfileFile")]
    pub fn from_profile_file(path: String) -> Result<Self> {
        let config = EngramConfig::from_profile_file(&path).map_err(Error::from_reason)?;
        let provider = EngramProvider::open(&config).map_err(to_napi_error)?;
        Ok(Self { inner: provider })
    }

    /// Returns the capability report (19 keys) as a JSON string.
    #[napi(js_name = "capabilitiesJson")]
    pub fn capabilities_json(&self) -> Result<String> {
        encode(self.inner.capabilities())
    }

    // ---- require_*: one per handle proxy. Each clones the Arc<dyn Trait>
    // out of the provider's Option, throwing CapabilityUnsupported when the
    // family is not wired.

    /// Returns a memory API handle, or throws if memory is not wired.
    #[napi(js_name = "requireMemoryApi")]
    pub fn require_memory_api(&self) -> Result<NativeMemoryApi> {
        let handle = self.inner.require_memory().map_err(to_napi_error)?.clone();
        Ok(NativeMemoryApi { handle })
    }

    /// Returns a graph API handle (entity/relationship reads + neighbors), or
    /// throws if either knowledge or graph is not wired. Both handles are
    /// required because entity/relationship reads live on
    /// `KnowledgeRepository` while neighbors live on `KnowledgeGraphRepository`.
    #[napi(js_name = "requireGraphApi")]
    pub fn require_graph_api(&self) -> Result<NativeGraphApi> {
        let knowledge = self
            .inner
            .require_knowledge()
            .map_err(to_napi_error)?
            .clone();
        let graph = self.inner.require_graph().map_err(to_napi_error)?.clone();
        Ok(NativeGraphApi { knowledge, graph })
    }

    /// Returns a provenance / evidence handle, or throws if not wired.
    #[napi(js_name = "requireProvenanceApi")]
    pub fn require_provenance_api(&self) -> Result<NativeProvenanceApi> {
        let handle = self
            .inner
            .require_provenance()
            .map_err(to_napi_error)?
            .clone();
        Ok(NativeProvenanceApi { handle })
    }

    /// Returns a batch-ingest handle, or throws if not wired.
    #[napi(js_name = "requireBatchApi")]
    pub fn require_batch_api(&self) -> Result<NativeBatchApi> {
        let handle = self.inner.require_batch().map_err(to_napi_error)?.clone();
        Ok(NativeBatchApi { handle })
    }

    /// Returns a unified-recall handle, or throws if not wired.
    #[napi(js_name = "requireRecallApi")]
    pub fn require_recall_api(&self) -> Result<NativeRecallApi> {
        let handle = self.inner.require_recall().map_err(to_napi_error)?.clone();
        Ok(NativeRecallApi { handle })
    }

    /// Returns a consolidation handle, or throws if not wired.
    #[napi(js_name = "consolidateJson")]
    pub fn consolidate_json(&self, request_json: String) -> Result<String> {
        let request: ConsolidationRequest = decode(&request_json)?;
        let handle = self
            .inner
            .require_consolidation()
            .map_err(to_napi_error)?
            .clone();
        let run = block_on(handle.consolidate(request)).map_err(to_napi_error)?;
        encode(&run)
    }

    /// Returns an export-import handle, or throws if not wired.
    #[napi(js_name = "requireExportImportApi")]
    pub fn require_export_import_api(&self) -> Result<NativeExportImportApi> {
        let handle = self
            .inner
            .require_export_import()
            .map_err(to_napi_error)?
            .clone();
        Ok(NativeExportImportApi { handle })
    }

    /// Returns an observability / diagnostics handle, or throws if not wired.
    #[napi(js_name = "requireObservabilityApi")]
    pub fn require_observability_api(&self) -> Result<NativeObservabilityApi> {
        let handle = self
            .inner
            .require_observability()
            .map_err(to_napi_error)?
            .clone();
        Ok(NativeObservabilityApi { handle })
    }

    /// Returns a knowledge-query handle (engine-neutral entity/relationship
    /// listing), or throws if not wired. Use this for scope-wide listing that
    /// backs graph traversal; the trait-level reads/writes live on
    /// [`NativeGraphApi`].
    #[napi(js_name = "requireKnowledgeQueryApi")]
    pub fn require_knowledge_query_api(&self) -> Result<NativeKnowledgeQueryApi> {
        let handle = self
            .inner
            .require_knowledge_query()
            .map_err(to_napi_error)?
            .clone();
        Ok(NativeKnowledgeQueryApi { handle })
    }

    /// Returns a belief handle, or throws if not wired.
    #[napi(js_name = "requireBeliefsApi")]
    pub fn require_beliefs_api(&self) -> Result<NativeBeliefsApi> {
        let handle = self.inner.require_beliefs().map_err(to_napi_error)?.clone();
        Ok(NativeBeliefsApi { handle })
    }

    /// Returns a hierarchy handle, or throws if not wired.
    #[napi(js_name = "requireHierarchyApi")]
    pub fn require_hierarchy_api(&self) -> Result<NativeHierarchyApi> {
        let handle = self
            .inner
            .require_hierarchy()
            .map_err(to_napi_error)?
            .clone();
        Ok(NativeHierarchyApi { handle })
    }

    /// Returns a lexical-feed handle (BM25 lane upserts), or throws if not wired.
    #[napi(js_name = "requireLexicalFeedApi")]
    pub fn require_lexical_feed_api(&self) -> Result<NativeLexicalFeedApi> {
        let handle = self
            .inner
            .require_lexical_feed()
            .map_err(to_napi_error)?
            .clone();
        Ok(NativeLexicalFeedApi { handle })
    }

    /// Returns a procedures handle (replayable runbooks), or throws if not wired.
    #[napi(js_name = "requireProceduresApi")]
    pub fn require_procedures_api(&self) -> Result<NativeProceduresApi> {
        let handle = self
            .inner
            .require_procedures()
            .map_err(to_napi_error)?
            .clone();
        Ok(NativeProceduresApi { handle })
    }

    /// Returns an ontology handle, or throws if not wired.
    #[napi(js_name = "requireOntologyApi")]
    pub fn require_ontology_api(&self) -> Result<NativeOntologyApi> {
        let handle = self
            .inner
            .require_ontology()
            .map_err(to_napi_error)?
            .clone();
        Ok(NativeOntologyApi { handle })
    }

    /// Returns a taxonomy handle, or throws if not wired.
    #[napi(js_name = "requireTaxonomyApi")]
    pub fn require_taxonomy_api(&self) -> Result<NativeTaxonomyApi> {
        let handle = self
            .inner
            .require_taxonomy()
            .map_err(to_napi_error)?
            .clone();
        Ok(NativeTaxonomyApi { handle })
    }

    /// Returns a retrieval handle, or throws if not wired.
    #[napi(js_name = "requireRetrievalApi")]
    pub fn require_retrieval_api(&self) -> Result<NativeRetrievalApi> {
        let handle = self
            .inner
            .require_retrieval()
            .map_err(to_napi_error)?
            .clone();
        Ok(NativeRetrievalApi { handle })
    }

    /// Returns a vectors handle, or throws if not wired.
    #[napi(js_name = "requireVectorsApi")]
    pub fn require_vectors_api(&self) -> Result<NativeVectorsApi> {
        let handle = self.inner.require_vectors().map_err(to_napi_error)?.clone();
        Ok(NativeVectorsApi { handle })
    }

    /// Returns a migration handle, or throws if not wired.
    #[napi(js_name = "requireMigrationApi")]
    pub fn require_migration_api(&self) -> Result<NativeMigrationApi> {
        let handle = self
            .inner
            .require_migration()
            .map_err(to_napi_error)?
            .clone();
        Ok(NativeMigrationApi { handle })
    }

    /// Returns an embedding-provider handle, or throws if not wired.
    #[napi(js_name = "requireEmbeddingProviderApi")]
    pub fn require_embedding_provider_api(&self) -> Result<NativeEmbeddingProviderApi> {
        let handle = self
            .inner
            .require_embedding_provider()
            .map_err(to_napi_error)?
            .clone();
        Ok(NativeEmbeddingProviderApi { handle })
    }
}

// ---------------------------------------------------------------------------
// NativeMemoryApi — retrieve / write / forget
// ---------------------------------------------------------------------------

/// Memory API handle proxy. Holds an `Arc<dyn MemoryService>` and exposes the
/// core memory operations as JSON-in / JSON-out methods.
#[napi]
pub struct NativeMemoryApi {
    handle: Arc<dyn MemoryService>,
}

#[napi]
impl NativeMemoryApi {
    /// Retrieves policy-checked context. Takes a `RetrievalRequest` JSON,
    /// returns a `ContextPayload` JSON.
    #[napi(js_name = "searchJson")]
    pub fn search_json(&self, request_json: String) -> Result<String> {
        let request: RetrievalRequest = decode(&request_json)?;
        let result: ContextPayload =
            block_on(self.handle.retrieve(request)).map_err(to_napi_error)?;
        encode(&result)
    }

    /// Writes a memory and records the lifecycle event. Takes a
    /// `WriteMemoryRequest` JSON, returns a `WriteMemoryResponse` JSON.
    #[napi(js_name = "writeJson")]
    pub fn write_json(&self, request_json: String) -> Result<String> {
        let request: WriteMemoryRequest = decode(&request_json)?;
        let result: WriteMemoryResponse =
            block_on(self.handle.write_memory(request)).map_err(to_napi_error)?;
        encode(&result)
    }

    /// Applies delete/redact/tombstone/archive behavior. Takes a `ForgetRequest`
    /// JSON, returns a `ForgetResult` JSON.
    #[napi(js_name = "forgetJson")]
    pub fn forget_json(&self, request_json: String) -> Result<String> {
        let request: ForgetRequest = decode(&request_json)?;
        let result: ForgetResult = block_on(self.handle.forget(request)).map_err(to_napi_error)?;
        encode(&result)
    }
}

// ---------------------------------------------------------------------------
// NativeGraphApi — entity/relationship reads + graph neighbors
// ---------------------------------------------------------------------------

/// Graph API handle proxy. Holds both the `KnowledgeRepository` (entity /
/// relationship reads and writes) and `KnowledgeGraphRepository` (neighbors)
/// handles, exposing the key graph operations as JSON-in / JSON-out methods.
///
/// Scope-wide listing (`list_entities` / `list_relationships`) lives on the
/// `KnowledgeQuery` port — use [`NativeKnowledgeQueryApi`] for that. This proxy
/// offers the trait-level reads/writes plus traversal.
#[napi]
pub struct NativeGraphApi {
    knowledge: Arc<dyn KnowledgeRepository>,
    graph: Arc<dyn KnowledgeGraphRepository>,
}

#[napi]
impl NativeGraphApi {
    /// Looks up an entity by ID inside a scope. Takes `{ id, scope }` JSON,
    /// returns the `KnowledgeEntity` JSON or `null`.
    #[napi(js_name = "getEntityJson")]
    pub fn get_entity_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let id: Id = id_field(&value, "id")?;
        let scope = scope_field(&value)?;
        let result = block_on(self.knowledge.get_entity(&id, &scope)).map_err(to_napi_error)?;
        encode(&result)
    }

    /// Stores or updates an entity. Takes a `KnowledgeEntity` JSON, returns the
    /// persisted `KnowledgeEntity` JSON.
    #[napi(js_name = "putEntityJson")]
    pub fn put_entity_json(&self, entity_json: String) -> Result<String> {
        let entity: KnowledgeEntity = decode(&entity_json)?;
        let result = block_on(self.knowledge.put_entity(entity)).map_err(to_napi_error)?;
        encode(&result)
    }

    /// Returns graph neighbors for a node without crossing scope boundaries.
    /// Takes `{ graphId, nodeId, scope, limit? }` JSON, returns a
    /// `[KnowledgeRelationship, …]` JSON array.
    #[napi(js_name = "neighborsJson")]
    pub fn neighbors_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let graph_id: Id = id_field(&value, "graphId")?;
        let node_id: Id = id_field(&value, "nodeId")?;
        let scope = scope_field(&value)?;
        let limit = value
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|n| n as u32);
        let result = block_on(self.graph.neighbors(&graph_id, &node_id, &scope, limit))
            .map_err(to_napi_error)?;
        encode(&result)
    }
}

// ---------------------------------------------------------------------------
// NativeProvenanceApi — provenance reads + evidence attach
// ---------------------------------------------------------------------------

/// Provenance / evidence handle proxy. Holds an `Arc<dyn ProvenanceQuery>` and
/// exposes provenance reads and the additive evidence-attach write as
/// JSON-in / JSON-out methods.
#[napi]
pub struct NativeProvenanceApi {
    handle: Arc<dyn ProvenanceQuery>,
}

#[napi]
impl NativeProvenanceApi {
    /// Provenance carried by a record. Takes `{ target, id, scope }` JSON
    /// (`target` is an `EvidenceTargetType` like `"entity"` / `"relationship"` /
    /// `"source"`), returns the `Provenance` JSON or `null`.
    #[napi(js_name = "provenanceForJson")]
    pub fn provenance_for_json(&self, request_json: String) -> Result<String> {
        let (target, id, scope) = decode_target_query(&request_json)?;
        let result: Option<Provenance> =
            block_on(self.handle.provenance_for(target, &id, &scope)).map_err(to_napi_error)?;
        encode(&result)
    }

    /// Evidence links carried by a record. Takes `{ target, id, scope }` JSON,
    /// returns an `[EvidenceRef, …]` JSON array (empty if none).
    #[napi(js_name = "evidenceForJson")]
    pub fn evidence_for_json(&self, request_json: String) -> Result<String> {
        let (target, id, scope) = decode_target_query(&request_json)?;
        let result: Vec<EvidenceRef> =
            block_on(self.handle.evidence_for(target, &id, &scope)).map_err(to_napi_error)?;
        encode(&result)
    }

    /// Appends an evidence link to a record's provenance and returns the
    /// updated `Provenance`. Takes
    /// `{ target, targetId, evidence, scope }` JSON, returns the updated
    /// `Provenance` JSON.
    #[napi(js_name = "attachEvidenceJson")]
    pub fn attach_evidence_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let target = decode_target(&value)?;
        let target_id = value
            .get("targetId")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::from_reason("missing string field 'targetId'"))?
            .to_owned();
        let evidence_value = value
            .get("evidence")
            .ok_or_else(|| Error::from_reason("missing 'evidence' field"))?;
        let evidence: EvidenceRef = serde_json::from_value(evidence_value.clone())
            .map_err(|e| Error::from_reason(format!("invalid evidence: {e}")))?;
        let scope = scope_field(&value)?;
        let result: Provenance = block_on(
            self.handle
                .attach_evidence(target, &target_id, evidence, &scope),
        )
        .map_err(to_napi_error)?;
        encode(&result)
    }
}

/// Decodes the common `{ target, id, scope }` shape shared by provenance/evidence reads.
fn decode_target_query(request_json: &str) -> Result<(EvidenceTargetType, String, Scope)> {
    let value = decode::<serde_json::Value>(request_json)?;
    let target = decode_target(&value)?;
    let id = value
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::from_reason("missing string field 'id'"))?
        .to_owned();
    let scope = scope_field(&value)?;
    Ok((target, id, scope))
}

/// Decodes the `target` field (`EvidenceTargetType`, e.g. `"entity"`).
fn decode_target(value: &serde_json::Value) -> Result<EvidenceTargetType> {
    let target_value = value
        .get("target")
        .ok_or_else(|| Error::from_reason("missing 'target' field"))?;
    serde_json::from_value::<EvidenceTargetType>(target_value.clone())
        .map_err(|e| Error::from_reason(format!("invalid target: {e}")))
}

// ---------------------------------------------------------------------------
// NativeBatchApi — best-effort batch ingest
// ---------------------------------------------------------------------------

/// Batch-ingest handle proxy. Holds an `Arc<dyn BatchIngest>` and exposes the
/// best-effort batch ingest plus its transactional guarantee.
///
/// `BatchOutcome` carries a typed `CoreError` per failed step, which is not
/// `Serialize`; the outcome is therefore projected into a serializable shape
/// (`BatchOutcomeJson`) that stringifies each step's error.
#[napi]
pub struct NativeBatchApi {
    handle: Arc<dyn BatchIngest>,
}

#[napi]
impl NativeBatchApi {
    /// Ingests a semantic batch best-effort. Takes a `BatchIngestRequest` JSON,
    /// returns a per-step outcome JSON (`{ guarantee, status, steps: […] }`).
    #[napi(js_name = "ingestJson")]
    pub fn ingest_json(&self, request_json: String) -> Result<String> {
        let request: BatchIngestRequest = decode(&request_json)?;
        let outcome: BatchOutcome = block_on(self.handle.ingest(request)).map_err(to_napi_error)?;
        // Project into a serializable shape (CoreError → string).
        let projected = BatchOutcomeJson::from(&outcome);
        encode(&projected)
    }

    /// Returns the transactional guarantee this backend provides, as a JSON
    /// string (`"BestEffort"` for the SQLite backend).
    #[napi(js_name = "transactionGuarantee")]
    pub fn transaction_guarantee(&self) -> Result<String> {
        let guarantee: TransactionGuarantee = self.handle.transaction_guarantee();
        encode(&guarantee)
    }
}

/// Serializable projection of [`BatchOutcome`] for the N-API boundary.
/// `CoreError` (not `Serialize`) is stringified per step.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BatchOutcomeJson<'a> {
    guarantee: &'a TransactionGuarantee,
    status: &'a BatchStatus,
    steps: Vec<StepOutcomeJson<'a>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StepOutcomeJson<'a> {
    step: &'a BatchStep,
    status: &'a StepStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl<'a> BatchOutcomeJson<'a> {
    fn from(outcome: &'a BatchOutcome) -> Self {
        let steps = outcome
            .steps
            .iter()
            .map(|s| StepOutcomeJson {
                step: &s.step,
                status: &s.status,
                error: s.error.as_ref().map(|e| e.to_string()),
            })
            .collect();
        Self {
            guarantee: &outcome.guarantee,
            status: &outcome.status,
            steps,
        }
    }
}

// ---------------------------------------------------------------------------
// NativeRecallApi — unified recall
// ---------------------------------------------------------------------------

/// Unified-recall handle proxy. Holds an `Arc<dyn UnifiedRecall>` and exposes
/// the one-query-fused-across-lanes recall as JSON-in / JSON-out.
#[napi]
pub struct NativeRecallApi {
    handle: Arc<dyn UnifiedRecall>,
}

#[napi]
impl NativeRecallApi {
    /// Fans a query across the v1 lanes (facts, graph, vector, lexical,
    /// beliefs) and fuses them into one `ContextPayload`. Takes a
    /// `RetrievalRequest` JSON, returns a `ContextPayload` JSON.
    #[napi(js_name = "recallJson")]
    pub fn recall_json(&self, request_json: String) -> Result<String> {
        let request: RetrievalRequest = decode(&request_json)?;
        let result: ContextPayload =
            block_on(self.handle.recall(request)).map_err(to_napi_error)?;
        encode(&result)
    }
}

// ---------------------------------------------------------------------------
// NativeExportImportApi — export
// ---------------------------------------------------------------------------

/// Export-import handle proxy. Holds an `Arc<dyn ExportImport>` and exposes the
/// export half (import stays on `MigrationService`).
#[napi]
pub struct NativeExportImportApi {
    handle: Arc<dyn ExportImport>,
}

#[napi]
impl NativeExportImportApi {
    /// Exports the semantic state visible to a scope into one `ImportData`
    /// payload. Takes a `Scope` JSON, returns an `ImportData` JSON ready for
    /// `MigrationService::dry_run_import`.
    #[napi(js_name = "exportJson")]
    pub fn export_json(&self, scope_json: String) -> Result<String> {
        let scope: Scope = decode(&scope_json)?;
        let result = block_on(self.handle.export(&scope)).map_err(to_napi_error)?;
        encode(&result)
    }
}

// ---------------------------------------------------------------------------
// NativeObservabilityApi — diagnostics
// ---------------------------------------------------------------------------

/// Observability / diagnostics handle proxy. Holds an `Arc<dyn Observability>`
/// and exposes the one-call diagnostic read as JSON-in / JSON-out.
#[napi]
pub struct NativeObservabilityApi {
    handle: Arc<dyn Observability>,
}

#[napi]
impl NativeObservabilityApi {
    /// Returns a point-in-time diagnostic snapshot of this provider as a
    /// `DiagnosticsSnapshot` JSON (capability report, record counts, embedding
    /// config, schema/adapter versions).
    #[napi(js_name = "diagnosticsJson")]
    pub fn diagnostics_json(&self) -> Result<String> {
        let result = block_on(self.handle.diagnostics()).map_err(to_napi_error)?;
        encode(&result)
    }
}

// ---------------------------------------------------------------------------
// NativeKnowledgeQueryApi — engine-neutral entity/relationship listing
// ---------------------------------------------------------------------------

/// Knowledge-query handle proxy. Holds an `Arc<dyn KnowledgeQuery>` and exposes
/// scope-wide entity + relationship listing as JSON-in / JSON-out methods. This
/// is the engine-neutral listing port that backs graph traversal (the MCP
/// `graph_*` / `codegraph_*` tools).
#[napi]
pub struct NativeKnowledgeQueryApi {
    handle: Arc<dyn KnowledgeQuery>,
}

#[napi]
impl NativeKnowledgeQueryApi {
    /// Lists entities in a scope. Takes a `Scope` JSON, returns
    /// `[KnowledgeEntity, …]` JSON.
    #[napi(js_name = "listEntitiesJson")]
    pub fn list_entities_json(&self, scope_json: String) -> Result<String> {
        let scope: Scope = decode(&scope_json)?;
        let result = block_on(self.handle.list_entities(&scope)).map_err(to_napi_error)?;
        encode(&result)
    }

    /// Lists relationships in a scope. Takes a `Scope` JSON, returns
    /// `[KnowledgeRelationship, …]` JSON.
    #[napi(js_name = "listRelationshipsJson")]
    pub fn list_relationships_json(&self, scope_json: String) -> Result<String> {
        let scope: Scope = decode(&scope_json)?;
        let result = block_on(self.handle.list_relationships(&scope)).map_err(to_napi_error)?;
        encode(&result)
    }
}

// ---------------------------------------------------------------------------
// NativeBeliefsApi — belief lifecycle
// ---------------------------------------------------------------------------

/// Belief handle proxy. Holds an `Arc<dyn BeliefRepository>` and exposes the
/// belief lifecycle as JSON-in / JSON-out methods (mirrors the MCP `belief_*`
/// tools).
#[napi]
pub struct NativeBeliefsApi {
    handle: Arc<dyn BeliefRepository>,
}

#[napi]
impl NativeBeliefsApi {
    /// Returns the live belief for a subject at `asOf` (default now). Takes
    /// `{ subject, scope, asOf? }` JSON (asOf is RFC3339), returns a `Belief`
    /// JSON or `null`.
    #[napi(js_name = "getBeliefJson")]
    pub fn get_belief_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let subject = value
            .get("subject")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::from_reason("subject is required"))?;
        let scope = scope_field(&value)?;
        let as_of = match value.get("asOf").and_then(|v| v.as_str()) {
            Some(s) => chrono::DateTime::parse_from_rfc3339(s)
                .map_err(|e| Error::from_reason(format!("invalid asOf (RFC3339): {e}")))?
                .with_timezone(&chrono::Utc),
            None => chrono::Utc::now(),
        };
        let query = BeliefQuery::live_subject(scope, subject, as_of);
        let result = block_on(self.handle.get_belief(query)).map_err(to_napi_error)?;
        encode(&result)
    }

    /// Upserts a belief by its compatibility key. Takes a `Belief` JSON, returns
    /// the persisted `Belief` JSON.
    #[napi(js_name = "upsertBeliefJson")]
    pub fn upsert_belief_json(&self, belief_json: String) -> Result<String> {
        let belief: Belief = decode(&belief_json)?;
        let result = block_on(self.handle.upsert_belief(belief)).map_err(to_napi_error)?;
        encode(&result)
    }

    /// Retracts a belief by id (closes its valid interval). Takes
    /// `{ id, scope, at? }` JSON (at is RFC3339, defaults to now), returns the
    /// retracted `Belief` JSON.
    #[napi(js_name = "retractBeliefJson")]
    pub fn retract_belief_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let id: Id = id_field(&value, "id")?;
        let scope = scope_field(&value)?;
        let at = match value.get("at").and_then(|v| v.as_str()) {
            Some(s) => chrono::DateTime::parse_from_rfc3339(s)
                .map_err(|e| Error::from_reason(format!("invalid at (RFC3339): {e}")))?
                .with_timezone(&chrono::Utc),
            None => chrono::Utc::now(),
        };
        let result =
            block_on(self.handle.retract_belief(&id, &scope, at)).map_err(to_napi_error)?;
        encode(&result)
    }

    /// Lists stale beliefs in a scope. Takes a `Scope` JSON, returns
    /// `[Belief, …]` JSON.
    #[napi(js_name = "listStaleBeliefsJson")]
    pub fn list_stale_beliefs_json(&self, scope_json: String) -> Result<String> {
        let scope: Scope = decode(&scope_json)?;
        let result = block_on(self.handle.list_stale(&scope)).map_err(to_napi_error)?;
        encode(&result)
    }
}

// ---------------------------------------------------------------------------
// NativeHierarchyApi — hierarchy navigation
// ---------------------------------------------------------------------------

/// Hierarchy handle proxy. Holds an `Arc<dyn HierarchyRepository>` and exposes
/// hierarchy navigation as JSON-in / JSON-out methods (mirrors the MCP
/// `hierarchy_path` tool).
#[napi]
pub struct NativeHierarchyApi {
    handle: Arc<dyn HierarchyRepository>,
}

#[napi]
impl NativeHierarchyApi {
    /// Navigation path for seed entity ids. Takes `{ seeds: [String], scope,
    /// maxLayer? }` JSON, returns a `HierarchyPath` JSON (nodes/relations/lca).
    #[napi(js_name = "pathForJson")]
    pub fn path_for_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let seeds: Vec<String> = value
            .get("seeds")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        let scope = scope_field(&value)?;
        let max_layer = value
            .get("maxLayer")
            .and_then(|v| v.as_u64())
            .map(|n| n as u32);
        let result =
            block_on(self.handle.path_for(&seeds, &scope, max_layer)).map_err(to_napi_error)?;
        encode(&result)
    }
}

// ---------------------------------------------------------------------------
// NativeLexicalFeedApi — BM25 lane feed
// ---------------------------------------------------------------------------

/// Lexical-feed handle proxy. Holds an `Arc<dyn LexicalFeed>` and exposes the
/// BM25-lane upsert operations as JSON-in / JSON-out methods.
#[napi]
pub struct NativeLexicalFeedApi {
    handle: Arc<dyn LexicalFeed>,
}

#[napi]
impl NativeLexicalFeedApi {
    /// Upserts one searchable document. Takes `{ targetId, text }` JSON.
    #[napi(js_name = "upsertJson")]
    pub fn upsert_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let target_id = value
            .get("targetId")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::from_reason("targetId is required"))?;
        let text = value
            .get("text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::from_reason("text is required"))?;
        block_on(self.handle.upsert(target_id, text)).map_err(to_napi_error)?;
        encode(&serde_json::json!({"ok": true}))
    }

    /// Upserts many `(targetId, text)` entries. Takes `[{ targetId, text }, …]` JSON.
    #[napi(js_name = "upsertBatchJson")]
    pub fn upsert_batch_json(&self, entries_json: String) -> Result<String> {
        let entries: Vec<serde_json::Value> = decode(&entries_json)?;
        let pairs: Vec<(String, String)> = entries
            .iter()
            .filter_map(|e| {
                let id = e.get("targetId")?.as_str()?.to_owned();
                let text = e.get("text")?.as_str()?.to_owned();
                Some((id, text))
            })
            .collect();
        let count = pairs.len();
        block_on(self.handle.upsert_batch(&pairs)).map_err(to_napi_error)?;
        encode(&serde_json::json!({"ok": true, "count": count}))
    }
}

// ---------------------------------------------------------------------------
// NativeProceduresApi — replayable runbooks (Layer 6)
// ---------------------------------------------------------------------------

/// Procedures handle proxy. Holds an `Arc<dyn ProcedureRepository>` and exposes
/// the procedure lifecycle (upsert / list / increment success or failure) as
/// JSON-in / JSON-out methods.
#[napi]
pub struct NativeProceduresApi {
    handle: Arc<dyn ProcedureRepository>,
}

#[napi]
impl NativeProceduresApi {
    /// Upserts a procedure by id. Takes a `Procedure` JSON, returns the persisted
    /// `Procedure` JSON.
    #[napi(js_name = "upsertJson")]
    pub fn upsert_json(&self, procedure_json: String) -> Result<String> {
        let procedure: Procedure = decode(&procedure_json)?;
        let result = block_on(self.handle.upsert_procedure(procedure)).map_err(to_napi_error)?;
        encode(&result)
    }

    /// Lists procedures in a scope. Takes a `Scope` JSON, returns `[Procedure, …]`.
    #[napi(js_name = "listJson")]
    pub fn list_json(&self, scope_json: String) -> Result<String> {
        let scope: Scope = decode(&scope_json)?;
        let result = block_on(self.handle.list_procedures(&scope)).map_err(to_napi_error)?;
        encode(&result)
    }

    /// Bumps the success counter. Takes `{ id, scope }` JSON, returns the updated
    /// `Procedure` JSON.
    #[napi(js_name = "incrementSuccessJson")]
    pub fn increment_success_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let id: Id = id_field(&value, "id")?;
        let scope = scope_field(&value)?;
        let result = block_on(self.handle.increment_success(&id, &scope)).map_err(to_napi_error)?;
        encode(&result)
    }

    /// Bumps the failure counter. Takes `{ id, scope }` JSON, returns the updated
    /// `Procedure` JSON.
    #[napi(js_name = "incrementFailureJson")]
    pub fn increment_failure_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let id: Id = id_field(&value, "id")?;
        let scope = scope_field(&value)?;
        let result = block_on(self.handle.increment_failure(&id, &scope)).map_err(to_napi_error)?;
        encode(&result)
    }
}

// ---------------------------------------------------------------------------
// Minimal handle proxies for remaining capabilities (ADR-0022 parity).
// Each holds the trait handle + a `ping`. Real methods added as needed.
// ---------------------------------------------------------------------------

#[napi]
pub struct NativeOntologyApi {
    handle: Arc<dyn OntologyRepository>,
}

#[napi]
impl NativeOntologyApi {
    #[napi(js_name = "ping")]
    pub fn ping(&self) -> Result<String> {
        Ok("ontology".to_owned())
    }
}

#[napi]
pub struct NativeTaxonomyApi {
    handle: Arc<dyn TaxonomyRepository>,
}

#[napi]
impl NativeTaxonomyApi {
    #[napi(js_name = "ping")]
    pub fn ping(&self) -> Result<String> {
        Ok("taxonomy".to_owned())
    }
}

#[napi]
pub struct NativeRetrievalApi {
    handle: Arc<dyn RetrievalIndex>,
}

#[napi]
impl NativeRetrievalApi {
    #[napi(js_name = "ping")]
    pub fn ping(&self) -> Result<String> {
        Ok("retrieval".to_owned())
    }
}

#[napi]
pub struct NativeVectorsApi {
    handle: Arc<dyn VectorIndex>,
}

#[napi]
impl NativeVectorsApi {
    #[napi(js_name = "ping")]
    pub fn ping(&self) -> Result<String> {
        Ok("vectors".to_owned())
    }
}

#[napi]
pub struct NativeMigrationApi {
    handle: Arc<dyn MigrationService>,
}

#[napi]
impl NativeMigrationApi {
    #[napi(js_name = "ping")]
    pub fn ping(&self) -> Result<String> {
        Ok("migration".to_owned())
    }
}

#[napi]
pub struct NativeEmbeddingProviderApi {
    handle: Arc<dyn EmbeddingProvider>,
}

#[napi]
impl NativeEmbeddingProviderApi {
    #[napi(js_name = "ping")]
    pub fn ping(&self) -> Result<String> {
        Ok("embedding_provider".to_owned())
    }
}
