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
  putOntologyJson(ontologyJson: string): string;
  getOntologyJson(requestJson: string): string;
  putClassJson(classJson: string): string;
  putPropertyJson(propertyJson: string): string;
  putAxiomJson(axiomJson: string): string;
  validateGraphJson(requestJson: string): string;
}

/** Constructor shape for the Rust-backed knowledge + taxonomy engine. */
export interface NativeKnowledgeEngineConstructor {
  new (path?: string | null): NativeKnowledgeEngineBinding;
}

/** Native class shape for the Rust-backed ingest + extract engine. */
export interface NativeIngestEngineBinding {
  ingestExtractJson(requestJson: string): string;
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

/** Native class shape for the Rust-backed semantic-retrieval engine (FastEmbed). */
export interface NativeRetrievalEngineBinding {
  indexJson(requestJson: string): string;
  searchJson(requestJson: string): string;
}

/** Constructor shape for the Rust-backed semantic-retrieval engine. */
export interface NativeRetrievalEngineConstructor {
  new (): NativeRetrievalEngineBinding;
}

/** Native addon surface consumed by `@engram/node`. */
export interface NativeBinding {
  NativeMemoryEngine: NativeMemoryEngineConstructor;
  NativeKnowledgeEngine: NativeKnowledgeEngineConstructor;
  NativeIngestEngine: NativeIngestEngineConstructor;
  NativeBeliefEngine: NativeBeliefEngineConstructor;
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
