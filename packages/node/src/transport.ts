import type {
  ContextPayload,
  ForgetRequest,
  ForgetResult,
  RetrievalRequest,
  WriteMemoryRequest,
  WriteMemoryResponse
} from "@engram/contracts";

import {
  loadNativeBinding,
  type NativeBinding,
  type NativeBindingLoader,
  type NativeBeliefEngineBinding,
  type NativeIngestEngineBinding,
  type NativeKnowledgeEngineBinding,
  type NativeMemoryEngineBinding,
  type NativeRetrievalEngineBinding
} from "./binding.js";

/** Transport interface implemented by the native Engram Node package. */
export interface NativeMemoryTransport {
  writeMemory(request: WriteMemoryRequest): Promise<WriteMemoryResponse>;
  retrieve(request: RetrievalRequest): Promise<ContextPayload>;
  forget(request: ForgetRequest): Promise<ForgetResult>;
}

/** Options for constructing a native memory transport. */
export interface NativeMemoryTransportOptions {
  binding?: NativeBinding;
  loader?: NativeBindingLoader;
  dbPath?: string | null;
}

/** Creates a transport that delegates memory behavior to the Rust native engine. */
export function createNativeMemoryTransport(
  options: NativeMemoryTransportOptions = {}
): NativeMemoryTransport {
  const binding = options.binding ?? loadNativeBinding(options.loader);
  return new JsonNativeMemoryTransport(
    new binding.NativeMemoryEngine(options.dbPath ?? null)
  );
}

class JsonNativeMemoryTransport implements NativeMemoryTransport {
  constructor(private readonly engine: NativeMemoryEngineBinding) {}

  async writeMemory(request: WriteMemoryRequest): Promise<WriteMemoryResponse> {
    return decode<WriteMemoryResponse>(this.engine.writeMemoryJson(encode(request)));
  }

  async retrieve(request: RetrievalRequest): Promise<ContextPayload> {
    return decode<ContextPayload>(this.engine.retrieveJson(encode(request)));
  }

  async forget(request: ForgetRequest): Promise<ForgetResult> {
    return decode<ForgetResult>(this.engine.forgetJson(encode(request)));
  }
}

function encode(value: unknown): string {
  return JSON.stringify(value);
}

function decode<T>(json: string): T {
  return JSON.parse(json) as T;
}

/** Transport interface for knowledge graph + taxonomy operations. */
export interface NativeKnowledgeTransport {
  putEntity(entity: unknown): Promise<unknown>;
  putRelationship(relationship: unknown): Promise<unknown>;
  getEntity(id: string, scope: unknown): Promise<unknown>;
  putGraph(graph: unknown): Promise<unknown>;
  getGraph(id: string, scope: unknown): Promise<unknown>;
  neighbors(
    graphId: string,
    nodeId: string,
    scope: unknown,
    limit?: number
  ): Promise<unknown>;
  putConceptScheme(scheme: unknown): Promise<unknown>;
  putConcept(concept: unknown): Promise<unknown>;
  putConceptRelation(relation: unknown): Promise<unknown>;
  listConcepts(schemeId: string, scope: unknown): Promise<unknown>;
  putOntology(ontology: unknown): Promise<unknown>;
  getOntology(id: string, scope: unknown): Promise<unknown>;
  putClass(klass: unknown): Promise<unknown>;
  putProperty(property: unknown): Promise<unknown>;
  putAxiom(axiom: unknown): Promise<unknown>;
  validateGraph(graphId: string, ontologyId: string, scope: unknown): Promise<unknown>;
}

/** Options for constructing a native knowledge transport. */
export interface NativeKnowledgeTransportOptions {
  binding?: NativeBinding;
  loader?: NativeBindingLoader;
  dbPath?: string | null;
}

/** Creates a transport that delegates knowledge + taxonomy behavior to Rust. */
export function createNativeKnowledgeTransport(
  options: NativeKnowledgeTransportOptions = {}
): NativeKnowledgeTransport {
  const binding = options.binding ?? loadNativeBinding(options.loader);
  return new JsonNativeKnowledgeTransport(
    new binding.NativeKnowledgeEngine(options.dbPath ?? null)
  );
}

class JsonNativeKnowledgeTransport implements NativeKnowledgeTransport {
  constructor(private readonly engine: NativeKnowledgeEngineBinding) {}

  async putEntity(entity: unknown): Promise<unknown> {
    return decode(this.engine.putEntityJson(encode(entity)));
  }

  async putRelationship(relationship: unknown): Promise<unknown> {
    return decode(this.engine.putRelationshipJson(encode(relationship)));
  }

  async getEntity(id: string, scope: unknown): Promise<unknown> {
    return decode(this.engine.getEntityJson(encode({ id, scope })));
  }

  async putGraph(graph: unknown): Promise<unknown> {
    return decode(this.engine.putGraphJson(encode(graph)));
  }

  async getGraph(id: string, scope: unknown): Promise<unknown> {
    return decode(this.engine.getGraphJson(encode({ id, scope })));
  }

  async neighbors(
    graphId: string,
    nodeId: string,
    scope: unknown,
    limit?: number
  ): Promise<unknown> {
    return decode(this.engine.neighborsJson(encode({ graphId, nodeId, scope, limit })));
  }

  async putConceptScheme(scheme: unknown): Promise<unknown> {
    return decode(this.engine.putConceptSchemeJson(encode(scheme)));
  }

  async putConcept(concept: unknown): Promise<unknown> {
    return decode(this.engine.putConceptJson(encode(concept)));
  }

  async putConceptRelation(relation: unknown): Promise<unknown> {
    return decode(this.engine.putConceptRelationJson(encode(relation)));
  }

  async listConcepts(schemeId: string, scope: unknown): Promise<unknown> {
    return decode(this.engine.listConceptsJson(encode({ schemeId, scope })));
  }

  async putOntology(ontology: unknown): Promise<unknown> {
    return decode(this.engine.putOntologyJson(encode(ontology)));
  }

  async getOntology(id: string, scope: unknown): Promise<unknown> {
    return decode(this.engine.getOntologyJson(encode({ id, scope })));
  }

  async putClass(klass: unknown): Promise<unknown> {
    return decode(this.engine.putClassJson(encode(klass)));
  }

  async putProperty(property: unknown): Promise<unknown> {
    return decode(this.engine.putPropertyJson(encode(property)));
  }

  async putAxiom(axiom: unknown): Promise<unknown> {
    return decode(this.engine.putAxiomJson(encode(axiom)));
  }

  async validateGraph(graphId: string, ontologyId: string, scope: unknown): Promise<unknown> {
    return decode(this.engine.validateGraphJson(encode({ graphId, ontologyId, scope })));
  }
}

/** Result of an ingest + extract pass over Rust. */
export interface IngestExtractResult {
  graph: unknown;
  entities: unknown[];
  relationships: unknown[];
  chunkCount: number;
}

/** Transport interface for ingest + extract operations. */
export interface NativeIngestTransport {
  ingestExtract(request: unknown): Promise<IngestExtractResult>;
}

/** Options for constructing a native ingest transport. */
export interface NativeIngestTransportOptions {
  binding?: NativeBinding;
  loader?: NativeBindingLoader;
  dbPath?: string | null;
}

/** Creates a transport that delegates ingest + extract behavior to Rust. */
export function createNativeIngestTransport(
  options: NativeIngestTransportOptions = {}
): NativeIngestTransport {
  const binding = options.binding ?? loadNativeBinding(options.loader);
  return new JsonNativeIngestTransport(
    new binding.NativeIngestEngine(options.dbPath ?? null)
  );
}

class JsonNativeIngestTransport implements NativeIngestTransport {
  constructor(private readonly engine: NativeIngestEngineBinding) {}

  async ingestExtract(request: unknown): Promise<IngestExtractResult> {
    return decode(this.engine.ingestExtractJson(encode(request)));
  }
}

/** Transport interface for belief + contradiction operations. */
export interface NativeBeliefTransport {
  putBelief(belief: unknown): Promise<unknown>;
  listBeliefs(scope: unknown): Promise<unknown>;
  putContradiction(contradiction: unknown): Promise<unknown>;
  listContradictions(scope: unknown): Promise<unknown>;
  getContradiction(id: string, scope: unknown): Promise<unknown>;
  resolveContradiction(id: string, scope: unknown, resolution: unknown): Promise<unknown>;
  detectContradictions(beliefs: unknown): Promise<unknown>;
}

/** Options for constructing a native belief transport. */
export interface NativeBeliefTransportOptions {
  binding?: NativeBinding;
  loader?: NativeBindingLoader;
  dbPath?: string | null;
}

/** Creates a transport that delegates belief + contradiction behavior to Rust. */
export function createNativeBeliefTransport(
  options: NativeBeliefTransportOptions = {}
): NativeBeliefTransport {
  const binding = options.binding ?? loadNativeBinding(options.loader);
  return new JsonNativeBeliefTransport(new binding.NativeBeliefEngine(options.dbPath ?? null));
}

class JsonNativeBeliefTransport implements NativeBeliefTransport {
  constructor(private readonly engine: NativeBeliefEngineBinding) {}

  async putBelief(belief: unknown): Promise<unknown> {
    return decode(this.engine.putBeliefJson(encode(belief)));
  }

  async listBeliefs(scope: unknown): Promise<unknown> {
    return decode(this.engine.listBeliefsJson(encode({ scope })));
  }

  async putContradiction(contradiction: unknown): Promise<unknown> {
    return decode(this.engine.putContradictionJson(encode(contradiction)));
  }

  async listContradictions(scope: unknown): Promise<unknown> {
    return decode(this.engine.listContradictionsJson(encode({ scope })));
  }

  async getContradiction(id: string, scope: unknown): Promise<unknown> {
    return decode(this.engine.getContradictionJson(encode({ id, scope })));
  }

  async resolveContradiction(
    id: string,
    scope: unknown,
    resolution: unknown
  ): Promise<unknown> {
    return decode(this.engine.resolveContradictionJson(encode({ id, scope, resolution })));
  }

  async detectContradictions(beliefs: unknown): Promise<unknown> {
    return decode(this.engine.detectContradictionsJson(encode(beliefs)));
  }
}

/** One semantic-search hit returned by Rust. */
export interface RetrievalSearchHit {
  id: string;
  text: string;
  score: number;
}

/** Transport interface for FastEmbed semantic retrieval. */
export interface NativeRetrievalTransport {
  index(text: string): Promise<{ indexed: number }>;
  search(query: string, topK?: number): Promise<RetrievalSearchHit[]>;
}

/** Options for constructing a native retrieval transport. */
export interface NativeRetrievalTransportOptions {
  binding?: NativeBinding;
  loader?: NativeBindingLoader;
}

/** Creates a transport that delegates semantic retrieval to Rust (FastEmbed). */
export function createNativeRetrievalTransport(
  options: NativeRetrievalTransportOptions = {}
): NativeRetrievalTransport {
  const binding = options.binding ?? loadNativeBinding(options.loader);
  return new JsonNativeRetrievalTransport(new binding.NativeRetrievalEngine());
}

class JsonNativeRetrievalTransport implements NativeRetrievalTransport {
  constructor(private readonly engine: NativeRetrievalEngineBinding) {}

  async index(text: string): Promise<{ indexed: number }> {
    return decode(this.engine.indexJson(encode({ text })));
  }

  async search(query: string, topK?: number): Promise<RetrievalSearchHit[]> {
    return decode(this.engine.searchJson(encode({ query, topK })));
  }
}
