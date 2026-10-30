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
    Actor, ActorKind, AllowedUse, ApplyMode, Belief, ConsolidationRequest, ContextPayload,
    Contradiction, DeleteMode, EvidenceRef, EvidenceTargetType, ForgetRequest, ForgetResult, Id,
    KnowledgeEntity, KnowledgeRelationship, MaintenancePlan, MaintenancePlanRequest,
    MaintenancePolicy, MemoryRecord, Page, Policy, Procedure, Provenance, Retention,
    RetrievalRequest, Scope, Sensitivity, Visibility, WriteMemoryRequest, WriteMemoryResponse,
};
use engram_hierarchy::HierarchyRepository;
use engram_ingest::{
    KnowledgeRepoGraph, ScanFilter, ScanFilterConfig, ScanOptions, ScanSummary, scan_repository,
};
use engram_integration::{
    BatchIngest, BatchIngestRequest, BatchOutcome, BatchStatus, BatchStep, CommunityQuery,
    EmbeddingProvider, EngramConfig, EngramProvider, ExportImport, KnowledgeQuery, LexicalFeed,
    MigrationService, Observability, ProvenanceQuery, StepStatus, SupersessionCounts,
    TransactionGuarantee, UnifiedRecall,
};
use engram_knowledge::{
    GraphMaintenanceRepository, KnowledgeGraphRepository, KnowledgeRepository, OntologyRepository,
    TaxonomyRepository,
};
use engram_knowledge::{KnowledgeChangeLog, ScanCheckpointStore};
use engram_memory::MemoryService;
use engram_procedures::ProcedureRepository;
use engram_proposals::ProposalService;
use engram_retrieval::OutcomeLedger;
use engram_retrieval::{RetrievalIndex, VectorIndex};
use engram_runtime::{CoreError, CoreResult};
use futures::executor::block_on;
use napi::bindgen_prelude::*;
use napi_derive::napi;
use serde::{Deserialize, Serialize};
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodeResetPreviewRequest {
    scope: Scope,
    repo: engram_domain::RepoId,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodeResetApplyRequest {
    journal: engram_domain::CodeResetJournal,
    decision: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodeResetResumeRequest {
    journal: engram_domain::CodeResetJournal,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodeFetchRequest {
    scope: Scope,
    path: String,
    symbol: Option<String>,
}

fn exact_read_candidates(
    chunks: &[engram_domain::KnowledgeChunk],
    path: &str,
) -> Vec<engram_integration::code_binding::ExactReadCandidate> {
    chunks
        .iter()
        .filter(|chunk| {
            chunk
                .location
                .as_ref()
                .and_then(|location| location.path.as_deref())
                .is_some_and(|chunk_path| {
                    chunk_path.trim_start_matches("./").trim_end_matches('/')
                        == path.trim_start_matches("./").trim_end_matches('/')
                })
        })
        .map(|chunk| {
            let location = chunk.location.as_ref();
            let mut names = chunk
                .entities
                .iter()
                .filter_map(|entity| entity.name.clone())
                .collect::<Vec<_>>();
            if let Some(anchor) = location.and_then(|location| location.anchor.as_deref()) {
                names.push(anchor.to_owned());
            }
            engram_integration::code_binding::ExactReadCandidate {
                record_id: chunk.id.to_string(),
                start_line: location.and_then(|location| location.start_line),
                end_line: location.and_then(|location| location.end_line),
                names,
                text_head: chunk.text.lines().next().unwrap_or("").to_owned(),
            }
        })
        .collect()
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
        // The config JSON may carry a `codeAuthority` object (review r6
        // finding 1): the HOST's explicit code-operation policy. `launchScopes`
        // lists the scopes whose code operations this process may perform;
        // absent `codeAuthority`, the recipe default (deny-by-default
        // `RecipeDefaultAuthorizer`) stays in force — every code operation
        // denies with the typed `CodeAuthorizationRequired` envelope.
        let value: serde_json::Value = decode(&config_json)?;
        let launch_scopes: Option<Vec<Scope>> = value
            .get("codeAuthority")
            .and_then(|authority| authority.get("launchScopes"))
            .cloned()
            .map(|scopes| serde_json::from_value(scopes))
            .transpose()
            .map_err(|error| {
                to_napi_error(engram_runtime::CoreError::InvalidRequest {
                    reason: format!(
                        "InvalidRequest: codeAuthority.launchScopes is malformed: {error}"
                    ),
                })
            })?;
        let config: EngramConfig =
            serde_json::from_value(value).map_err(|error| Error::from_reason(error.to_string()))?;
        // pgvector configs route to the backends/pgvector recipe (the SDK facade's
        // open() is engine-neutral and sqlite-default). When this binding is built
        // without the `pgvector` feature, surface a clear error instead of a silent
        // sqlite open.
        let provider = if config.pgvector_connection_string.is_some() {
            #[cfg(feature = "pgvector")]
            {
                engram_backend_pgvector::open(&config).map_err(to_napi_error)?
            }
            #[cfg(not(feature = "pgvector"))]
            {
                return Err(to_napi_error(engram_runtime::CoreError::InvalidRequest {
                    reason: "pgvector config requires the `pgvector` feature on @engram/node; \
                             rebuild with --features pgvector"
                        .to_owned(),
                }));
            }
        } else {
            EngramProvider::open(&config).map_err(to_napi_error)?
        };
        // Host policy injection (review r6 finding 1): an explicit
        // `codeAuthority.launchScopes` config replaces the deny-by-default
        // recipe authorizer with the launch-scope policy — the native twin of
        // the MCP server's launch-scope injection.
        let provider = match launch_scopes {
            Some(scopes) => provider
                .with_code_authorizer(std::sync::Arc::new(
                    engram_integration::code_binding::ScopeSetAuthorizer::new(scopes),
                ))
                .map_err(to_napi_error)?,
            None => provider,
        };
        Ok(Self { inner: provider })
    }

    /// Opens a provider from a profile file path (e.g. `semantic-engine.toml`).
    ///
    /// Profile files reject the Postgres backend, so this always opens sqlite;
    /// a pgvector provider is reachable only through [`Self::new`] with a config
    /// carrying `pgvector_connection_string`.
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

    /// Returns a graph-maintenance handle (reversible plan/apply graph repair),
    /// or throws if not wired (ADR-0027).
    #[napi(js_name = "requireGraphMaintenanceApi")]
    pub fn require_graph_maintenance_api(&self) -> Result<NativeGraphMaintenanceApi> {
        let handle = self
            .inner
            .require_graph_maintenance()
            .map_err(to_napi_error)?
            .clone();
        Ok(NativeGraphMaintenanceApi { handle })
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

    /// File-level import graph (RFC-0020 Phase 2): takes `{scope}`, returns
    /// `[{fromPath, importPath, resolvedTo?}, …]` via the facade's single
    /// implementation.
    #[napi(js_name = "fileDependenciesJson")]
    pub fn file_dependencies_json(&self, request_json: String) -> Result<String> {
        let request: Scope = decode(&request_json)?;
        let deps =
            block_on(self.inner.code_graph_file_dependencies(&request)).map_err(to_napi_error)?;
        encode(&deps)
    }

    /// Natural-language explore (RFC-0020 Phase 2): takes
    /// `{scope, query, depth?, maxNodes?, maxEdges?}`, returns seeded
    /// bounded-subgraph nodes.
    #[napi(js_name = "exploreJson")]
    pub fn explore_json(&self, request_json: String) -> Result<String> {
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct ExploreRequest {
            scope: Scope,
            query: String,
            depth: Option<u32>,
            max_nodes: Option<usize>,
            max_edges: Option<usize>,
            /// Opt Variable/Module/File/Repository entities into the
            /// discovery seed list (suppressed by default).
            include_code_artifacts: Option<bool>,
        }
        let request: ExploreRequest = decode(&request_json)?;
        let nodes = block_on(self.inner.code_graph_explore_with_artifacts(
            &request.scope,
            &request.query,
            request.depth,
            request.max_nodes,
            request.max_edges,
            request.include_code_artifacts.unwrap_or(false),
        ))
        .map_err(to_napi_error)?;
        encode(&nodes)
    }

    /// Ingests a BOUND repository's code through the registered-locator
    /// acquisition + generation staging/publication pipeline (review r6
    /// finding 2): `{ scope, repo, branchRef?, scanFilter? }` → the
    /// serialized outcome (`{ mode: "bound", …IngestedGeneration }`). The
    /// caller NEVER supplies a path — the bytes come from the REGISTERED
    /// locator's bound branch at its pinned commit (a dirty checkout or
    /// another repository's directory cannot influence what is ingested).
    /// With NO `repo` the call is the explicitly DOCUMENT-ONLY legacy scan
    /// over `path` (`{ path, scope, scanFilter? }`) — code files are
    /// excluded by the document-only filter; code bytes never flow through
    /// the legacy lane. Supplying BOTH `path` and `repo` is a typed
    /// rejection (path is not an input to bound ingestion).
    #[napi(js_name = "scanRepositoryJson")]
    pub fn scan_repository_json(&self, request_json: String) -> Result<String> {
        let request: ScanRepoRequest = decode(&request_json)?;
        let outcome = scan_via_provider(&self.inner, &request).map_err(to_napi_error)?;
        encode(&outcome)
    }

    // ---- TASK-020: bound code operations (thin JSON forwards over the SAME
    // neutral coordinator the recipes and MCP use — no independent binding
    // derivation in the binding layer) ----

    /// Binds a repository's branch for a scope: `{ scope, repo, branchRef }`
    /// → serialized `CodeBinding`. Same-ref retries are idempotent; another
    /// branch is a `BindingConflict` denial.
    #[napi(js_name = "codeBindJson")]
    pub fn code_bind_json(&self, request_json: String) -> Result<String> {
        let request: CodeBindRequest = decode(&request_json)?;
        let coordinator = self.inner.code_bindings().map_err(to_napi_error)?.clone();
        let binding = futures::executor::block_on(coordinator.bind(
            &system_code_actor(),
            &request.scope,
            &request.repo,
            &request.branch_ref,
        ))
        .map_err(to_napi_error)?;
        encode(&binding)
    }

    /// Binding-keyed status for one repository: `{ scope, repo }` → the
    /// serialized status (binding, indexed OID, on-request ref freshness,
    /// binding-keyed checkpoint). Freshness is `unknown` until an explicit
    /// observation — never derived from a checkout HEAD.
    #[napi(js_name = "codeStatusJson")]
    pub fn code_status_json(&self, request_json: String) -> Result<String> {
        let request: CodeStatusRequest = decode(&request_json)?;
        let coordinator = self.inner.code_bindings().map_err(to_napi_error)?.clone();
        let status = futures::executor::block_on(coordinator.status(
            &system_code_actor(),
            &request.scope,
            &request.repo,
            None,
        ))
        .map_err(to_napi_error)?;
        encode(&status)
    }

    /// Registers a repository's acquisition locator: `{ scope, repo, path,
    /// mode? }` → the serialized registration. `mode` is `local-only` (the
    /// default) or `configured-remote`; an unknown mode is a typed
    /// `InvalidRequest`. The SAME shared coordinator seam the MCP and
    /// one-shot surfaces use (authorize before store).
    #[napi(js_name = "codeRegisterJson")]
    pub fn code_register_json(&self, request_json: String) -> Result<String> {
        let request: CodeRegisterRequest = decode(&request_json)?;
        let acquisition = match request.mode.as_deref() {
            None | Some("local-only") => engram_knowledge::AcquisitionMode::LocalOnly,
            Some("configured-remote") => engram_knowledge::AcquisitionMode::ConfiguredRemote,
            Some(other) => {
                return Err(to_napi_error(engram_runtime::CoreError::InvalidRequest {
                    reason: format!(
                        "unknown acquisition mode {other:?} (local-only | configured-remote)"
                    ),
                }));
            }
        };
        let coordinator = self.inner.code_bindings().map_err(to_napi_error)?.clone();
        let registration = futures::executor::block_on(coordinator.register(
            &system_code_actor(),
            &request.scope,
            engram_knowledge::RepositoryRegistration {
                repo_id: request.repo.clone(),
                locator_id: request.path,
                acquisition,
            },
        ))
        .map_err(to_napi_error)?;
        encode(&registration)
    }

    /// ONE on-request refresh of the bound branch ref: `{ scope, repo }` →
    /// the serialized observation. Delegates to the SAME shared
    /// registered-locator refresh as stdio/one-shot — the OID is resolved
    /// from the REGISTERED locator's local git, never supplied by the caller
    /// (the former `codeObserveRefJson` accepted arbitrary syntactically
    /// valid OIDs; review r6 finding 5 removed it).
    #[napi(js_name = "codeRefreshJson")]
    pub fn code_refresh_json(&self, request_json: String) -> Result<String> {
        let request: CodeStatusRequest = decode(&request_json)?;
        let coordinator = self.inner.code_bindings().map_err(to_napi_error)?.clone();
        let observation = futures::executor::block_on(coordinator.refresh_from_registration(
            &system_code_actor(),
            &request.scope,
            &request.repo,
        ))
        .map_err(to_napi_error)?;
        encode(&observation)
    }

    /// Reads digest-validated retained bytes for a published bound code path.
    #[napi(js_name = "codeFetchJson")]
    pub fn code_fetch_json(&self, request_json: String) -> Result<String> {
        let request: CodeFetchRequest = decode(&request_json)?;
        let query = self
            .inner
            .require_knowledge_query()
            .map_err(to_napi_error)?;
        let chunks = block_on(query.list_chunks(&request.scope)).map_err(to_napi_error)?;
        let candidates = exact_read_candidates(&chunks, &request.path);
        let outcome = block_on(self.inner.code_fetch_exact_participants(
            &system_code_actor(),
            &request.scope,
            &request.path,
            &candidates,
            request.symbol.as_deref(),
        ))
        .map_err(to_napi_error)?;
        match outcome {
            engram_integration::code_binding::ParticipantExactSource::Served {
                repo_id,
                generation_id,
                view,
                span,
                participants,
            } => encode(&serde_json::json!({
                "repoId": repo_id,
                "generationId": generation_id,
                "bytes": view.bytes,
                "fullContentDigest": view.full_content_digest,
                "servedDigest": view.served_digest,
                "commitOid": view.commit_oid,
                "path": view.path,
                "span": span,
                "participants": participants.iter().map(|participant| serde_json::json!({
                    "recordId": participant.record_id,
                    "startLine": participant.start_line,
                    "endLine": participant.end_line,
                    "byteSpan": participant.byte_span,
                })).collect::<Vec<_>>(),
            })),
            engram_integration::code_binding::ParticipantExactSource::Denied { reason, .. } => {
                Err(to_napi_error(CoreError::InvalidRequest { reason }))
            }
            engram_integration::code_binding::ParticipantExactSource::Unbound => {
                Err(to_napi_error(CoreError::InvalidRequest {
                    reason: "no bound source participants for requested path".to_owned(),
                }))
            }
        }
    }

    /// Creates an exact reset preview through the reset-authorized coordinator.
    #[napi(js_name = "codeResetPreviewJson")]
    pub fn code_reset_preview_json(&self, request_json: String) -> Result<String> {
        let request: CodeResetPreviewRequest = decode(&request_json)?;
        let coordinator = self.inner.code_bindings().map_err(to_napi_error)?.clone();
        let preview = futures::executor::block_on(coordinator.preview_reset(
            &system_code_actor(),
            &request.scope,
            &request.repo,
        ))
        .map_err(to_napi_error)?;
        encode(&preview)
    }

    /// Persists an owner-approved reset journal and applies its durable state machine.
    #[napi(js_name = "codeResetApplyJson")]
    pub fn code_reset_apply_json(&self, request_json: String) -> Result<String> {
        let request: CodeResetApplyRequest = decode(&request_json)?;
        let coordinator = self.inner.code_bindings().map_err(to_napi_error)?.clone();
        let journal = futures::executor::block_on(coordinator.journal_reset(
            &system_code_actor(),
            request.journal,
            request.decision,
        ))
        .map_err(to_napi_error)?;
        let applied =
            futures::executor::block_on(coordinator.apply_reset(&system_code_actor(), journal))
                .map_err(to_napi_error)?;
        encode(&applied)
    }

    /// Resumes pending or failed reset lanes from an immutable approved journal.
    #[napi(js_name = "codeResetResumeJson")]
    pub fn code_reset_resume_json(&self, request_json: String) -> Result<String> {
        let request: CodeResetResumeRequest = decode(&request_json)?;
        let coordinator = self.inner.code_bindings().map_err(to_napi_error)?.clone();
        let resumed = futures::executor::block_on(
            coordinator.apply_reset(&system_code_actor(), request.journal),
        )
        .map_err(to_napi_error)?;
        encode(&resumed)
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

    /// Returns a community-query handle (Louvain + meta-edges + member index),
    /// or throws if not wired.
    #[napi(js_name = "requireCommunityQueryApi")]
    pub fn require_community_query_api(&self) -> Result<NativeCommunityQueryApi> {
        let handle = self
            .inner
            .community_query()
            .ok_or_else(|| {
                to_napi_error(engram_runtime::CoreError::CapabilityUnsupported {
                    capability: "community_query".to_string(),
                    reason: "community aggregation not wired for this provider".to_string(),
                })
            })?
            .clone();
        Ok(NativeCommunityQueryApi { handle })
    }

    /// Returns a belief handle, or throws if not wired.
    #[napi(js_name = "requireBeliefsApi")]
    pub fn require_beliefs_api(&self) -> Result<NativeBeliefsApi> {
        let handle = self.inner.require_beliefs().map_err(to_napi_error)?.clone();
        Ok(NativeBeliefsApi { handle })
    }

    /// Returns a hierarchy handle, or throws if not wired. When the provider
    /// also wires knowledge-query, the handle is attached so `buildHierarchyJson`
    /// can read the call edges it clusters (best-effort: build throws a typed
    /// `CapabilityUnsupported` error when knowledge-query is absent).
    #[napi(js_name = "requireHierarchyApi")]
    pub fn require_hierarchy_api(&self) -> Result<NativeHierarchyApi> {
        let handle = self
            .inner
            .require_hierarchy()
            .map_err(to_napi_error)?
            .clone();
        let knowledge_query = self.inner.knowledge_query().cloned();
        Ok(NativeHierarchyApi {
            handle,
            knowledge_query,
        })
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

    /// Returns an operator proposal queue handle (inert pending writes),
    /// or throws if not wired.
    #[napi(js_name = "requireProposalsApi")]
    pub fn require_proposals_api(&self) -> Result<NativeProposalsApi> {
        let handle = self
            .inner
            .require_proposals()
            .map_err(to_napi_error)?
            .clone();
        Ok(NativeProposalsApi { handle })
    }

    /// Returns the attributed outcome ledger (F2), or throws if not wired.
    #[napi(js_name = "requireOutcomesApi")]
    pub fn require_outcomes_api(&self) -> Result<NativeOutcomesApi> {
        let handle = self
            .inner
            .require_outcomes()
            .map_err(to_napi_error)?
            .clone();
        Ok(NativeOutcomesApi { handle })
    }

    /// Returns the knowledge change-log handle (resumable invalidation
    /// pages), or throws if not wired.
    #[napi(js_name = "requireChangeLogApi")]
    pub fn require_change_log_api(&self) -> Result<NativeChangeLogApi> {
        let handle = self
            .inner
            .require_change_log()
            .map_err(to_napi_error)?
            .clone();
        Ok(NativeChangeLogApi { handle })
    }

    /// Returns the scan-checkpoint handle, or throws if not wired.
    #[napi(js_name = "requireCheckpointsApi")]
    pub fn require_checkpoints_api(&self) -> Result<NativeCheckpointsApi> {
        let handle = self
            .inner
            .require_checkpoints()
            .map_err(to_napi_error)?
            .clone();
        Ok(NativeCheckpointsApi { handle })
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

    /// Lists memories visible to `scope` as a keyset page. Takes
    /// `{ scope, after?: string, limit?: number }` JSON (`after` is the opaque
    /// `nextCursor` from a prior page); returns a
    /// `{ items: [MemoryRecord], nextCursor: string | null }` page JSON. The Rust
    /// service drives the keyset read (scope in the SQL); TypeScript holds no SQL.
    #[napi(js_name = "listMemoriesPagedJson")]
    pub fn list_memories_paged_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let scope = scope_field(&value)?;
        let after = value
            .get("after")
            .and_then(|v| v.as_str())
            .map(|s| engram_domain::Cursor::new(s.to_owned()));
        let limit = value.get("limit").and_then(|v| v.as_u64()).unwrap_or(100) as usize;
        let page: Page<MemoryRecord> = block_on(self.handle.list_memories_paged(
            &scope,
            after.as_ref(),
            limit,
        ))
        .map_err(to_napi_error)?;
        encode(&page)
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

    /// Stores or updates a relationship. Takes a `KnowledgeRelationship` JSON,
    /// returns the persisted `KnowledgeRelationship` JSON.
    #[napi(js_name = "putRelationshipJson")]
    pub fn put_relationship_json(&self, relationship_json: String) -> Result<String> {
        let relationship: KnowledgeRelationship = decode(&relationship_json)?;
        let result =
            block_on(self.knowledge.put_relationship(relationship)).map_err(to_napi_error)?;
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
// NativeGraphMaintenanceApi — reversible plan/apply graph repair (ADR-0027)
// ---------------------------------------------------------------------------

/// Graph-maintenance handle proxy. Holds an `Arc<dyn GraphMaintenanceRepository>`
/// and exposes reversible, plan/apply graph repair as JSON-in / JSON-out methods.
/// Mutating paths run through `build_plan` (dry-run, fills previews) then
/// `apply_plan` with `ApplyMode::Apply`; `Preview` stages without committing.
#[napi]
pub struct NativeGraphMaintenanceApi {
    handle: Arc<dyn GraphMaintenanceRepository>,
}

#[napi]
impl NativeGraphMaintenanceApi {
    /// Build a dry-run plan (fills before/after previews; never mutates). Takes a
    /// `MaintenancePlanRequest` JSON, returns the `MaintenancePlan` JSON.
    #[napi(js_name = "buildPlanJson")]
    pub fn build_plan_json(&self, request_json: String) -> Result<String> {
        let request: MaintenancePlanRequest = decode(&request_json)?;
        let result = block_on(self.handle.build_plan(request)).map_err(to_napi_error)?;
        encode(&result)
    }

    /// Apply (or preview) a reviewed plan. Takes a `{ plan, mode }` JSON (`mode`
    /// is `"preview"` or `"apply"`), returns the `MaintenanceApplyResult` JSON.
    #[napi(js_name = "applyPlanJson")]
    pub fn apply_plan_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let plan: MaintenancePlan = serde_json::from_value(
            value
                .get("plan")
                .cloned()
                .ok_or_else(|| Error::from_reason("missing 'plan' field"))?,
        )
        .map_err(|e| Error::from_reason(format!("invalid plan: {e}")))?;
        let mode: ApplyMode = serde_json::from_value(
            value
                .get("mode")
                .cloned()
                .ok_or_else(|| Error::from_reason("missing 'mode' field"))?,
        )
        .map_err(|e| Error::from_reason(format!("invalid mode: {e}")))?;
        let result = block_on(self.handle.apply_plan(&plan, mode)).map_err(to_napi_error)?;
        encode(&result)
    }

    /// Deterministic candidate detection (orphan / low-confidence / unsupported /
    /// duplicate; no LLM). Takes a `{ scope, graphId?, policy }` JSON, returns a
    /// `[MaintenanceCandidate, …]` JSON array.
    #[napi(js_name = "listMaintenanceCandidatesJson")]
    pub fn list_maintenance_candidates_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let scope = scope_field(&value)?;
        let graph_id = value.get("graphId").and_then(|v| v.as_str()).map(Id::from);
        let policy: MaintenancePolicy = serde_json::from_value(
            value
                .get("policy")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({})),
        )
        .map_err(|e| Error::from_reason(format!("invalid policy: {e}")))?;
        let result = block_on(
            self.handle
                .detect_candidates(&scope, graph_id.as_ref(), &policy),
        )
        .map_err(to_napi_error)?;
        encode(&result)
    }

    /// Point-in-time graph-health aggregates. Takes a `{ scope, graphId? }` JSON,
    /// returns the `MaintenanceHealth` JSON.
    #[napi(js_name = "graphHealthJson")]
    pub fn graph_health_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let scope = scope_field(&value)?;
        let graph_id = value.get("graphId").and_then(|v| v.as_str()).map(Id::from);
        let result =
            block_on(self.handle.graph_health(&scope, graph_id.as_ref())).map_err(to_napi_error)?;
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
/// `CoreError` (not `Serialize`) is stringified per step; per-step supersession
/// counts (sourced batches) ride additively — camelCase, omitted when `None`
/// so unsourced outcome JSON is byte-identical to the legacy shape.
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
    #[serde(skip_serializing_if = "Option::is_none")]
    supersession: Option<&'a SupersessionCounts>,
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
                supersession: s.supersession.as_ref(),
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

    /// Lists graphs in a scope. Takes a `Scope` JSON, returns `[KnowledgeGraph, …]`.
    #[napi(js_name = "listGraphsJson")]
    pub fn list_graphs_json(&self, scope_json: String) -> Result<String> {
        let scope: Scope = decode(&scope_json)?;
        let result = block_on(self.handle.list_graphs(&scope)).map_err(to_napi_error)?;
        encode(&result)
    }

    /// Lists one document's chunks in scope. Takes `{ documentId, scope }` JSON,
    /// returns `[KnowledgeChunk, …]` (RFC-0020 T4).
    #[napi(js_name = "listChunksByDocumentJson")]
    pub fn list_chunks_by_document_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let document_id = id_field(&value, "documentId")?;
        let scope = scope_field(&value)?;
        let result = block_on(self.handle.list_chunks_by_document(&document_id, &scope))
            .map_err(to_napi_error)?;
        encode(&result)
    }
}

// ---------------------------------------------------------------------------
// NativeCommunityQueryApi — Louvain community aggregate
// ---------------------------------------------------------------------------

/// Community-query handle proxy (Louvain overview + member index + community-of).
#[napi]
pub struct NativeCommunityQueryApi {
    handle: Arc<dyn CommunityQuery>,
}

#[napi]
impl NativeCommunityQueryApi {
    /// Community overview: top-N communities + inter-community meta-edges.
    #[napi(js_name = "overviewJson")]
    pub fn overview_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let scope = scope_field(&value)?;
        let limit = value.get("limit").and_then(|v| v.as_u64()).unwrap_or(150) as usize;
        let result = block_on(self.handle.overview(&scope, limit)).map_err(to_napi_error)?;
        encode(&result)
    }

    /// Member index: label → entity-id strings. Takes a `Scope` JSON.
    #[napi(js_name = "memberIndexJson")]
    pub fn member_index_json(&self, scope_json: String) -> Result<String> {
        let scope: Scope = decode(&scope_json)?;
        let result = block_on(self.handle.member_index(&scope)).map_err(to_napi_error)?;
        encode(&result)
    }

    /// Community-of: which community an entity belongs to. `{ scope, entityId }`.
    #[napi(js_name = "communityOfJson")]
    pub fn community_of_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let scope = scope_field(&value)?;
        let entity_id = value.get("entityId").and_then(|v| v.as_str()).unwrap_or("");
        let result =
            block_on(self.handle.community_of(&scope, entity_id)).map_err(to_napi_error)?;
        encode(&result)
    }

    /// Scope-filtered record counts (entities/relationships/memories/beliefs/hierarchy).
    #[napi(js_name = "scopeCountsJson")]
    pub fn scope_counts_json(&self, scope_json: String) -> Result<String> {
        let scope: Scope = decode(&scope_json)?;
        let result = block_on(self.handle.scope_counts(&scope)).map_err(to_napi_error)?;
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

    /// Lists live beliefs in a scope. Takes a `Scope` JSON, returns
    /// `[Belief, …]` JSON. (Maintenance reads the active belief set over the
    /// provider surface — surface parity with the standalone belief engine.)
    #[napi(js_name = "listBeliefsJson")]
    pub fn list_beliefs_json(&self, scope_json: String) -> Result<String> {
        let scope: Scope = decode(&scope_json)?;
        let result = block_on(self.handle.list_beliefs(&scope)).map_err(to_napi_error)?;
        encode(&result)
    }

    /// Lists live beliefs in a scope, keyset-paged. Takes
    /// `{ scope, after?, limit? }` JSON (`after` is the prior page's `nextCursor`;
    /// default limit 100), returns a `Page<Belief>` JSON `{ items, nextCursor }`.
    #[napi(js_name = "listBeliefsPagedJson")]
    pub fn list_beliefs_paged_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let scope = scope_field(&value)?;
        let after = value
            .get("after")
            .and_then(|v| v.as_str())
            .map(|s| engram_domain::Cursor::new(s.to_owned()));
        let limit = value.get("limit").and_then(|v| v.as_u64()).unwrap_or(100) as usize;
        let page: Page<Belief> = block_on(self.handle.list_beliefs_paged(
            &scope,
            after.as_ref(),
            limit,
        ))
        .map_err(to_napi_error)?;
        encode(&page)
    }

    /// Lists open contradiction records in a scope. Takes a `Scope` JSON, returns
    /// `[Contradiction, …]` JSON.
    #[napi(js_name = "listContradictionsJson")]
    pub fn list_contradictions_json(&self, scope_json: String) -> Result<String> {
        let scope: Scope = decode(&scope_json)?;
        let result = block_on(self.handle.list_contradictions(&scope)).map_err(to_napi_error)?;
        encode(&result)
    }

    /// Stores or updates a contradiction record. Takes a `Contradiction` JSON,
    /// returns the persisted `Contradiction` JSON.
    #[napi(js_name = "putContradictionJson")]
    pub fn put_contradiction_json(&self, contradiction_json: String) -> Result<String> {
        let contradiction: Contradiction = decode(&contradiction_json)?;
        let result =
            block_on(self.handle.put_contradiction(contradiction)).map_err(to_napi_error)?;
        encode(&result)
    }
}

// ---------------------------------------------------------------------------
// NativeHierarchyApi — hierarchy navigation
// ---------------------------------------------------------------------------

/// Hierarchy handle proxy. Holds an `Arc<dyn HierarchyRepository>` and exposes
/// hierarchy navigation + build as JSON-in / JSON-out methods (mirrors the MCP
/// `hierarchy_path` / `hierarchy_build` tools). The optional `KnowledgeQuery`
/// handle is populated by [`NativeProvider::require_hierarchy_api`] when the
/// provider also wires knowledge-query — `buildHierarchyJson` needs it to read
/// the call edges it clusters; `pathForJson` does not.
#[napi]
pub struct NativeHierarchyApi {
    handle: Arc<dyn HierarchyRepository>,
    knowledge_query: Option<Arc<dyn KnowledgeQuery>>,
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

    /// Builds the hierarchy for a scope: clusters the knowledge graph's call
    /// edges via Louvain communities and persists one layer-0 cluster node per
    /// community plus inter-cluster relations (the write counterpart to
    /// `pathForJson`). Deterministic — no LLM. After this, `pathForJson` returns
    /// navigation results for the scope.
    ///
    /// Takes `{ scope, maxPasses? }` JSON (default `maxPasses` = 3), returns a
    /// `HierarchyBuildStats` JSON (`clusterCount`, `entitiesClustered`,
    /// `totalEntities`, `totalRelationships`, `interClusterRelationCount`).
    /// Throws `CapabilityUnsupported` when knowledge-query is not wired.
    #[napi(js_name = "buildHierarchyJson")]
    pub fn build_hierarchy_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let scope = scope_field(&value)?;
        let max_passes = value.get("maxPasses").and_then(|v| v.as_u64()).unwrap_or(3) as usize;
        let knowledge_query = self.knowledge_query.clone().ok_or_else(|| {
            to_napi_error(engram_runtime::CoreError::CapabilityUnsupported {
                capability: "knowledge_query".to_string(),
                reason: "hierarchy build needs knowledge-query to read call edges".to_string(),
            })
        })?;
        let stats = block_on(engram_integration::build_hierarchy_from_communities(
            &knowledge_query,
            &self.handle,
            &scope,
            max_passes,
            "engram-node",
        ))
        .map_err(to_napi_error)?;
        encode(&stats)
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

/// Outcome-ledger API (F2): report attributed post-action results and read
/// windows. Association data, never causal proof.
#[napi]
pub struct NativeOutcomesApi {
    handle: Arc<dyn OutcomeLedger>,
}

#[napi]
impl NativeOutcomesApi {
    /// Appends one attributed outcome. Takes a `ReportOutcomeRequest` JSON,
    /// returns a `ReportOutcomeResult` JSON.
    #[napi(js_name = "reportOutcomeJson")]
    pub fn report_outcome_json(&self, request_json: String) -> Result<String> {
        let request = decode(&request_json)?;
        let result = block_on(self.handle.report_outcome(request)).map_err(to_napi_error)?;
        encode(&result)
    }

    /// Reads a window of outcomes. Takes an `OutcomeWindow` JSON, returns
    /// `[RecordedOutcome, ...]` JSON.
    #[napi(js_name = "listOutcomesJson")]
    pub fn list_outcomes_json(&self, window_json: String) -> Result<String> {
        let window = decode(&window_json)?;
        let result = block_on(self.handle.list_outcomes(&window)).map_err(to_napi_error)?;
        encode(&result)
    }
}

/// Knowledge change-log API: bounded, resumable invalidation pages plus
/// latest-checkpoint reads (never recallable content).
#[napi]
pub struct NativeChangeLogApi {
    handle: Arc<dyn KnowledgeChangeLog>,
}

#[napi]
impl NativeChangeLogApi {
    /// One page after an opaque cursor. Takes a `KnowledgeChangesRequest`
    /// JSON, returns a `KnowledgeChangesPage` JSON.
    #[napi(js_name = "changesSinceJson")]
    pub fn changes_since_json(&self, request_json: String) -> Result<String> {
        let request = decode(&request_json)?;
        let result = block_on(self.handle.changes_since(&request)).map_err(to_napi_error)?;
        encode(&result)
    }

    /// Latest checkpoint for one (scope, stable source key). Takes
    /// `{ scope, stableSourceKey }` JSON, returns a `ScanCheckpoint` JSON
    /// or null.
    #[napi(js_name = "latestCheckpointJson")]
    pub fn latest_checkpoint_json(&self, request_json: String) -> Result<String> {
        let request = decode::<serde_json::Value>(&request_json)?;
        let scope = scope_field(&request)?;
        let key = request["stableSourceKey"]
            .as_str()
            .or_else(|| request["stable_source_key"].as_str())
            .ok_or_else(|| napi::Error::from_reason("stableSourceKey is required"))?;
        let result = block_on(self.handle.latest_checkpoint(&scope, key)).map_err(to_napi_error)?;
        encode(&result)
    }
}

/// Scan-checkpoint API: the scan coordinator's write side.
#[napi]
pub struct NativeCheckpointsApi {
    handle: Arc<dyn ScanCheckpointStore>,
}

#[napi]
impl NativeCheckpointsApi {
    /// Upserts a checkpoint (stale runs never overwrite newer ones). Takes
    /// a `ScanCheckpoint` JSON, returns the stored `ScanCheckpoint` JSON.
    #[napi(js_name = "putCheckpointJson")]
    pub fn put_checkpoint_json(&self, checkpoint_json: String) -> Result<String> {
        let checkpoint = decode(&checkpoint_json)?;
        let result = block_on(self.handle.put_checkpoint(checkpoint)).map_err(to_napi_error)?;
        encode(&result)
    }
}

/// Operator proposal queue API: submit, list, approve, and reject inert
/// pending writes. Approval promotes through the policy-checked memory write
/// path; `applying` outcomes report the retry-safe in-progress state.
#[napi]
pub struct NativeProposalsApi {
    handle: Arc<ProposalService>,
}

#[napi]
impl NativeProposalsApi {
    /// Submits a proposal. Takes a `ProposalSubmitRequest` JSON, returns a
    /// `ProposalSubmitResult` JSON.
    #[napi(js_name = "submitJson")]
    pub fn submit_json(&self, request_json: String) -> Result<String> {
        let request = decode(&request_json)?;
        let result = block_on(self.handle.submit(request)).map_err(to_napi_error)?;
        encode(&result)
    }

    /// Lists queue records. Takes a `ProposalListRequest` JSON, returns a
    /// `ProposalListResult` JSON.
    #[napi(js_name = "listJson")]
    pub fn list_json(&self, request_json: String) -> Result<String> {
        let request = decode(&request_json)?;
        let result = block_on(self.handle.list(request)).map_err(to_napi_error)?;
        encode(&result)
    }

    /// Approves a pending proposal. Takes a `ProposalApproveRequest` JSON,
    /// returns a `ProposalApproveResult` JSON.
    #[napi(js_name = "approveJson")]
    pub fn approve_json(&self, request_json: String) -> Result<String> {
        let request = decode(&request_json)?;
        let result = block_on(self.handle.approve(request)).map_err(to_napi_error)?;
        encode(&result)
    }

    /// Rejects a pending proposal. Takes a `ProposalRejectRequest` JSON,
    /// returns a `ProposalRejectResult` JSON.
    #[napi(js_name = "rejectJson")]
    pub fn reject_json(&self, request_json: String) -> Result<String> {
        let request = decode(&request_json)?;
        let result = block_on(self.handle.reject(request)).map_err(to_napi_error)?;
        encode(&result)
    }
}

#[napi]
pub struct NativeOntologyApi {
    handle: Arc<dyn OntologyRepository>,
}

#[napi]
impl NativeOntologyApi {
    #[napi(js_name = "getOntologyJson")]
    pub fn get_ontology_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let id = id_field(&value, "id")?;
        let scope = scope_field(&value)?;
        let result = block_on(self.handle.get_ontology(&id, &scope)).map_err(to_napi_error)?;
        encode(&result)
    }

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
    #[napi(js_name = "listConceptsJson")]
    pub fn list_concepts_json(&self, request_json: String) -> Result<String> {
        let value = decode::<serde_json::Value>(&request_json)?;
        let scheme_id = id_field(&value, "schemeId")?;
        let scope = scope_field(&value)?;
        let result =
            block_on(self.handle.list_concepts(&scheme_id, &scope)).map_err(to_napi_error)?;
        encode(&result)
    }

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
    #[napi(js_name = "retrieveCandidatesJson")]
    pub fn retrieve_candidates_json(&self, request_json: String) -> Result<String> {
        let request: engram_domain::RetrievalRequest = decode(&request_json)?;
        let result = block_on(self.handle.retrieve_candidates(&request)).map_err(to_napi_error)?;
        encode(&result)
    }

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
    #[napi(js_name = "embeddingSpace")]
    pub fn embedding_space(&self) -> Result<String> {
        let space = self.handle.embedding_space();
        encode(&space)
    }

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
    #[napi(js_name = "dryRunImportJson")]
    pub fn dry_run_import_json(&self, import_json: String) -> Result<String> {
        let import: engram_integration::ImportData = decode(&import_json)?;
        let result = self.handle.dry_run_import(&import).map_err(to_napi_error)?;
        encode(&result)
    }

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
    #[napi(js_name = "embeddingSpace")]
    pub fn embedding_space(&self) -> Result<String> {
        let space = self.handle.embedding_space();
        encode(&space)
    }

    #[napi(js_name = "providerId")]
    pub fn provider_id(&self) -> String {
        self.handle.provider_id().to_owned()
    }

    /// Generate a query embedding through the configured Rust provider.
    #[napi(js_name = "embedQuery")]
    pub fn embed_query(&self, text: String) -> Result<Vec<f64>> {
        self.handle
            .embed_query(&text)
            .map(|vector| vector.into_iter().map(f64::from).collect())
            .map_err(to_napi_error)
    }

    /// Generate an indexed passage embedding through the configured Rust provider.
    #[napi(js_name = "embedPassage")]
    pub fn embed_passage(&self, text: String) -> Result<Vec<f64>> {
        self.handle
            .embed_passage(&text)
            .map(|vector| vector.into_iter().map(f64::from).collect())
            .map_err(to_napi_error)
    }

    /// Generate passage embeddings in the provider's original batch order.
    #[napi(js_name = "embedBatch")]
    pub fn embed_batch(&self, texts: Vec<String>) -> Result<Vec<Vec<f64>>> {
        self.handle
            .embed_batch(&texts)
            .map(|vectors| {
                vectors
                    .into_iter()
                    .map(|vector| vector.into_iter().map(f64::from).collect())
                    .collect()
            })
            .map_err(to_napi_error)
    }

    #[napi(js_name = "ping")]
    pub fn ping(&self) -> Result<String> {
        Ok("embedding_provider".to_owned())
    }
}

// ---------------------------------------------------------------------------
// scan_repo — fan-in scan over the held provider's knowledge + graph handles
// ---------------------------------------------------------------------------

/// Request shape for [`NativeProvider::scan_repository_json`].
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScanRepoRequest {
    /// Filesystem path for the DOCUMENT-ONLY legacy lane (no `repo`):
    /// documents under `path` are scanned with code files excluded. NEVER an
    /// input to bound code ingestion (supplying both is a typed rejection).
    #[serde(default)]
    path: Option<String>,
    /// Scope the scanned entities/relationships/graph land in.
    scope: Scope,
    /// The BOUND repository for registered-locator code ingestion (review r6
    /// finding 2): the RepoId is the authority key — never the path or its
    /// basename. With `repo`, acquisition goes through the REGISTERED
    /// locator and the generation staging/publication pipeline.
    repo: Option<engram_domain::RepoId>,
    /// Optional branch assertion: equality-only against the bound ref.
    #[serde(default)]
    branch_ref: Option<String>,
    /// Optional scan-filter config merged over the builtin defaults.
    #[serde(default)]
    scan_filter: Option<ScanFilterConfig>,
}

/// `{ scope, repo, branch_ref }` for [`NativeProvider::code_bind_json`].
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodeBindRequest {
    scope: Scope,
    repo: engram_domain::RepoId,
    branch_ref: String,
}

/// `{ scope, repo }` for [`NativeProvider::code_status_json`] and
/// [`NativeProvider::code_refresh_json`].
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodeStatusRequest {
    scope: Scope,
    repo: engram_domain::RepoId,
}

/// `{ scope, repo, path, mode? }` for
/// [`NativeProvider::code_register_json`].
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodeRegisterRequest {
    scope: Scope,
    repo: engram_domain::RepoId,
    path: String,
    mode: Option<String>,
}

/// System actor attributed to bound code operations performed through the
/// binding layer (same identity discipline as `scan_actor`).
fn system_code_actor() -> Actor {
    Actor {
        id: Id::from("engram-node-code"),
        kind: ActorKind::Agent,
        display_name: None,
        metadata: None,
    }
}

/// The outcome of [`scan_via_provider`]: either a published bound
/// generation or a completed legacy document-only scan summary.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScanOutcome {
    /// `bound` (registered-locator pipeline) or `legacy-document-only`.
    mode: &'static str,
    #[serde(flatten)]
    payload: ScanPayload,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum ScanPayload {
    Ingested(engram_integration::code_binding::IngestedGeneration),
    Documents(ScanSummary),
}

/// The single native scan dispatcher (review r6 finding 2):
///
/// - `repo` present → REGISTERED-LOCATOR acquisition + generation
///   staging/publication through the SAME authorized coordinator the recipes
///   and MCP use. The caller's `path` (when supplied) is a typed rejection —
///   binding-existence was never authorization, and a caller path is never
///   an ingestion input. Requires the provider's ingestion lanes (absent
///   lanes are a typed `CapabilityUnsupported`, never a checkout fallback).
/// - `repo` absent, `path` present → the EXPLICITLY DOCUMENT-ONLY legacy
///   scan: code files are excluded by the document-only filter (the response
///   carries the `legacy-document-only` mode label); documents never need a
///   binding.
/// - neither → typed `InvalidRequest`.
fn scan_via_provider(
    provider: &EngramProvider,
    request: &ScanRepoRequest,
) -> CoreResult<ScanOutcome> {
    match (&request.repo, &request.path) {
        (Some(_repo), Some(_)) => Err(CoreError::InvalidRequest {
            reason: "path is not an input to bound code ingestion — the registered locator \
                     is the acquisition authority; drop `path` and register the repository"
                .to_owned(),
        }),
        (Some(repo), None) => {
            let coordinator = provider.code_bindings()?;
            let lanes = provider.code_ingestion()?;
            let filter = request.scan_filter.as_ref();
            let ingested = futures::executor::block_on(coordinator.ingest_registered(
                &system_code_actor(),
                &request.scope,
                repo,
                request.branch_ref.as_deref(),
                filter,
                lanes,
            ))?;
            Ok(ScanOutcome {
                mode: "bound",
                payload: ScanPayload::Ingested(ingested),
            })
        }
        (None, Some(path)) => {
            let knowledge = provider.require_knowledge()?.clone();
            let graph = provider.require_graph()?.clone();
            let repo = KnowledgeRepoGraph::new(knowledge, graph);
            let scan_filter = match &request.scan_filter {
                Some(cfg) => ScanFilter::document_only_with(cfg),
                None => ScanFilter::document_only(),
            };
            let opts = ScanOptions {
                scope: request.scope.clone(),
                policy: scan_policy(),
                actor: scan_actor(),
                source_name: "engram-node-legacy-docs".to_owned(),
                max_bytes: 0,
                manifest: std::collections::HashMap::new(),
                scan_filter,
            };
            let (summary, _manifest) =
                scan_repository(std::path::Path::new(path), &opts, &repo, |_| ())?;
            Ok(ScanOutcome {
                mode: "legacy-document-only",
                payload: ScanPayload::Documents(summary),
            })
        }
        (None, None) => Err(CoreError::InvalidRequest {
            reason: "BindingRequired: code ingestion requires a persisted binding — name the \
                     bound repository (`repo`), or pass `path` for the document-only legacy \
                     scan"
                .to_owned(),
        }),
    }
}

/// Default write policy for scanned knowledge (workspace-visible, durable).
fn scan_policy() -> Policy {
    Policy {
        visibility: Visibility::Workspace,
        retention: Retention::Durable,
        sensitivity: Some(Sensitivity::Low),
        allowed_uses: vec![AllowedUse::Retrieval],
        expires_at: None,
        delete_mode: Some(DeleteMode::Tombstone),
    }
}

/// System actor attributed to scans performed through the binding.
fn scan_actor() -> Actor {
    Actor {
        id: Id::from("engram-node"),
        kind: ActorKind::Agent,
        display_name: Some("engram-node".to_owned()),
        metadata: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engram_domain::ScopeMappingStrategy;
    use engram_integration::StepOutcome;
    use engram_integration::{CapabilityPolicy, EmbeddingProviderConfig, MigrationMode};

    /// Review r6 finding 2 (repairs the struct drift that left this test
    /// uncompilable since TASK-020): the provider's path-based lane is the
    /// DOCUMENT-ONLY legacy scan — code files are excluded; code intent
    /// (`repo`) without a registered locator is a typed denial; supplying
    /// BOTH path and repo is rejected.
    #[test]
    fn scan_via_provider_document_only_lane() {
        let dir =
            std::env::temp_dir().join(format!("engram-node-scan-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // One code file (must be EXCLUDED) and one document (must land).
        std::fs::write(dir.join("fixture.rs"), "pub fn hello() {}\n").unwrap();
        std::fs::write(dir.join("notes.md"), "# Notes\nlegacy lane text\n").unwrap();

        let config = EngramConfig::new(
            dir.join("store"),
            dir.clone(),
            ScopeMappingStrategy::Strict,
            EmbeddingProviderConfig {
                provider_type: "none".to_owned(),
                model: "test_model".to_owned(),
                dimensions: 384,
                prompt_profile: "query".to_owned(),
                normalization: None,
                ..Default::default()
            },
            MigrationMode::DryRun,
            CapabilityPolicy::FailClosed,
        );
        let provider = EngramProvider::open(&config).expect("provider opens");

        let scope = Scope {
            tenant: "test".to_owned(),
            subject: None,
            workspace: Some("test".to_owned()),
            session: None,
            environment: None,
        };
        // Neither lane selected: typed guidance.
        let neither = scan_via_provider(
            &provider,
            &ScanRepoRequest {
                path: None,
                scope: scope.clone(),
                repo: None,
                branch_ref: None,
                scan_filter: None,
            },
        );
        assert!(
            matches!(&neither, Err(CoreError::InvalidRequest { reason }) if reason.contains("BindingRequired")),
            "neither lane selected must deny typed, got {neither:?}"
        );
        // Code intent without a registration: the pipeline's typed denial.
        let unregistered = scan_via_provider(
            &provider,
            &ScanRepoRequest {
                path: None,
                scope: scope.clone(),
                repo: Some(engram_domain::RepoId("repo-unregistered".to_owned())),
                branch_ref: None,
                scan_filter: None,
            },
        );
        assert!(
            matches!(&unregistered, Err(CoreError::InvalidRequest { reason }) if reason.contains("no registered repository")),
            "code ingestion without a locator must deny typed, got {unregistered:?}"
        );
        // Both path and repo: rejected — path is never an ingestion input.
        let both = scan_via_provider(
            &provider,
            &ScanRepoRequest {
                path: Some(dir.to_string_lossy().to_string()),
                scope: scope.clone(),
                repo: Some(engram_domain::RepoId("repo-both".to_owned())),
                branch_ref: None,
                scan_filter: None,
            },
        );
        assert!(
            matches!(&both, Err(CoreError::InvalidRequest { reason }) if reason.contains("path is not an input")),
            "path + repo must be rejected, got {both:?}"
        );
        // Document-only lane over the path: documents land, code is excluded.
        let outcome = scan_via_provider(
            &provider,
            &ScanRepoRequest {
                path: Some(dir.to_string_lossy().to_string()),
                scope: scope.clone(),
                repo: None,
                branch_ref: None,
                scan_filter: None,
            },
        )
        .expect("document-only scan runs");
        assert_eq!(outcome.mode, "legacy-document-only");
        let ScanPayload::Documents(summary) = outcome.payload else {
            panic!("document lane returns a summary");
        };
        assert!(summary.scanned >= 1, "fixture scanned: {summary:?}");

        let entities = block_on(
            provider
                .require_knowledge_query()
                .expect("query wired")
                .list_entities(&scope),
        )
        .expect("list entities");
        assert!(
            !entities.is_empty(),
            "the document's entities persist in the knowledge store"
        );
        assert!(
            entities
                .iter()
                .all(|entity| entity.kind != engram_domain::EntityKind::Function),
            "the document-only lane never ingests code entities"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Surface parity for sourced batches: the N-API `ingestJson` outcome
    /// projection carries per-step supersession counts (camelCase, omitted
    /// when `None`), so a TS host sees exactly what a Rust embedder sees.
    #[test]
    fn batch_outcome_json_carries_supersession_counts() {
        let counts = SupersessionCounts {
            examined: 3,
            forgotten_facts: 2,
            archived_entities: 1,
            ..SupersessionCounts::default()
        };
        let mut steps: Vec<StepOutcome> = engram_integration::ALL_STEPS
            .iter()
            .map(|&s| StepOutcome::ok(s, StepStatus::Succeeded))
            .collect();
        let facts_index = engram_integration::ALL_STEPS
            .iter()
            .position(|s| *s == BatchStep::Facts)
            .expect("facts step");
        steps[facts_index] =
            StepOutcome::ok(BatchStep::Facts, StepStatus::Succeeded).with_counts(counts);

        let outcome = BatchOutcome::from_steps(steps);
        let projected = BatchOutcomeJson::from(&outcome);
        let json = serde_json::to_value(&projected).expect("project");

        let facts = &json["steps"][facts_index];
        assert_eq!(facts["step"], "Facts");
        assert_eq!(
            facts["supersession"]["examined"], 3,
            "camelCase counts ride"
        );
        assert_eq!(
            facts["supersession"]["forgottenFacts"], 2,
            "camelCase field"
        );
        assert_eq!(
            facts["supersession"]["archivedEntities"], 1,
            "camelCase field"
        );

        // Unsourced steps omit the key entirely (legacy JSON shape intact).
        let episode_index = engram_integration::ALL_STEPS
            .iter()
            .position(|s| *s == BatchStep::Episode)
            .expect("episode step");
        assert!(
            json["steps"][episode_index].get("supersession").is_none(),
            "None counts are omitted: {}",
            json["steps"][episode_index]
        );
    }
}
