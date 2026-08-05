//! LLM maintenance ops — reflectLlm + contradictLlm. Stub transport + stub LLM
//! (completeOverride) — no native binding, no tokens. Asserts the ops read the
//! right facade surface, drive the LLM, and write records with the correct
//! `reflection-llm` / `contradiction-llm` provenance.

import { describe, it, expect, vi } from "vitest";
import type { NativeProviderTransport } from "@engram/node";

import { createLlmProvider } from "../src/maintenance/llm.js";
import { reflectLlm } from "../src/maintenance/reflect.js";
import { contradictLlm } from "../src/maintenance/contradict.js";

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

const stubLlm = (toolCalls: Array<{ name: string; arguments: Record<string, unknown> }>) =>
  createLlmProvider({
    provider: "stub",
    model: "stub",
    completeOverride: async () => ({ toolCalls, text: "" }),
  });

describe("reflectLlm", () => {
  it("reads memories, drives the LLM, writes a reflection-llm belief", async () => {
    const beliefPut = vi.fn(async (b: unknown) => b);
    const t = mockTransport({
      listMemoriesPaged: vi.fn(async () => ({
        items: [
          { id: "m1", text: "the cat naps by the radiator" },
          { id: "m2", text: "cats seek out warm spots" },
        ],
        nextCursor: null,
      })),
      beliefPut,
    });

    const res = await reflectLlm({
      transport: t,
      scope: { tenant: "t", workspace: "w" },
      llm: stubLlm([
        {
          name: "record_belief",
          arguments: { subjectKey: "cat", content: "Cats seek warmth.", confidence: 0.8, reasoning: "both memories" },
        },
      ]),
    });

    expect(res.memoriesRead).toBe(2);
    expect(res.beliefsWritten).toBe(1);
    expect(beliefPut).toHaveBeenCalledTimes(1);
    const belief = beliefPut.mock.calls[0]![0] as {
      subject: { key: string };
      status: string;
      provenance: { method: string; source: string };
    };
    expect(belief.subject.key).toBe("cat");
    expect(belief.status).toBe("active");
    expect(belief.provenance.method).toBe("reflection-llm");
    expect(belief.provenance.source).toBe("pi-mono");
  });

  it("writes nothing when there are no memories", async () => {
    const beliefPut = vi.fn(async () => ({}));
    const t = mockTransport({ beliefPut });
    const res = await reflectLlm({ transport: t, scope: { tenant: "t" }, llm: stubLlm([]) });
    expect(res).toEqual({ memoriesRead: 0, beliefsWritten: 0, skipped: 0 });
    expect(beliefPut).not.toHaveBeenCalled();
  });

  it("skips tool calls missing required fields or with the wrong name", async () => {
    const beliefPut = vi.fn(async () => ({}));
    const t = mockTransport({
      listMemoriesPaged: vi.fn(async () => ({ items: [{ id: "m1", text: "x" }], nextCursor: null })),
      beliefPut,
    });
    const res = await reflectLlm({
      transport: t,
      scope: { tenant: "t" },
      llm: stubLlm([
        { name: "record_belief", arguments: { content: "no subject" } }, // missing subjectKey
        { name: "unrelated_tool", arguments: {} },
      ]),
    });
    expect(res.beliefsWritten).toBe(0);
    expect(res.skipped).toBe(2);
    expect(beliefPut).not.toHaveBeenCalled();
  });
});

describe("contradictLlm", () => {
  it("reads beliefs, drives the LLM, writes a contradiction-llm record over the two targets", async () => {
    const putContradiction = vi.fn(async (c: unknown) => c);
    const t = mockTransport({
      listBeliefsPaged: vi.fn(async () => ({
        items: [
          { id: "b1", content: "X is fast", subject: { key: "x" }, confidence: 0.9 },
          { id: "b2", content: "X is slow", subject: { key: "y" }, confidence: 0.9 },
        ],
        nextCursor: null,
      })),
      putContradiction,
    });

    const res = await contradictLlm({
      transport: t,
      scope: { tenant: "t", workspace: "w" },
      llm: stubLlm([
        {
          name: "find_contradiction",
          arguments: { beliefIds: ["b1", "b2"], kind: "tension", severity: 0.7, reasoning: "fast vs slow" },
        },
      ]),
    });

    expect(res.beliefsRead).toBe(2);
    expect(res.contradictionsWritten).toBe(1);
    expect(putContradiction).toHaveBeenCalledTimes(1);
    const c = putContradiction.mock.calls[0]![0] as {
      kind: string;
      status: string;
      targets: Array<{ targetType: string; targetId: string }>;
      provenance: { method: string };
    };
    expect(c.kind).toBe("tension");
    expect(c.status).toBe("open");
    expect(c.targets).toHaveLength(2);
    expect(c.targets.map((x) => x.targetId).sort()).toEqual(["b1", "b2"]);
    expect(c.targets.every((x) => x.targetType === "belief")).toBe(true);
    expect(c.provenance.method).toBe("contradiction-llm");
  });

  it("writes nothing with fewer than 2 beliefs", async () => {
    const putContradiction = vi.fn(async () => ({}));
    const t = mockTransport({
      listBeliefsPaged: vi.fn(async () => ({ items: [{ id: "b1", content: "only one" }], nextCursor: null })),
      putContradiction,
    });
    const res = await contradictLlm({ transport: t, scope: { tenant: "t" }, llm: stubLlm([]) });
    expect(res.contradictionsWritten).toBe(0);
    expect(putContradiction).not.toHaveBeenCalled();
  });

  it("skips contradictions that don't supply two ids + reasoning", async () => {
    const putContradiction = vi.fn(async () => ({}));
    const t = mockTransport({
      listBeliefsPaged: vi.fn(async () => ({
        items: [
          { id: "b1", content: "a" },
          { id: "b2", content: "b" },
        ],
        nextCursor: null,
      })),
      putContradiction,
    });
    const res = await contradictLlm({
      transport: t,
      scope: { tenant: "t" },
      llm: stubLlm([
        { name: "find_contradiction", arguments: { beliefIds: ["b1"] } }, // only one id, no reasoning
      ]),
    });
    expect(res.contradictionsWritten).toBe(0);
    expect(res.skipped).toBe(1);
  });
});
