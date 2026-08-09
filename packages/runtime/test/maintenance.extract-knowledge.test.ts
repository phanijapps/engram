//! LLM `extract-knowledge` op — stub transport + stub LLM (completeOverride).
//! No native binding, no tokens. Asserts the op iterates per-document graphs,
//! drives the LLM once per document, filters noise concepts (ported
//! `is_noise_concept` + doc-heading blocklist), writes Concept entities + typed
//! edges with deterministic scope+label-keyed ids (graph_id = None → RFC-0014
//! cross-document consolidation), and is idempotent on re-run.

import { describe, it, expect, vi } from "vitest";
import type { NativeProviderTransport } from "@engram/node";

import { createLlmProvider } from "../src/maintenance/llm.js";
import { extractKnowledge, isNoiseConcept } from "../src/maintenance/extract_knowledge.js";

function mockTransport(overrides: Partial<NativeProviderTransport> = {}): NativeProviderTransport {
  return {
    capabilities: vi.fn(async () => ({})),
    recall: vi.fn(async () => ({})),
    write: vi.fn(async () => ({})),
    scan: vi.fn(async () => ({})),
    consolidate: vi.fn(async () => ({})),
    putEntity: vi.fn(async () => ({})),
    putRelationship: vi.fn(async () => ({})),
    beliefPut: vi.fn(async () => ({})),
    forget: vi.fn(async () => ({})),
    batchIngest: vi.fn(async () => ({})),
    listMemoriesPaged: vi.fn(async () => ({ items: [], nextCursor: null })),
    diagnostics: vi.fn(async () => ({ record_counts: {} })),
    communityOverview: vi.fn(async () => ({})),
    communityMemberIndex: vi.fn(async () => ({})),
    scopeCounts: vi.fn(async () => ({})),
    listBeliefs: vi.fn(async () => []),
    listBeliefsPaged: vi.fn(async () => ({ items: [], nextCursor: null })),
    listContradictions: vi.fn(async () => []),
    putContradiction: vi.fn(async () => ({})),
    ...overrides,
  } as unknown as NativeProviderTransport;
}

/** Dispatches a different fixture LLM response per document based on the
 *  `[document: <id>]` prefix the op puts in the userText. */
function perDocLlm(responses: Record<string, ReadonlyArray<{ name: string; arguments: Record<string, unknown> }>>) {
  return createLlmProvider({
    provider: "stub",
    model: "stub",
    completeOverride: async (opts) => {
      const m = /\[document:\s*([^\]]+)\]/[Symbol.match](opts.userText);
      const docId = m?.[1]?.trim();
      const toolCalls = docId !== undefined ? responses[docId] : undefined;
      return { toolCalls: toolCalls ? [...toolCalls] : [], text: "" };
    },
  });
}

describe("isNoiseConcept (ported from Rust + doc-heading blocklist)", () => {
  it("rejects short, punctuation-only, and type/system noise", () => {
    for (const n of ["x", "ab", "...", "str", "string", "fn", "self", "u32", "value", "name"]) {
      expect(isNoiseConcept(n), `${n} should be noise`).toBe(true);
    }
  });
  it("rejects doc-heading generics (case-insensitive)", () => {
    for (const n of ["Architecture", "overview", "Introduction", "background", "Summary", "conclusion", "references"]) {
      expect(isNoiseConcept(n), `${n} should be noise`).toBe(true);
    }
  });
  it("rejects key:value patterns and non-alpha-leading names", () => {
    expect(isNoiseConcept("type: string")).toBe(true);
    expect(isNoiseConcept("123abc")).toBe(true);
    expect(isNoiseConcept("---")).toBe(true);
  });
  it("keeps real concept labels", () => {
    for (const n of ["Memory", "Retrieval", "Vector Index", "Consolidation", "Belief Synthesis"]) {
      expect(isNoiseConcept(n), `${n} should NOT be noise`).toBe(false);
    }
  });
});

describe("extractKnowledge", () => {
  it("writes exactly 3 Concept entities + 4 typed edges over a two-document fixture", async () => {
    const putEntity = vi.fn(async (e: unknown) => e);
    const putRelationship = vi.fn(async (r: unknown) => r);
    const listGraphs = vi.fn(async () => [
      { id: "graph-docA", metadata: { documentId: "docA" } },
      { id: "graph-docB", metadata: { documentId: "docB" } },
    ]);
    const listChunksByDocument = vi.fn(async (documentId: string) => {
      if (documentId === "docA") {
        return [{ text: "Memory systems use a Vector Index for fast retrieval. Retrieval depends on Memory." }];
      }
      if (documentId === "docB") {
        return [{ text: "The Vector Index is consulted by Memory. Memory relates to the Vector Index." }];
      }
      return [];
    });

    const t = mockTransport({ listGraphs, listChunksByDocument, putEntity, putRelationship });

    // Doc A: 4 emitted concepts (Introduction/overview filtered → memory, retrieval valid);
    //        1 property + 1 relationship.
    // Doc B: 3 emitted concepts (Architecture filtered → memory [converges], vector index valid);
    //        2 relationships.
    const llm = perDocLlm({
      docA: [
        {
          name: "record_extraction",
          arguments: {
            concepts: ["Memory", "Retrieval", "Introduction", "overview"],
            properties: [{ subject: "Memory", predicate: "has_property", value: "durable" }],
            relationships: [{ subject: "Retrieval", predicate: "depends_on", object: "Memory" }],
          },
        },
      ],
      docB: [
        {
          name: "record_extraction",
          arguments: {
            concepts: ["Memory", "Vector Index", "Architecture"],
            properties: [],
            relationships: [
              { subject: "Memory", predicate: "relates_to", object: "Vector Index" },
              { subject: "Vector Index", predicate: "depends_on", object: "Memory" },
            ],
          },
        },
      ],
    });

    const res = await extractKnowledge({
      transport: t,
      scope: { tenant: "t", workspace: "w" },
      llm,
    });

    // 2 documents processed.
    expect(res.documentsRead).toBe(2);
    // Noise concepts (Introduction, overview, Architecture) skipped.
    expect(res.skipped).toBe(3);

    const entityIds = new Set(putEntity.mock.calls.map((c) => (c[0] as { id: string }).id));
    const relIds = new Set(putRelationship.mock.calls.map((c) => (c[0] as { id: string }).id));
    expect(entityIds.size).toBe(3); // memory, retrieval, vector index
    expect(relIds.size).toBe(4); // memory→has_property→durable + retrieval→depends_on→memory + memory→relates_to→vector_index + vector_index→depends_on→memory

    // Every written entity is a Concept, not file-scoped, scope+label-keyed.
    for (const call of putEntity.mock.calls) {
      const e = call[0] as { id: string; kind: string; name: string; provenance: { method: string; source: string }; metadata?: { extractionMethod?: string } };
      expect(e.kind).toBe("concept");
      expect(e.id.startsWith("concept-")).toBe(true);
      expect(e.provenance.method).toBe("extraction-llm");
      expect(e.provenance.source).toBe("pi-mono");
      expect(e.metadata?.extractionMethod).toBe("llm");
      expect("graphId" in e).toBe(false); // graph_id omitted (None) — RFC-0014
    }

    // Every written relationship has a typed predicate and graph_id omitted.
    for (const call of putRelationship.mock.calls) {
      const r = call[0] as { id: string; predicate: string; subject: { id?: string }; object: { id?: string; kind?: string }; provenance: { method: string } };
      expect(["has_property", "depends_on", "relates_to"]).toContain(r.predicate);
      expect(r.id.startsWith("concept-rel-")).toBe(true);
      expect("graphId" in r).toBe(false);
      expect(r.provenance.method).toBe("extraction-llm");
    }
  });

  it("converges the same concept label across two documents to ONE entity id", async () => {
    const putEntity = vi.fn(async (e: unknown) => e);
    const listGraphs = vi.fn(async () => [
      { id: "graph-docA", metadata: { documentId: "docA" } },
      { id: "graph-docB", metadata: { documentId: "docB" } },
    ]);
    const listChunksByDocument = vi.fn(async (documentId: string) => {
      if (documentId === "docA") return [{ text: "docA text mentioning Memory." }];
      if (documentId === "docB") return [{ text: "docB text mentioning memory." }];
      return [];
    });

    const t = mockTransport({ listGraphs, listChunksByDocument, putEntity, putRelationship: vi.fn(async () => ({})) });

    const llm = perDocLlm({
      // Doc A emits "Memory" (capitalized).
      docA: [{ name: "record_extraction", arguments: { concepts: ["Memory"], properties: [], relationships: [] } }],
      // Doc B emits "memory" (lowercase) — different surface form, same canonical label.
      docB: [{ name: "record_extraction", arguments: { concepts: ["memory"], properties: [], relationships: [] } }],
    });

    await extractKnowledge({ transport: t, scope: { tenant: "t" }, llm });

    const ids = putEntity.mock.calls.map((c) => (c[0] as { id: string }).id);
    expect(ids.length).toBe(2); // two putEntity calls (one per doc)
    expect(ids[0]).toBe(ids[1]); // same id — RFC-0014 consolidation
  });

  it("is idempotent — a second run writes no new entity or edge ids", async () => {
    const putEntity = vi.fn(async (e: unknown) => e);
    const putRelationship = vi.fn(async (r: unknown) => r);
    const listGraphs = vi.fn(async () => [
      { id: "graph-docA", metadata: { documentId: "docA" } },
      { id: "graph-docB", metadata: { documentId: "docB" } },
    ]);
    const listChunksByDocument = vi.fn(async (documentId: string) => {
      if (documentId === "docA") return [{ text: "docA" }];
      if (documentId === "docB") return [{ text: "docB" }];
      return [];
    });

    const responses = {
      docA: [
        {
          name: "record_extraction",
          arguments: {
            concepts: ["Memory", "Retrieval", "Introduction"],
            properties: [{ subject: "Memory", predicate: "has_property", value: "durable" }],
            relationships: [{ subject: "Retrieval", predicate: "depends_on", object: "Memory" }],
          },
        },
      ],
      docB: [
        {
          name: "record_extraction",
          arguments: {
            concepts: ["Memory", "Vector Index"],
            properties: [],
            relationships: [{ subject: "Memory", predicate: "relates_to", object: "Vector Index" }],
          },
        },
      ],
    } as const;

    const runOnce = async () => {
      const t = mockTransport({
        listGraphs,
        listChunksByDocument,
        putEntity,
        putRelationship,
      });
      return extractKnowledge({
        transport: t,
        scope: { tenant: "t", workspace: "w" },
        llm: perDocLlm(responses),
      });
    };

    await runOnce();
    const afterRun1Entities = new Set(putEntity.mock.calls.map((c) => (c[0] as { id: string }).id));
    const afterRun1Rels = new Set(putRelationship.mock.calls.map((c) => (c[0] as { id: string }).id));

    await runOnce();
    const afterRun2Entities = new Set(putEntity.mock.calls.map((c) => (c[0] as { id: string }).id));
    const afterRun2Rels = new Set(putRelationship.mock.calls.map((c) => (c[0] as { id: string }).id));

    // No new ids appeared on the second run (deterministic ids → upserts).
    expect(afterRun2Entities.size).toBe(afterRun1Entities.size);
    expect(afterRun2Rels.size).toBe(afterRun1Rels.size);
    for (const id of afterRun2Entities) expect(afterRun1Entities.has(id)).toBe(true);
    for (const id of afterRun2Rels) expect(afterRun1Rels.has(id)).toBe(true);
  });

  it("skips graphs lacking a documentId and writes nothing for empty docs", async () => {
    const putEntity = vi.fn(async () => ({}));
    const putRelationship = vi.fn(async () => ({}));
    const listGraphs = vi.fn(async () => [
      { id: "graph-noDoc", metadata: {} }, // no documentId → skipped
      { id: "graph-docEmpty", metadata: { documentId: "docEmpty" } }, // empty chunks → skipped
    ]);
    const listChunksByDocument = vi.fn(async () => []);
    const t = mockTransport({ listGraphs, listChunksByDocument, putEntity, putRelationship });
    const llm = perDocLlm({});

    const res = await extractKnowledge({ transport: t, scope: { tenant: "t" }, llm });

    expect(res.documentsRead).toBe(0);
    expect(res.entitiesWritten).toBe(0);
    expect(res.relationshipsWritten).toBe(0);
    expect(putEntity).not.toHaveBeenCalled();
    expect(putRelationship).not.toHaveBeenCalled();
  });

  it("skips properties/relationships whose endpoints aren't in the concept set", async () => {
    const putEntity = vi.fn(async (e: unknown) => e);
    const putRelationship = vi.fn(async (r: unknown) => r);
    const listGraphs = vi.fn(async () => [{ id: "g1", metadata: { documentId: "d1" } }]);
    const listChunksByDocument = vi.fn(async () => [{ text: "x" }]);
    const t = mockTransport({ listGraphs, listChunksByDocument, putEntity, putRelationship });

    const llm = perDocLlm({
      d1: [
        {
          name: "record_extraction",
          arguments: {
            concepts: ["Memory"],
            properties: [{ subject: "Unknown", predicate: "has_property", value: "x" }], // subject not in concepts
            relationships: [{ subject: "Memory", predicate: "relates_to", object: "Ghost" }], // object not in concepts
          },
        },
        {
          name: "record_extraction",
          arguments: {
            concepts: [],
            properties: [],
            relationships: [{ subject: "A", predicate: "bogus_predicate", object: "B" }], // unknown tool call shape
          },
        },
        { name: "unrelated_tool", arguments: {} },
      ],
    });

    const res = await extractKnowledge({ transport: t, scope: { tenant: "t" }, llm });

    expect(res.entitiesWritten).toBe(1); // just Memory
    expect(res.relationshipsWritten).toBe(0);
    expect(res.skipped).toBe(4); // 1 property + 1 relationship + 1 unknown-predicate rel + 1 unrelated tool call
  });
});
