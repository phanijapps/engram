import { describe, expect, it } from "vitest";
import { buildEvidence, type MemoryItem, type QaBelief } from "./qa.js";

const memory = (id: string, text: string): MemoryItem => ({
  targetId: id,
  content: { text },
  provenance: { source: "demo" },
});

const belief = (id: string, key: string, content: string): QaBelief => ({
  id,
  subject: { key },
  content,
});

describe("buildEvidence", () => {
  it("keeps memories and query-matched beliefs, with sources", () => {
    const { context, sources } = buildEvidence(
      "is svc-a up?",
      [memory("mem-1", "svc-a restarted at noon"), memory("mem-2", "unrelated note")],
      [
        belief("b1", "svc-a", "svc-a is up"),
        belief("b2", "billing", "invoices are late"), // no query-term overlap → dropped
      ]
    );
    expect(sources).toEqual([
      { kind: "memory", id: "mem-1", text: "svc-a restarted at noon", source: "demo" },
      { kind: "memory", id: "mem-2", text: "unrelated note", source: "demo" },
      { kind: "belief", id: "b1", text: "svc-a is up", source: "svc-a" },
    ]);
    expect(context).toContain("[memory mem-1]");
    expect(context).toContain("[belief b1]");
    expect(context).not.toContain("invoices"); // b2 filtered out
  });

  it("reports no-relevant-records when nothing matches", () => {
    const { context, sources } = buildEvidence("zzz nope", [memory("m", "")], []);
    expect(sources).toEqual([]);
    expect(context).toContain("no relevant records");
  });

  it("drops beliefs whose content is empty even on a subject-key match", () => {
    const { sources } = buildEvidence(
      "svc-a status",
      [],
      [belief("b1", "svc-a", "")] // empty content → still matched on key, included
    );
    expect(sources).toHaveLength(1);
    expect(sources[0].source).toBe("svc-a");
  });
});
