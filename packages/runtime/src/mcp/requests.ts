import type {
  Actor,
  MemoryContent,
  MemoryKind,
  Policy,
  Provenance,
  Requester,
  RetrievalRequest,
  Scope,
  WriteMemoryRequest
} from "@engram/contracts";

/** The system actor the HTTP-MCP server attributes to agent-driven writes/reads. */
export function defaultActor(): Actor {
  return { id: "engram-mcp-http", kind: "agent" };
}

/** A default requester for tools that drive retrieval/write (required by the
 *  contracts, but agent-identity is not surfaced over MCP v1 — loopback only). */
export function defaultRequester(): Requester {
  return { actor: defaultActor() };
}

function defaultPolicy(): Policy {
  return { visibility: "workspace", retention: "durable" };
}

function defaultProvenance(): Provenance {
  return {
    actor: defaultActor(),
    observedAt: new Date().toISOString(),
    source: "engram-mcp-http"
  };
}

/** Build a `RetrievalRequest` from the few fields an MCP caller provides. */
export function buildRetrievalRequest(query: string, scope: Scope, limit?: number): RetrievalRequest {
  return {
    query,
    scope,
    requester: defaultRequester(),
    ...(limit !== undefined ? { limit } : {}),
  } as RetrievalRequest;
}

/** Build a `WriteMemoryRequest` from a memory's text + scope. */
export function buildWriteMemoryRequest(opts: {
  text: string;
  scope: Scope;
  kind?: MemoryKind;
}): WriteMemoryRequest {
  const content: MemoryContent = { text: opts.text };
  return {
    content,
    kind: opts.kind ?? "observation",
    scope: opts.scope,
    requester: defaultRequester(),
    policy: defaultPolicy(),
    provenance: defaultProvenance()
  };
}
