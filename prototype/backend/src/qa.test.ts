import { describe, expect, it } from "vitest";
import { buildEvidence } from "./services/evidence.service.js";
import type { MemoryItem, QaBelief, QaEntity, QaRelationship, QaChunk } from "./services/qa.types.js";

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

const entity = (id: string, name: string, kind = "function"): QaEntity => ({
  id, name, kind, graphId: "graph-1",
});

const rel = (id: string, subj: string, pred: string, obj: string): QaRelationship => ({
  id,
  graphId: "graph-1",
  subject: { id: `e-${subj}`, name: subj, kind: "function" },
  predicate: pred,
  object: { id: `e-${obj}`, name: obj, kind: "function" },
});

const NO_GRAPH = { entities: [] as QaEntity[], relationships: [] as QaRelationship[], chunks: [] as QaChunk[] };

describe("buildEvidence", () => {
  it("keeps memories and query-matched beliefs, with sources", () => {
    const { context, sources } = buildEvidence(
      "is svc-a up?",
      [memory("mem-1", "svc-a restarted at noon"), memory("mem-2", "unrelated note")],
      [
        belief("b1", "svc-a", "svc-a is up"),
        belief("b2", "billing", "invoices are late"), // no query-term overlap → dropped
      ],
      NO_GRAPH.entities,
      NO_GRAPH.relationships,
      NO_GRAPH.chunks,
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
    const { context, sources } = buildEvidence("zzz nope", [memory("m", "")], [], [], [], []);
    expect(sources).toEqual([]);
    expect(context).toContain("no relevant records");
  });

  it("drops beliefs whose content is empty even on a subject-key match", () => {
    const { sources } = buildEvidence(
      "svc-a status", [],
      [belief("b1", "svc-a", "")], [], [], [],
    );
    expect(sources).toHaveLength(1);
    expect(sources[0].source).toBe("svc-a");
  });

  it("grounds in knowledge-graph entities + their call-graph relationships", () => {
    const { context, sources } = buildEvidence(
      "call graph for intent analysis",
      [],
      [],
      [
        entity("e-intent_analysis", "intent_analysis", "function"),
        entity("e-parser", "parser", "function"),
        entity("e-unrelated", "billing", "function"), // no query-term overlap → dropped
      ],
      [
        rel("r1", "intent_analysis", "calls", "parser"),
        rel("r2", "billing", "calls", "unrelated_fn"), // neither endpoint matched → dropped
      ],
      [
        { id: "chunk-1", text: "fn intent_analysis() { tokenize(); parse_intent(); classify(); }", entities: [{ id: "e-intent_analysis" }] },
        { id: "chunk-2", text: "fn billing() { invoice(); }", entities: [{ id: "e-unrelated" }] },
      ],
    );
    // intent_analysis matched (contains "intent" + "analysis"); parser is a neighbor.
    const entitySources = sources.filter((s) => s.kind === "entity");
    expect(entitySources).toHaveLength(1);
    expect(entitySources[0].text).toContain("intent_analysis");

    // The call-graph relationship (intent_analysis calls parser) is included.
    const relSources = sources.filter((s) => s.kind === "relationship");
    expect(relSources).toHaveLength(1);
    expect(relSources[0].text).toContain("calls");
    expect(relSources[0].text).toContain("parser");

    expect(context).toContain("[entity] intent_analysis (function)");
    expect(context).toContain("[calls] intent_analysis -> parser");
    expect(context).not.toContain("billing");

    // The chunk referencing intent_analysis is included (the actual code text).
    const chunkSources = sources.filter((s) => s.kind === "chunk");
    expect(chunkSources).toHaveLength(1);
    expect(chunkSources[0].text).toContain("intent_analysis");
    expect(context).toContain("[chunk]");
    expect(context).not.toContain("fn billing"); // unrelated chunk → dropped
  });
});
