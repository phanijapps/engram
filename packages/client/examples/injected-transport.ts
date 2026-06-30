import type {
  ContextPayload,
  ForgetRequest,
  ForgetResult,
  MemoryEvent,
  MemoryRecord,
  RetrievalRequest,
  WriteMemoryRequest,
  WriteMemoryResponse
} from "@engram/contracts";

import { createEngramClient, type EngramTransport } from "../src/index.js";

const observedAt = "2026-06-29T12:00:00Z";

const writeRequest: WriteMemoryRequest = {
  kind: "fact",
  content: {
    text: "engram v1 uses Rust 2024 for deterministic memory behavior.",
    summary: "Rust core with TypeScript bindings",
    language: "en",
    format: "text"
  },
  scope: {
    tenant: "tenant-demo",
    workspace: "engram",
    environment: "local"
  },
  requester: {
    actor: {
      id: "actor-agent-1",
      kind: "agent",
      displayName: "Contract Agent"
    },
    roles: ["maintainer"],
    permissions: ["memory.write"]
  },
  provenance: {
    source: "typescript_injected_transport_example",
    actor: {
      id: "actor-agent-1",
      kind: "agent",
      displayName: "Contract Agent"
    },
    observedAt,
    confidence: 1,
    method: "manual"
  },
  policy: {
    visibility: "workspace",
    retention: "durable",
    sensitivity: "low",
    allowedUses: ["retrieval", "evaluation", "debugging"],
    deleteMode: "tombstone"
  },
  idempotencyKey: "typescript-example-001"
};

const retrievalRequest: RetrievalRequest = {
  query: "What implementation stack does engram v1 use?",
  scope: writeRequest.scope,
  requester: {
    actor: writeRequest.requester.actor,
    roles: ["maintainer"],
    permissions: ["memory.retrieve"]
  },
  modes: ["keyword"],
  filters: {
    memoryKinds: ["fact"],
    includeArchived: false
  },
  limit: 5,
  budget: {
    maxItems: 3,
    maxTokens: 1200
  },
  includeExplanations: true
};

class ExampleTransport implements EngramTransport {
  private record: MemoryRecord | undefined;

  async writeMemory(request: WriteMemoryRequest): Promise<WriteMemoryResponse> {
    const record: MemoryRecord = {
      id: "memory-example-1",
      kind: request.kind,
      content: request.content,
      scope: request.scope,
      provenance: request.provenance,
      policy: request.policy,
      status: "active",
      createdAt: observedAt,
      ...(request.links ? { links: request.links } : {})
    };
    const event: MemoryEvent = {
      id: "event-example-1",
      kind: "written",
      memoryId: record.id,
      scope: record.scope,
      actor: request.requester.actor,
      provenance: request.provenance,
      payload: { idempotencyKey: request.idempotencyKey },
      occurredAt: observedAt,
      recordedAt: observedAt
    };
    this.record = record;
    return { record, event };
  }

  async retrieve(request: RetrievalRequest): Promise<ContextPayload> {
    if (!this.record) {
      return { items: [], createdAt: observedAt };
    }
    const explanation = request.includeExplanations
      ? {
          reason: "example transport returned the memory written earlier",
          matchedTerms: ["engram", "Rust"]
        }
      : undefined;

    return {
      createdAt: observedAt,
      ...(request.budget ? { budget: request.budget } : {}),
      items: [
        {
          id: "result-example-1",
          targetId: this.record.id,
          targetType: "memory",
          content: this.record.content.text,
          policy: this.record.policy,
          provenance: this.record.provenance,
          score: { total: 1, relevance: 1, confidence: 1 },
          ...(explanation ? { explanation } : {})
        }
      ]
    };
  }

  async forget(request: ForgetRequest): Promise<ForgetResult> {
    this.record = undefined;
    return {
      targetType: request.targetType,
      targetId: request.targetId,
      status: "tombstoned"
    };
  }
}

export async function runInjectedTransportExample(): Promise<ContextPayload> {
  const client = createEngramClient({ transport: new ExampleTransport() });
  const write = await client.writeMemory(writeRequest);
  const context = await client.retrieve(retrievalRequest);
  await client.forget({
    targetType: "memory",
    targetId: write.record.id,
    scope: write.record.scope,
    requester: {
      actor: writeRequest.requester.actor,
      roles: ["maintainer"],
      permissions: ["memory.forget"]
    },
    mode: "tombstone",
    reason: "example cleanup"
  });
  return context;
}
