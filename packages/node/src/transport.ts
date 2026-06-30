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
  type NativeKnowledgeEngineBinding,
  type NativeMemoryEngineBinding
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
}

/** Creates a transport that delegates memory behavior to the Rust native engine. */
export function createNativeMemoryTransport(
  options: NativeMemoryTransportOptions = {}
): NativeMemoryTransport {
  const binding = options.binding ?? loadNativeBinding(options.loader);
  return new JsonNativeMemoryTransport(new binding.NativeMemoryEngine());
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
}

/** Options for constructing a native knowledge transport. */
export interface NativeKnowledgeTransportOptions {
  binding?: NativeBinding;
  loader?: NativeBindingLoader;
}

/** Creates a transport that delegates knowledge + taxonomy behavior to Rust. */
export function createNativeKnowledgeTransport(
  options: NativeKnowledgeTransportOptions = {}
): NativeKnowledgeTransport {
  const binding = options.binding ?? loadNativeBinding(options.loader);
  return new JsonNativeKnowledgeTransport(new binding.NativeKnowledgeEngine());
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
}
