import type {
  ContextPayload,
  RetrievalRequest,
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

export interface NativeProviderTransport {
  /** The serialized `CapabilityReport` for the open provider. */
  capabilities(): Promise<unknown>;
  /** Unified recall (fused across lanes) → `ContextPayload`. */
  recall(request: RetrievalRequest): Promise<ContextPayload>;
  /** Write a memory + lifecycle event. */
  write(request: WriteMemoryRequest): Promise<WriteMemoryResponse>;
  /** Treesitter-index a repository into the project workspace. */
  scan(request: { path: string; scope: unknown; scanFilter?: unknown }): Promise<unknown>;
  /** Run a consolidation cycle (reflection + decay). A system requester is
   *  injected; callers pass only `scope` + optional `dryRun` / `since` / `until`. */
  consolidate(request: {
    scope: unknown;
    dryRun?: boolean;
    since?: string;
    until?: string;
  }): Promise<unknown>;
  /** Upsert a knowledge entity. */
  putEntity(entity: unknown): Promise<unknown>;
  /** Upsert a knowledge relationship. */
  putRelationship(relationship: unknown): Promise<unknown>;
  /** Upsert a belief. */
  beliefPut(belief: unknown): Promise<unknown>;
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

  async capabilities(): Promise<unknown> {
    return decode(this.provider.capabilitiesJson());
  }

  async recall(request: RetrievalRequest): Promise<ContextPayload> {
    return decode<ContextPayload>(this.provider.requireRecallApi().recallJson(encode(request)));
  }

  async write(request: WriteMemoryRequest): Promise<WriteMemoryResponse> {
    return decode<WriteMemoryResponse>(this.provider.requireMemoryApi().writeJson(encode(request)));
  }

  async scan(request: { path: string; scope: unknown; scanFilter?: unknown }): Promise<unknown> {
    return decode(this.provider.scanRepositoryJson(encode(request)));
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
}

function encode(value: unknown): string {
  return JSON.stringify(value);
}

function decode<T>(json: string): T {
  return JSON.parse(json) as T;
}
