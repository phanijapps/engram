import { describe, expect, it } from "vitest";
import {
  assignCommunities,
  colorForCommunity,
  highlightSet,
  tierForKind,
  type GraphEdge,
  type GraphNode,
} from "./graph-model";

const node = (id: string, kind: string, sourcePath?: string, degree = 0): GraphNode => ({
  id,
  name: id,
  kind,
  sourcePath,
  degree,
});

describe("tierForKind", () => {
  it("orders module > class > method by base size", () => {
    expect(tierForKind("module").baseSize).toBeGreaterThan(tierForKind("class").baseSize);
    expect(tierForKind("class").baseSize).toBeGreaterThan(tierForKind("function").baseSize);
  });

  it("marks module and class always-labeled, method not", () => {
    expect(tierForKind("module").alwaysLabel).toBe(true);
    expect(tierForKind("class").alwaysLabel).toBe(true);
    expect(tierForKind("function").alwaysLabel).toBe(false);
    expect(tierForKind("method").alwaysLabel).toBe(false);
  });

  it("maps concept to class tier but not always-labeled", () => {
    const t = tierForKind("concept");
    expect(t.tier).toBe(2);
    expect(t.alwaysLabel).toBe(false);
  });

  it("treats project/organization as top-tier always-labeled landmarks", () => {
    for (const k of ["project", "organization"]) {
      expect(tierForKind(k).tier).toBe(1);
      expect(tierForKind(k).alwaysLabel).toBe(true);
    }
  });

  it("treats variable/api as method-tier satellites", () => {
    for (const k of ["variable", "api"]) {
      expect(tierForKind(k).tier).toBe(3);
      expect(tierForKind(k).alwaysLabel).toBe(false);
    }
  });

  it("falls back to method tier for unrecognized kinds", () => {
    const t = tierForKind("totally-unknown");
    expect(t.tier).toBe(3);
    expect(t.baseSize).toBe(tierForKind("function").baseSize);
    expect(t.alwaysLabel).toBe(false);
  });
});

describe("assignCommunities", () => {
  it("groups a class and its same-sourcePath methods into one community (deterministic)", () => {
    const nodes = [
      node("C", "class", "foo.rs"),
      node("m1", "method", "foo.rs"),
      node("m2", "method", "foo.rs"),
    ];
    const edges: GraphEdge[] = [];

    const first = assignCommunities(nodes, edges);
    const second = assignCommunities(nodes, edges);

    // deterministic across runs
    expect([...first.entries()]).toEqual([...second.entries()]);
    // class + both methods share a community
    expect(first.get("C")).toBe(first.get("m1"));
    expect(first.get("C")).toBe(first.get("m2"));
  });

  it("puts two disconnected call-clusters in different communities", () => {
    const nodes = [
      node("a1", "function", "a.rs"),
      node("a2", "function", "a.rs"),
      node("b1", "function", "b.rs"),
      node("b2", "function", "b.rs"),
    ];
    const edges: GraphEdge[] = [
      { subject: "a1", predicate: "calls", object: "a2" },
      { subject: "b1", predicate: "calls", object: "b2" },
    ];
    const comm = assignCommunities(nodes, edges);
    // membership differs between the two clusters
    expect(comm.get("a1")).toBe(comm.get("a2"));
    expect(comm.get("b1")).toBe(comm.get("b2"));
    expect(comm.get("a1")).not.toBe(comm.get("b1"));
  });

  it("does not group a node with empty sourcePath with unrelated nodes", () => {
    const nodes = [
      node("x", "function", ""),
      node("y", "function", "y.rs"),
      node("z", "class", "y.rs"),
    ];
    const comm = assignCommunities(nodes, []);
    // y and z share a sourcePath → same community; x is isolated → different
    expect(comm.get("y")).toBe(comm.get("z"));
    expect(comm.get("x")).not.toBe(comm.get("y"));
  });

  it("returns an empty map for no nodes", () => {
    expect(assignCommunities([], []).size).toBe(0);
  });
});

describe("highlightSet", () => {
  it("returns the hovered node plus direct neighbors", () => {
    const edges: GraphEdge[] = [
      { subject: "a", predicate: "calls", object: "b" },
      { subject: "c", predicate: "calls", object: "a" },
      { subject: "d", predicate: "calls", object: "e" },
    ];
    expect([...highlightSet(edges, "a")].sort()).toEqual(["a", "b", "c"]);
  });

  it("returns just the node when it has no neighbors", () => {
    expect([...highlightSet([], "solo")]).toEqual(["solo"]);
  });
});

describe("colorForCommunity", () => {
  it("is stable and wraps around the palette", () => {
    expect(colorForCommunity(0)).toBe(colorForCommunity(8));
    expect(colorForCommunity(1)).not.toBe(colorForCommunity(0));
  });
});
