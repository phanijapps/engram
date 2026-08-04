import { createRequire } from "node:module";

/** Native class shape exported by the Rust Node-API addon. */
export interface NativeMemoryEngineBinding {
  writeMemoryJson(requestJson: string): string;
  retrieveJson(requestJson: string): string;
  forgetJson(requestJson: string): string;
}

/** Constructor shape for the Rust-backed local memory engine. */
export interface NativeMemoryEngineConstructor {
  new (path?: string | null): NativeMemoryEngineBinding;
}

/** Native class shape for the Rust-backed knowledge + taxonomy engine. */
export interface NativeKnowledgeEngineBinding {
  putEntityJson(entityJson: string): string;
  putRelationshipJson(relationshipJson: string): string;
  getEntityJson(requestJson: string): string;
  putGraphJson(graphJson: string): string;
  getGraphJson(requestJson: string): string;
  neighborsJson(requestJson: string): string;
  putConceptSchemeJson(schemeJson: string): string;
  getConceptSchemeJson(requestJson: string): string;
  putConceptJson(conceptJson: string): string;
  putConceptRelationJson(relationJson: string): string;
  listConceptsJson(requestJson: string): string;
  listGraphsJson(requestJson: string): string;
  listEntitiesJson(requestJson: string): string;
  listEntitiesBySourceJson(requestJson: string): string;
  listRelationshipsJson(requestJson: string): string;
  listRelationshipsBySourceJson(requestJson: string): string;
  listChunksJson(requestJson: string): string;
  listSourcesJson(requestJson: string): string;
  putOntologyJson(ontologyJson: string): string;
  getOntologyJson(requestJson: string): string;
  putClassJson(classJson: string): string;
  putPropertyJson(propertyJson: string): string;
  putAxiomJson(axiomJson: string): string;
  validateGraphJson(requestJson: string): string;
  validateTaxonomyProposalJson(requestJson: string): string;
  graphCandidatesJson(requestJson: string): string;
  associativeGraphCandidatesJson(requestJson: string): string;
  fuseRrfJson(requestJson: string): string;
  fuseRrfIdsJson(requestJson: string): string;
}

/** Constructor shape for the Rust-backed knowledge + taxonomy engine. */
export interface NativeKnowledgeEngineConstructor {
  new (path?: string | null): NativeKnowledgeEngineBinding;
}

/** Native class shape for the Rust-backed ingest + extract engine. */
export interface NativeIngestEngineBinding {
  ingestExtractJson(requestJson: string): string;
  startScanJobJson(requestJson: string): string;
  getScanJobJson(requestJson: string): string;
}

/** Constructor shape for the Rust-backed ingest + extract engine. */
export interface NativeIngestEngineConstructor {
  new (path?: string | null): NativeIngestEngineBinding;
}

/** Native class shape for the Rust-backed belief + contradiction engine. */
export interface NativeBeliefEngineBinding {
  putBeliefJson(beliefJson: string): string;
  listBeliefsJson(requestJson: string): string;
  putContradictionJson(contradictionJson: string): string;
  listContradictionsJson(requestJson: string): string;
  getContradictionJson(requestJson: string): string;
  resolveContradictionJson(requestJson: string): string;
  detectContradictionsJson(beliefsJson: string): string;
}

/** Constructor shape for the Rust-backed belief + contradiction engine. */
export interface NativeBeliefEngineConstructor {
  new (path?: string | null): NativeBeliefEngineBinding;
}

/** Native class shape for Rust-backed hierarchy validation. */
export interface NativeHierarchyEngineBinding {
  validateParentageJson(nodesJson: string): string;
}

/** Constructor shape for Rust-backed hierarchy validation. */
export interface NativeHierarchyEngineConstructor {
  new (): NativeHierarchyEngineBinding;
}

/** Native class shape for Rust-backed consolidation planning. */
export interface NativeConsolidationEngineBinding {
  planJson(requestJson: string): string;
}

/** Constructor shape for Rust-backed consolidation planning. */
export interface NativeConsolidationEngineConstructor {
  new (): NativeConsolidationEngineBinding;
}

/** Native class shape for Rust-backed evaluation coverage summaries. */
export interface NativeEvalEngineBinding {
  architectureCoverageJson(casesJson: string): string;
}

/** Constructor shape for Rust-backed evaluation coverage summaries. */
export interface NativeEvalEngineConstructor {
  new (): NativeEvalEngineBinding;
}

/** Native class shape for the Rust-backed semantic-retrieval engine (FastEmbed). */
export interface NativeRetrievalEngineBinding {
  indexJson(requestJson: string): string;
  searchJson(requestJson: string): string;
  indexChunkJson(requestJson: string): string;
  cacheStatsJson(): string;
  clearJson(): string;
}

/** Constructor shape for the Rust-backed semantic-retrieval engine. */
export interface NativeRetrievalEngineConstructor {
  new (path?: string | null): NativeRetrievalEngineBinding;
}

// ---------------------------------------------------------------------------
// NativeProvider — held EngramProvider surface (RFC-0017 Phase A)
// ---------------------------------------------------------------------------

/** Memory API handle proxy: retrieve / write / forget. */
export interface NativeMemoryApiBinding {
  searchJson(requestJson: string): string;
  writeJson(requestJson: string): string;
  forgetJson(requestJson: string): string;
  listMemoriesPagedJson(requestJson: string): string;
}

/** Unified-recall API handle proxy: one query fused across lanes. */
export interface NativeRecallApiBinding {
  recallJson(requestJson: string): string;
}

/** Graph API handle proxy: entity/relationship reads/writes + neighbors. */
export interface NativeGraphApiBinding {
  getEntityJson(requestJson: string): string;
  putEntityJson(entityJson: string): string;
  putRelationshipJson(relationshipJson: string): string;
  neighborsJson(requestJson: string): string;
}

/** Batch-ingest API handle proxy: best-effort batch write + guarantee. */
export interface NativeBatchApiBinding {
  ingestJson(requestJson: string): string;
  transactionGuarantee(): string;
}

/** Beliefs API handle proxy: belief lifecycle. */
export interface NativeBeliefsApiBinding {
  getBeliefJson(requestJson: string): string;
  upsertBeliefJson(beliefJson: string): string;
  retractBeliefJson(requestJson: string): string;
  listStaleBeliefsJson(scopeJson: string): string;
}

/** Observability / diagnostics handle proxy: point-in-time snapshot
 *  (capability report, record counts, embedding config, versions). */
export interface NativeObservabilityApiBinding {
  diagnosticsJson(): string;
}

/** Community-query handle proxy: Louvain overview + member index + community-of. */
export interface NativeCommunityQueryApiBinding {
  overviewJson(requestJson: string): string;
  memberIndexJson(scopeJson: string): string;
  communityOfJson(requestJson: string): string;
}

/**
 * Native class shape for the held `EngramProvider`
 * (`bindings/node/src/provider.rs`): one provider opened from a config, reaching
 * every wired capability through typed handle proxies plus the direct
 * `consolidateJson` / `scanRepositoryJson` methods. The remaining `require*Api`
 * proxies (provenance, hierarchy, ontology, taxonomy, procedures, lexical_feed,
 * knowledge_query, retrieval, vectors, migration, embedding_provider,
 * export_import, observability) are typed on demand when a module needs them.
 */
export interface NativeProviderBinding {
  capabilitiesJson(): string;
  consolidateJson(requestJson: string): string;
  scanRepositoryJson(requestJson: string): string;
  requireMemoryApi(): NativeMemoryApiBinding;
  requireRecallApi(): NativeRecallApiBinding;
  requireGraphApi(): NativeGraphApiBinding;
  requireBatchApi(): NativeBatchApiBinding;
  requireBeliefsApi(): NativeBeliefsApiBinding;
  requireObservabilityApi(): NativeObservabilityApiBinding;
  requireCommunityQueryApi(): NativeCommunityQueryApiBinding;
}

/** Constructor shape for the held `EngramProvider`. */
export interface NativeProviderConstructor {
  new (configJson: string): NativeProviderBinding;
  fromProfileFile(path: string): NativeProviderBinding;
}

/** Native addon surface consumed by `@engram/node`. */
export interface NativeBinding {
  NativeProvider: NativeProviderConstructor;
  NativeMemoryEngine: NativeMemoryEngineConstructor;
  NativeKnowledgeEngine: NativeKnowledgeEngineConstructor;
  NativeIngestEngine: NativeIngestEngineConstructor;
  NativeBeliefEngine: NativeBeliefEngineConstructor;
  NativeHierarchyEngine: NativeHierarchyEngineConstructor;
  NativeConsolidationEngine: NativeConsolidationEngineConstructor;
  NativeEvalEngine: NativeEvalEngineConstructor;
  NativeRetrievalEngine: NativeRetrievalEngineConstructor;
}

/** Function used to load a native addon, injectable for deterministic tests. */
export type NativeBindingLoader = () => NativeBinding;

/** Loads the compiled Engram Node-API addon from known package locations. */
export function loadNativeBinding(loader: NativeBindingLoader = loadCompiledBinding): NativeBinding {
  return loader();
}

function loadCompiledBinding(): NativeBinding {
  const require = createRequire(import.meta.url);
  const candidates = [
    "../engram_node.node",
    "../engram-node.node",
    "../index.node",
    "../../engram_node.node",
    "../../engram-node.node"
  ];

  for (const candidate of candidates) {
    try {
      return require(candidate) as NativeBinding;
    } catch (error) {
      if (!isModuleNotFound(error)) {
        throw error;
      }
    }
  }

  throw new Error(
    "Unable to load @engram/node native addon. Build bindings/node and place the .node artifact in the package root."
  );
}

function isModuleNotFound(error: unknown): boolean {
  return (
    error instanceof Error &&
    "code" in error &&
    (error as NodeJS.ErrnoException).code === "MODULE_NOT_FOUND"
  );
}
