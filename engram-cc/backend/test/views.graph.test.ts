//! T2 projections — TDD against authentic record_json fixtures (captured from
//! the agentzero store). Proves projectEntity / projectRelationship pick the
//! UI-facing fields and tolerate missing optional fields.

import { describe, it, expect } from "vitest";

import { projectEntity, projectOutgoingNeighbor, projectRelationship } from "../src/views/graph.ts";

// Authentic shapes (lightly truncated) from the agentzero store.
const entityRecord = {
  id: "repo-ebe4fd8c",
  kind: "repository",
  name: "github.com/phanijapps/zbot",
  scope: { tenant: "default", workspace: "agentzero" },
  provenance: {
    source: "engram-mcp-scan",
    confidence: 1.0,
    method: "deterministic_text_ingestion",
  },
  createdAt: "2026-07-31T19:46:38.058Z",
  validFrom: "2026-07-31T19:46:38.058Z",
  metadata: { stableSourceKey: "github.com/phanijapps/zbot" },
};

const relationshipRecord = {
  id: "exposes-9c410e",
  subject: { id: "repo-ebe4fd8c", kind: "repository", name: "github.com/phanijapps/zbot" },
  predicate: "exposes",
  object: { id: "api-d982cab6", kind: "api", name: "GET /api/agents" },
  scope: { tenant: "default", workspace: "agentzero" },
  confidence: 0.95,
  provenance: { source: "engram-mcp-scan […]", confidence: 1.0 },
  createdAt: "2026-07-31T19:46:38.659Z",
};

describe("projectEntity", () => {
  it("projects id / name / kind / stableSourceKey from a real entity record", () => {
    const view = projectEntity(entityRecord);
    expect(view).toEqual({
      id: "repo-ebe4fd8c",
      name: "github.com/phanijapps/zbot",
      kind: "repository",
      stableSourceKey: "github.com/phanijapps/zbot",
    });
  });

  it("picks graphId from graph_id when present", () => {
    const view = projectEntity({ ...entityRecord, graph_id: "graph-1" });
    expect(view.graphId).toBe("graph-1");
  });

  it("omits optional fields when absent (no undefined leakage)", () => {
    const view = projectEntity({ id: "e1", kind: "function", name: "foo" });
    expect(view).toEqual({ id: "e1", kind: "function", name: "foo" });
    expect("stableSourceKey" in view).toBe(false);
    expect("graphId" in view).toBe(false);
  });
});

describe("projectRelationship", () => {
  it("projects source / predicate / target / confidence from a real relationship", () => {
    const view = projectRelationship(relationshipRecord);
    expect(view).toEqual({
      source: "repo-ebe4fd8c",
      predicate: "exposes",
      target: "api-d982cab6",
      confidence: 0.95,
    });
  });

  it("omits confidence when absent", () => {
    const view = projectRelationship({
      subject: { id: "a" },
      predicate: "calls",
      object: { id: "b" },
    });
    expect(view).toEqual({ source: "a", predicate: "calls", target: "b" });
    expect("confidence" in view).toBe(false);
  });
});

describe("projectOutgoingNeighbor", () => {
  it("projects the object endpoint as the neighbor, direction outgoing", () => {
    const entry = projectOutgoingNeighbor(relationshipRecord);
    expect(entry.direction).toBe("outgoing");
    expect(entry.entity).toEqual({ id: "api-d982cab6", name: "GET /api/agents", kind: "api" });
    expect(entry.relationship).toEqual({
      source: "repo-ebe4fd8c",
      predicate: "exposes",
      target: "api-d982cab6",
      confidence: 0.95,
    });
  });
});
