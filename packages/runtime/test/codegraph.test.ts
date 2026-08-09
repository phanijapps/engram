/* Unit tests for the pure-TS codegraph algorithms (RFC-0020 T7 suffix resolver).
 *
 * After T1, code entity names are qualified (`{repo}/{path}::{bare}`). The
 * `resolveSymbolNames` resolver + the multi-seed BFS unions let a user-supplied
 * bare symbol reach its qualified entity + neighborhood on both `symbol_context`
 * / `change_impact` (multi-seed union) and `search` (all matches).
 */
import { describe, expect, it } from "vitest";

import {
  changeImpactBFS,
  changeImpactBFSMulti,
  resolveSymbolNames,
  symbolContextBFS,
  symbolContextBFSMulti,
  type CodeEdge,
} from "../src/mcp/codegraph.js";

describe("resolveSymbolNames (RFC-0020 T7)", () => {
  it("resolves a bare query to a qualified name via the :: suffix", () => {
    const names = ["my-repo/src/lib.rs::parse_symbol", "other/file.rs::other_fn"];
    expect(resolveSymbolNames(names, "parse_symbol")).toEqual([
      "my-repo/src/lib.rs::parse_symbol",
    ]);
  });

  it("resolves a bare query to multiple qualified names (multi-match)", () => {
    const names = [
      "my-repo/src/lib.rs::parse_symbol",
      "my-repo/src/parser.rs::parse_symbol",
    ];
    const resolved = resolveSymbolNames(names, "parse_symbol");
    expect(resolved.length).toBe(2);
    expect(resolved).toContain("my-repo/src/lib.rs::parse_symbol");
    expect(resolved).toContain("my-repo/src/parser.rs::parse_symbol");
  });

  it("matches the last path segment via the / suffix", () => {
    const names = ["my-repo/lib", "my-repo/src/main"];
    expect(resolveSymbolNames(names, "lib")).toEqual(["my-repo/lib"]);
  });

  it("matches an already-qualified query verbatim (exact arm)", () => {
    const names = ["my-repo/src/lib.rs::parse_symbol"];
    expect(resolveSymbolNames(names, "my-repo/src/lib.rs::parse_symbol")).toEqual([
      "my-repo/src/lib.rs::parse_symbol",
    ]);
  });

  it("does not match a bare suffix without a ::// delimiter", () => {
    expect(resolveSymbolNames(["foobar"], "bar")).toEqual([]);
    expect(resolveSymbolNames(["parse_symbol_x"], "parse_symbol")).toEqual([]);
  });

  it("returns empty when no name matches", () => {
    expect(resolveSymbolNames(["alpha", "beta"], "gamma")).toEqual([]);
  });
});

describe("symbolContextBFSMulti (multi-seed union, RFC-0020 T7)", () => {
  const edges: CodeEdge[] = [
    { subject: "repo/a.rs::fn_a", predicate: "calls", object: "repo/b.rs::fn_b" },
    { subject: "repo/c.rs::fn_c", predicate: "calls", object: "repo/d.rs::fn_d" },
    { subject: "repo/b.rs::fn_b", predicate: "calls", object: "repo/shared.rs::helper" },
  ];

  it("unions callees across two resolved seeds, de-duped", () => {
    const ctx = symbolContextBFSMulti(
      edges,
      ["repo/a.rs::fn_a", "repo/c.rs::fn_c"],
      2,
      50,
    );
    // fn_a → fn_b → helper; fn_c → fn_d. Union callees (de-duped).
    const sortedCallees = [...ctx.callees].sort();
    expect(sortedCallees).toEqual([
      "repo/b.rs::fn_b",
      "repo/d.rs::fn_d",
      "repo/shared.rs::helper",
    ]);
    expect(ctx.callers).toEqual([]);
  });

  it("is identical to single-seed symbolContextBFS when one seed is passed", () => {
    const single = symbolContextBFS(edges, "repo/a.rs::fn_a", 2, 50);
    const multi = symbolContextBFSMulti(edges, ["repo/a.rs::fn_a"], 2, 50);
    expect(multi.callers).toEqual(single.callers);
    expect(multi.callees).toEqual(single.callees);
  });

  it("de-dups a shared neighbor reached from two seeds", () => {
    // Both fn_a and fn_c ultimately reach nothing shared here, but construct a
    // case where two seeds reach the same callee.
    const localEdges: CodeEdge[] = [
      { subject: "x::a", predicate: "calls", object: "x::shared" },
      { subject: "x::b", predicate: "calls", object: "x::shared" },
    ];
    const ctx = symbolContextBFSMulti(localEdges, ["x::a", "x::b"], 1, 50);
    // shared reached from both seeds → appears once.
    expect(ctx.callees).toEqual(["x::shared"]);
  });
});

describe("changeImpactBFSMulti (multi-seed union, RFC-0020 T7)", () => {
  const edges: CodeEdge[] = [
    { subject: "repo/caller1.rs::c1", predicate: "calls", object: "repo/target.rs::t" },
    { subject: "repo/caller2.rs::c2", predicate: "calls", object: "repo/other.rs::o" },
  ];

  it("unions blast radius across two targets, de-duped", () => {
    const rows = changeImpactBFSMulti(
      edges,
      ["repo/target.rs::t", "repo/other.rs::o"],
      3,
      100,
    );
    const callers = rows.map((r) => r.caller).sort();
    expect(callers).toEqual(["repo/caller1.rs::c1", "repo/caller2.rs::c2"]);
  });

  it("is identical to single-seed changeImpactBFS when one target is passed", () => {
    const single = changeImpactBFS(edges, "repo/target.rs::t", 3, 100);
    const multi = changeImpactBFSMulti(edges, ["repo/target.rs::t"], 3, 100);
    expect(multi).toEqual(single);
  });
});
