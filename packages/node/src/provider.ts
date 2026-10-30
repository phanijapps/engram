import { createNativeEmbeddingProviderTransport, type NativeEmbeddingProviderTransport } from "./embedding-provider.js";
import type {
  CodeBinding,
  CodeGeneration,
  ContextPayload,
  RetrievalRequest,
  Scope,
  WriteMemoryRequest,
  WriteMemoryResponse
} from "@engram/contracts";

import {
  loadNativeBinding,
  type NativeBinding,
  type NativeBindingLoader,
  type NativeProviderBinding
} from "./binding.js";

/** Options for constructing a native provider transport. */
export interface NativeProviderTransportOptions {
  /** Inject a loaded binding (overrides `loader`). */
  binding?: NativeBinding;
  loader?: NativeBindingLoader;
  /** Serialized `EngramConfig` (v1 shape). Required when `provider` is not injected. */
  configJson?: string;
  /** Inject an already-constructed `NativeProvider` (tests, or a shared provider). */
  provider?: NativeProviderBinding;
}

/**
 * The provider-pattern surface the ingest / HTTP-MCP / maintenance modules
 * compose on (RFC-0017 Phase A). Each method dispatches to the held
 * `NativeProvider` — one engine-routed storage lifecycle, no per-family re-open.
 *
 * This is transport over Rust behavior, not a second implementation: it encodes
 * typed requests to JSON, calls the binding, and decodes the JSON result.
 */
/** A keyset page of memory records (mirrors engram-domain `Page<MemoryRecord>`). */
export interface MemoryPage {
  items: unknown[];
  nextCursor: string | null;
}

/** A keyset page of beliefs (mirrors engram-domain `Page<Belief>`). */
export interface BeliefPage {
  items: unknown[];
  nextCursor: string | null;
}

/** Record counts by semantic type (mirrors engram-integration `RecordCounts`). */
export interface RecordCounts {
  memories: number;
  entities: number;
  relationships: number;
  sources: number;
  documents: number;
  chunks: number;
  beliefs: number;
}

/** Diagnostics snapshot (mirrors engram-integration `DiagnosticsSnapshot`); the
 *  viz reads `record_counts` for graph/memory/belief stats. */
export interface Diagnostics {
  record_counts: RecordCounts;
  [key: string]: unknown;
}

/** Community-overview payload from the Louvain aggregate (mirrors engram-domain CommunityOverview). */
export interface CommunityOverviewData {
  communities: Array<{ label: number; memberCount: number }>;
  edges: Array<{ sourceLabel: number; targetLabel: number; weight: number }>;
  totalCommunities: number;
}

/** Member index: community label → entity-id strings. */
export type CommunityMemberIndex = Record<number, string[]>;

/** Scope-filtered record counts (mirrors engram-domain ScopeCounts). */
export interface ScopeCounts {
  entities: number;
  relationships: number;
  memories: number;
  beliefs: number;
  hierarchyNodes: number;
  hierarchyRelations: number;
}

/** One on-request ref observation as the Rust coordinator serializes it
 *  (policy `on_request`; r6 finding 6: carried with GENERATED contract types,
 *  never `unknown`). */
export interface CodeRefObservation {
  branchRef: string;
  observedOid: string;
  observedAt: string;
  policy: "on_request";
}

/** Truthful freshness classification (never checkout-HEAD-derived):
 *  the serde externally-tagged shape — `"unknown"` (unit variant) or
 *  `{ observed: { matchesIndexed } }`. */
export type CodeFreshnessState = "unknown" | { observed: { matchesIndexed: boolean | null } };

/** Binding-keyed freshness facts. */
export interface CodeRefFreshness {
  branchRef: string;
  policy: "on_request";
  observedOid: string | null;
  observedAt: string | null;
  state: CodeFreshnessState;
}

/** The binding-keyed checkpoint pointer (RepoId-keyed, never basename-keyed);
 *  its generation facts are the GENERATED `CodeGeneration` contract. */
export interface CodeBindingCheckpoint {
  repoId: string;
  branchRef: string;
  bindingVersion: number;
  codeEpoch: number;
  generation: CodeGeneration | null;
}

/** Serialized `CodeBindingStatus` — binding carried as the GENERATED
 *  `CodeBinding` contract. */
export interface CodeBindingStatus {
  binding: CodeBinding;
  attempt: unknown;
  codeEpoch: number;
  indexedCommitOid: string | null;
  freshness: CodeRefFreshness;
  checkpoint: CodeBindingCheckpoint;
}

/** Acquisition locator registration — the Rust port's serialization
 *  (snake_case fields and enum values). */
export interface RepositoryRegistration {
  repo_id: string;
  locator_id: string;
  acquisition: "local_only" | "configured_remote";
}

/** The typed transport over the native binding — every route in the TS MCP
 *  server dispatches through one of these methods. */
export interface NativeProviderTransport {
  /** Access the configured Rust embedding provider; throws when unavailable. */
  embedding(): NativeEmbeddingProviderTransport;
  /** The serialized `CapabilityReport` for the open provider. */
  capabilities(): Promise<unknown>;
  /** Unified recall (fused across lanes) → `ContextPayload`. */
  recall(request: RetrievalRequest): Promise<ContextPayload>;
  /** Write a memory + lifecycle event. */
  write(request: WriteMemoryRequest): Promise<WriteMemoryResponse>;
  /** Treesitter-index a BOUND repository into the project workspace
   *  (TASK-020: `repo` names the persisted binding — the durable cell, not
   *  the path, is the authority; `branchRef` may assert equality). */
  scan(request: {
    path: string;
    scope: unknown;
    /** The bound repository (TASK-020): code scans require a persisted
     *  binding; omitting it denies with the typed `BindingRequired`
     *  envelope from the Rust side. */
    repo?: string;
    branchRef?: string;
    scanFilter?: unknown;
  }): Promise<unknown>;
  /** Bind a repository branch (TASK-020): same-ref idempotent, other branch
   *  BindingConflict. */
  codeBind(request: { scope: Scope; repo: string; branchRef: string }): Promise<CodeBinding>;
  /** Register a repository's acquisition locator (r6 finding 5): the SAME
   *  shared seam as stdio/one-shot — `mode` is `local-only` (default) or
   *  `configured-remote`. */
  codeRegister(request: {
    scope: Scope;
    repo: string;
    path: string;
    mode?: "local-only" | "configured-remote";
  }): Promise<RepositoryRegistration>;
  /** Binding-keyed status: indexed OID, on-request ref freshness (unknown
   *  until an explicit observation — never checkout-HEAD-derived), and the
   *  binding-keyed checkpoint. */
  codeStatus(request: { scope: Scope; repo: string }): Promise<CodeBindingStatus>;
  /** ONE on-request refresh of the bound branch ref, resolved from the
   *  REGISTERED locator's local git (r6 finding 5) — the caller never
   *  supplies an OID; the former codeObserveRef surface is gone. */
  codeRefresh(request: { scope: Scope; repo: string }): Promise<CodeRefObservation>;
  /** Run a consolidation cycle (reflection + decay). A system requester is
   *  injected; callers pass only `scope` + optional `dryRun` / `since` / `until`. */
  consolidate(request: {
    scope: unknown;
    dryRun?: boolean;
    since?: string;
    until?: string;
  }): Promise<unknown>;
  /** File-level import graph with module→file resolution (RFC-0020 Phase 2).
   *  One implementation (the Rust facade) — the binding is a transport. */
  fileDependencies(scope: unknown): Promise<unknown>;
  /** Natural-language explore: seeded bounded subgraph (RFC-0020 Phase 2).
   *  includeCodeArtifacts opts Variable/Module/File/Repository entities into
   *  the discovery seed list (suppressed by default; always in the graph). */
  explore(request: {
    scope: unknown;
    query: string;
    depth?: number;
    maxNodes?: number;
    maxEdges?: number;
    includeCodeArtifacts?: boolean;
  }): Promise<unknown>;
  /** Upsert a knowledge entity. */
  putEntity(entity: unknown): Promise<unknown>;
  /** Upsert a knowledge relationship. */
  putRelationship(relationship: unknown): Promise<unknown>;
  /** Upsert a belief. */
  beliefPut(belief: unknown): Promise<unknown>;
  /** List live beliefs visible to `scope` (Rust-backed; maintenance reads the active set). */
  listBeliefs(scope: unknown): Promise<unknown[]>;
  /** List live beliefs visible to `scope`, keyset-paged (bounded reads on large scopes). */
  listBeliefsPaged(
    scope: unknown,
    after?: string | null,
    limit?: number,
  ): Promise<BeliefPage>;
  /** List open contradiction records visible to `scope` (Rust-backed). */
  listContradictions(scope: unknown): Promise<unknown[]>;
  /** Upsert a contradiction record (Rust-backed). */
  putContradiction(contradiction: unknown): Promise<unknown>;
  /** Forget (delete/redact/tombstone/archive) a memory. */
  forget(request: unknown): Promise<unknown>;
  /** Best-effort batch ingest. */
  batchIngest(request: unknown): Promise<unknown>;
  /** List memories visible to `scope` as a keyset page (Rust-backed; no SQL in TS).
   *  `after` is the opaque `nextCursor` from a prior page. */
  listMemoriesPaged(scope: unknown, after?: string | null, limit?: number): Promise<MemoryPage>;
  /** Point-in-time diagnostics snapshot (record counts, etc.) — Rust-backed. */
  diagnostics(): Promise<Diagnostics>;
  /** Community overview: top-N Louvain communities + meta-edges (Rust-backed). */
  communityOverview(scope: unknown, limit?: number): Promise<CommunityOverviewData>;
  /** Member index: community label → entity-id strings (Rust-backed). */
  communityMemberIndex(scope: unknown): Promise<CommunityMemberIndex>;
  /** Scope-filtered record counts (Rust-backed fast SQL COUNT). */
  scopeCounts(scope: unknown): Promise<ScopeCounts>;
  /** Get a single belief by id (Rust-backed). Returns the `Belief` JSON or null. */
  beliefGet(request: unknown): Promise<unknown>;
  /** Retract (soft-delete) a belief (Rust-backed). Returns the retracted belief. */
  beliefRetract(request: unknown): Promise<unknown>;
  /** List beliefs marked stale in scope (Rust-backed). Returns `[Belief, …]`. */
  beliefStaleList(scope: unknown): Promise<unknown[]>;
  /** List all entities in scope (engine-neutral KnowledgeQuery port; Rust-backed).
   *  Returns `[KnowledgeEntity, …]`. */
  listEntities(scope: unknown): Promise<unknown[]>;
  /** List all relationships in scope (engine-neutral KnowledgeQuery port). Each
   *  relationship's `subject` / `object` are `EntityRef` (`{id, kind, name, aliases}`). */
  listRelationships(scope: unknown): Promise<unknown[]>;
  /** List all graphs in scope (engine-neutral KnowledgeQuery port). Returns
   *  `[KnowledgeGraph, …]`; each graph's `metadata` carries its `document_id`. */
  listGraphs(scope: unknown): Promise<unknown[]>;
  /** List one document's chunks in scope (bounded per-document; RFC-0020 T4).
   *  Returns `[KnowledgeChunk, …]`. */
  listChunksByDocument(documentId: string, scope: unknown): Promise<unknown[]>;
  /** Hierarchy navigation path for seed entity ids (Rust-backed). */
  hierarchyPath(request: unknown): Promise<unknown>;
  /** Build the hierarchy for a scope: cluster the KG's call edges via Louvain
   *  and persist layer-0 cluster nodes + inter-cluster relations (Rust-backed,
   *  deterministic — no LLM). Returns `HierarchyBuildStats`. */
  buildHierarchy(scope: unknown, maxPasses?: number): Promise<unknown>;
  /** Get an entity by id inside a scope (graph API; returns entity JSON or null). */
  getEntity(id: string, scope: unknown): Promise<unknown>;
  /** Graph neighbors of a node `{ graphId, nodeId, scope, limit? }` (graph API). */
  graphNeighbors(request: unknown): Promise<unknown>;
  /** Build a dry-run maintenance plan (fills before/after previews; non-mutating). */
  graphMaintenanceBuildPlan(request: unknown): Promise<unknown>;
  /** Apply (or preview) a reviewed maintenance plan `{ plan, mode }`. */
  graphMaintenanceApplyPlan(request: unknown): Promise<unknown>;
  /** Deterministic candidate detection `{ scope, graphId?, policy }` (no LLM). */
  graphMaintenanceCandidates(request: unknown): Promise<unknown[]>;
  /** Point-in-time graph-health aggregates `{ scope, graphId? }`. */
  graphMaintenanceHealth(request: unknown): Promise<unknown>;
  /** Upsert a replayable procedure (Layer 6). Returns the persisted `Procedure`. */
  procedureUpsert(procedure: unknown): Promise<unknown>;
  /** List procedures in scope (Layer 6). Returns `[Procedure, …]`. */
  procedureList(scope: unknown): Promise<unknown[]>;
  /** Bump a procedure's success counter `{ id, scope }` (Layer 6). */
  procedureIncrementSuccess(request: unknown): Promise<unknown>;
  /** Bump a procedure's failure counter `{ id, scope }` (Layer 6). */
  procedureIncrementFailure(request: unknown): Promise<unknown>;

  /** Outcome loop: report an attributed task result (caller-asserted, non-causal). */
  reportOutcome(request: unknown): Promise<unknown>;

  /** Outcome loop: read a window of attributed outcomes. */
  listOutcomes(window: unknown): Promise<unknown[]>;
}

/** Creates a transport over the held `NativeProvider`. */
export function createNativeProviderTransport(
  options: NativeProviderTransportOptions
): NativeProviderTransport {
  if (options.provider) {
    return new JsonNativeProviderTransport(options.provider);
  }
  if (!options.configJson) {
    throw new Error(
      "createNativeProviderTransport: configJson is required when provider is not injected"
    );
  }
  const binding = options.binding ?? loadNativeBinding(options.loader);
  return new JsonNativeProviderTransport(new binding.NativeProvider(options.configJson));
}

class JsonNativeProviderTransport implements NativeProviderTransport {
  constructor(private readonly provider: NativeProviderBinding) {}

  embedding(): NativeEmbeddingProviderTransport {
    return createNativeEmbeddingProviderTransport(this.provider);
  }

  async capabilities(): Promise<unknown> {
    return decode(this.provider.capabilitiesJson());
  }

  async recall(request: RetrievalRequest): Promise<ContextPayload> {
    return decode<ContextPayload>(this.provider.requireRecallApi().recallJson(encode(request)));
  }

  async write(request: WriteMemoryRequest): Promise<WriteMemoryResponse> {
    return decode<WriteMemoryResponse>(this.provider.requireMemoryApi().writeJson(encode(request)));
  }

  async scan(request: {
    path: string;
    scope: unknown;
    repo?: string;
    branchRef?: string;
    scanFilter?: unknown;
  }): Promise<unknown> {
    return decode(this.provider.scanRepositoryJson(encode(request)));
  }

  async codeBind(request: { scope: Scope; repo: string; branchRef: string }): Promise<CodeBinding> {
    return decode<CodeBinding>(this.provider.codeBindJson(encode(request)));
  }

  async codeStatus(request: { scope: Scope; repo: string }): Promise<CodeBindingStatus> {
    return decode<CodeBindingStatus>(this.provider.codeStatusJson(encode(request)));
  }

  async codeRegister(request: {
    scope: Scope;
    repo: string;
    path: string;
    mode?: "local-only" | "configured-remote";
  }): Promise<RepositoryRegistration> {
    return decode<RepositoryRegistration>(this.provider.codeRegisterJson(encode(request)));
  }

  async codeRefresh(request: { scope: Scope; repo: string }): Promise<CodeRefObservation> {
    return decode<CodeRefObservation>(this.provider.codeRefreshJson(encode(request)));
  }

  async consolidate(request: {
    scope: unknown;
    dryRun?: boolean;
    since?: string;
    until?: string;
  }): Promise<unknown> {
    // ConsolidationRequest.requester is required server-side; the facade injects
    // a system agent so callers pass only { scope, dryRun, since, until }.
    const full = {
      scope: request.scope,
      requester: {
        actor: { id: "engram-node", kind: "agent", displayName: "engram-node" }
      },
      ...(request.dryRun !== undefined ? { dryRun: request.dryRun } : {}),
      ...(request.since ? { since: request.since } : {}),
      ...(request.until ? { until: request.until } : {})
    };
    return decode(this.provider.consolidateJson(encode(full)));
  }

  /** File-level import graph with module→file resolution (RFC-0020 Phase 2).
   *  One implementation (the Rust facade) — the binding is a transport. */
  async fileDependencies(scope: unknown): Promise<unknown> {
    return decode(this.provider.fileDependenciesJson(encode(scope)));
  }

  /** Natural-language explore: seeded bounded subgraph (RFC-0020 Phase 2). */
  async explore(request: {
    scope: unknown;
    query: string;
    depth?: number;
    maxNodes?: number;
    maxEdges?: number;
    includeCodeArtifacts?: boolean;
  }): Promise<unknown> {
    return decode(this.provider.exploreJson(encode(request)));
  }

  async putEntity(entity: unknown): Promise<unknown> {
    return decode(this.provider.requireGraphApi().putEntityJson(encode(entity)));
  }

  async putRelationship(relationship: unknown): Promise<unknown> {
    return decode(
      this.provider.requireGraphApi().putRelationshipJson(encode(relationship))
    );
  }

  async beliefPut(belief: unknown): Promise<unknown> {
    return decode(this.provider.requireBeliefsApi().upsertBeliefJson(encode(belief)));
  }

  async listBeliefs(scope: unknown): Promise<unknown[]> {
    return decode<unknown[]>(this.provider.requireBeliefsApi().listBeliefsJson(encode(scope)));
  }

  async listBeliefsPaged(
    scope: unknown,
    after?: string | null,
    limit?: number,
  ): Promise<BeliefPage> {
    return decode<BeliefPage>(
      this.provider.requireBeliefsApi().listBeliefsPagedJson(
        encode({
          scope,
          ...(after ? { after } : {}),
          ...(limit ? { limit } : {}),
        }),
      ),
    );
  }

  async listContradictions(scope: unknown): Promise<unknown[]> {
    return decode<unknown[]>(
      this.provider.requireBeliefsApi().listContradictionsJson(encode(scope)),
    );
  }

  async putContradiction(contradiction: unknown): Promise<unknown> {
    return decode(
      this.provider.requireBeliefsApi().putContradictionJson(encode(contradiction)),
    );
  }

  async forget(request: unknown): Promise<unknown> {
    return decode(this.provider.requireMemoryApi().forgetJson(encode(request)));
  }

  async batchIngest(request: unknown): Promise<unknown> {
    return decode(this.provider.requireBatchApi().ingestJson(encode(request)));
  }

  async listMemoriesPaged(
    scope: unknown,
    after?: string | null,
    limit?: number
  ): Promise<MemoryPage> {
    return decode<MemoryPage>(
      this.provider.requireMemoryApi().listMemoriesPagedJson(
        encode({
          scope,
          ...(after ? { after } : {}),
          ...(limit ? { limit } : {}),
        })
      )
    );
  }

  async diagnostics(): Promise<Diagnostics> {
    return decode<Diagnostics>(this.provider.requireObservabilityApi().diagnosticsJson());
  }

  async communityOverview(scope: unknown, limit?: number): Promise<CommunityOverviewData> {
    return decode<CommunityOverviewData>(
      this.provider.requireCommunityQueryApi().overviewJson(
        encode({ scope, ...(limit ? { limit } : {}) })
      )
    );
  }

  async communityMemberIndex(scope: unknown): Promise<CommunityMemberIndex> {
    return decode<CommunityMemberIndex>(
      this.provider.requireCommunityQueryApi().memberIndexJson(encode(scope))
    );
  }

  async scopeCounts(scope: unknown): Promise<ScopeCounts> {
    return decode<ScopeCounts>(
      this.provider.requireCommunityQueryApi().scopeCountsJson(encode(scope))
    );
  }

  async beliefGet(request: unknown): Promise<unknown> {
    return decode(this.provider.requireBeliefsApi().getBeliefJson(encode(request)));
  }

  async beliefRetract(request: unknown): Promise<unknown> {
    return decode(this.provider.requireBeliefsApi().retractBeliefJson(encode(request)));
  }

  async beliefStaleList(scope: unknown): Promise<unknown[]> {
    return decode<unknown[]>(
      this.provider.requireBeliefsApi().listStaleBeliefsJson(encode(scope)),
    );
  }

  async listEntities(scope: unknown): Promise<unknown[]> {
    return decode<unknown[]>(
      this.provider.requireKnowledgeQueryApi().listEntitiesJson(encode(scope)),
    );
  }

  async listRelationships(scope: unknown): Promise<unknown[]> {
    return decode<unknown[]>(
      this.provider.requireKnowledgeQueryApi().listRelationshipsJson(encode(scope)),
    );
  }

  async listGraphs(scope: unknown): Promise<unknown[]> {
    return decode<unknown[]>(
      this.provider.requireKnowledgeQueryApi().listGraphsJson(encode(scope)),
    );
  }

  async listChunksByDocument(documentId: string, scope: unknown): Promise<unknown[]> {
    return decode<unknown[]>(
      this.provider.requireKnowledgeQueryApi().listChunksByDocumentJson(
        encode({ documentId, scope }),
      ),
    );
  }

  async hierarchyPath(request: unknown): Promise<unknown> {
    return decode(this.provider.requireHierarchyApi().pathForJson(encode(request)));
  }

  async buildHierarchy(scope: unknown, maxPasses?: number): Promise<unknown> {
    return decode(
      this.provider
        .requireHierarchyApi()
        .buildHierarchyJson(encode({ scope, ...(maxPasses ? { maxPasses } : {}) })),
    );
  }

  async getEntity(id: string, scope: unknown): Promise<unknown> {
    return decode(this.provider.requireGraphApi().getEntityJson(encode({ id, scope })));
  }

  async graphNeighbors(request: unknown): Promise<unknown> {
    return decode(this.provider.requireGraphApi().neighborsJson(encode(request)));
  }

  async graphMaintenanceBuildPlan(request: unknown): Promise<unknown> {
    return decode(this.provider.requireGraphMaintenanceApi().buildPlanJson(encode(request)));
  }

  async graphMaintenanceApplyPlan(request: unknown): Promise<unknown> {
    return decode(this.provider.requireGraphMaintenanceApi().applyPlanJson(encode(request)));
  }

  async graphMaintenanceCandidates(request: unknown): Promise<unknown[]> {
    return decode<unknown[]>(
      this.provider.requireGraphMaintenanceApi().listMaintenanceCandidatesJson(encode(request)),
    );
  }

  async graphMaintenanceHealth(request: unknown): Promise<unknown> {
    return decode(this.provider.requireGraphMaintenanceApi().graphHealthJson(encode(request)));
  }

  async procedureUpsert(procedure: unknown): Promise<unknown> {
    return decode(this.provider.requireProceduresApi().upsertJson(encode(procedure)));
  }

  async procedureList(scope: unknown): Promise<unknown[]> {
    return decode<unknown[]>(this.provider.requireProceduresApi().listJson(encode(scope)));
  }

  async procedureIncrementSuccess(request: unknown): Promise<unknown> {
    return decode(
      this.provider.requireProceduresApi().incrementSuccessJson(encode(request)),
    );
  }

  async procedureIncrementFailure(request: unknown): Promise<unknown> {
    return decode(
      this.provider.requireProceduresApi().incrementFailureJson(encode(request)),
    );
  }

  /** Operator proposal queue: submit an inert pending memory write. */
  async proposalSubmit(request: unknown): Promise<unknown> {
    return decode(this.provider.requireProposalsApi().submitJson(encode(request)));
  }

  /** Operator proposal queue: list records (optionally by status), paged. */
  async proposalList(request: unknown): Promise<unknown> {
    return decode(this.provider.requireProposalsApi().listJson(encode(request)));
  }

  /** Operator proposal queue: approve (policy-checked promotion; retry-safe). */
  async proposalApprove(request: unknown): Promise<unknown> {
    return decode(this.provider.requireProposalsApi().approveJson(encode(request)));
  }

  /** Outcome loop: report an attributed task result (association, non-causal). */
  async reportOutcome(request: unknown): Promise<unknown> {
    return decode(this.provider.requireOutcomesApi().reportOutcomeJson(encode(request)));
  }

  /** Outcome loop: read a window of attributed outcomes. */
  async listOutcomes(window: unknown): Promise<unknown[]> {
    return decode<unknown[]>(this.provider.requireOutcomesApi().listOutcomesJson(encode(window)));
  }

  /** Operator proposal queue: reject (payload stays inert). */
  async proposalReject(request: unknown): Promise<unknown> {
    return decode(this.provider.requireProposalsApi().rejectJson(encode(request)));
  }
}

function encode(value: unknown): string {
  return JSON.stringify(value);
}

function decode<T>(json: string): T {
  return JSON.parse(json) as T;
}
