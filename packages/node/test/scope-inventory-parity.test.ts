/**
 * TASK-005 focused proof — the RISK-002 differential (GATE-005): the SAME
 * fixture seeded through the engine's own write surfaces on BOTH native
 * recipes must yield the SAME inventory and partition views. Each recipe leg
 * runs in its own child process (one addon per process — vitest file
 * isolation is not enough here because one FILE needs both recipes), and the
 * two legs share one run-unique nonce so their fixtures are identical and
 * never collide with rows persisted by earlier runs on the disposable
 * Postgres.
 *
 * Equivalence is the plan's definition (spec assumption): equal partition
 * sets, equal per-partition counts, equal "(unset)" handling, and equal
 * partition-view row sets — not byte-equal rendering. Recent additions are
 * compared as sets (each engine's newest-first tie-breaking is its own).
 *
 * Escalate-stop rule (plan RISK-002): any divergence here invalidates the
 * unified-semantics seam — it is never fixed by weakening this test.
 */
import { describe, expect, it } from "vitest";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const LEG = fileURLToPath(new URL("./helpers/scope-inventory-leg.mjs", import.meta.url));

interface PartitionSummary {
  scope: {
    tenant: string;
    subject: string | null;
    workspace: string | null;
    session: string | null;
    environment: string | null;
  };
  memoryCount: number;
  entityCount: number;
  beliefCount: number;
}

interface LegResult {
  recipe: string;
  tenant: string;
  inventory: PartitionSummary[];
  listingA: string[];
  listingUnset: string[];
  listingOtherTenant: string[];
  entitiesA: string[];
  entitiesUnset: string[];
  relationshipsA: string[];
  recentA: string[];
  recentUnset: string[];
}

function runLeg(recipe: string, nonce: string): LegResult {
  const stdout = execFileSync(process.execPath, [LEG, recipe, nonce], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "inherit"]
  });
  const lines = stdout.trim().split("\n");
  return JSON.parse(lines[lines.length - 1]);
}

/** Canonical projection of the inventory for set/shape comparison. */
function partitionKey(row: PartitionSummary): string {
  const d = (v: string | null): string => (v === null || v === undefined ? "(unset)" : v);
  return [
    row.scope.tenant,
    d(row.scope.subject),
    d(row.scope.workspace),
    d(row.scope.session),
    d(row.scope.environment)
  ].join("/");
}

describe.skipIf(!process.env.ENGRAM_TEST_PG_URL)("job012 scope-inventory parity (differential)", () => {
  const nonce = `job012-parity-${process.pid}-${Date.now()}`;
  let sqlite: LegResult;
  let pgvector: LegResult;

  it(
    "both recipes load, seed the shared fixture, and read through the port",
    { timeout: 30 * 60 * 1000 },
    () => {
      sqlite = runLeg("sqlite", nonce);
      pgvector = runLeg("pgvector", nonce);
      expect(sqlite.recipe).toBe("sqlite");
      expect(pgvector.recipe).toBe("pgvector");
    }
  );

  it("partition sets are equal (including '(unset)' and cross-tenant dimensions)", () => {
    const sqliteKeys = sqlite.inventory.map(partitionKey).sort();
    const pgKeys = pgvector.inventory.map(partitionKey).sort();
    expect(pgKeys).toEqual(sqliteKeys);
    expect(sqliteKeys).toHaveLength(3);
    expect(sqliteKeys).toContain(`${nonce}/agent-1/(unset)/(unset)/prod`);
    expect(sqliteKeys).toContain(`${nonce}/agent-1/proj-a/(unset)/prod`);
    expect(sqliteKeys).toContain(`${nonce}-c/agent-1/proj-a/(unset)/prod`);
  });

  it("per-partition counts are equal on every kind", () => {
    const byKey = (rows: PartitionSummary[]) =>
      new Map(rows.map((row) => [partitionKey(row), row]));
    const sqliteMap = byKey(sqlite.inventory);
    const pgMap = byKey(pgvector.inventory);
    for (const [key, sqliteRow] of sqliteMap) {
      const pgRow = pgMap.get(key);
      expect(pgRow, `pgvector must report partition ${key}`).toBeDefined();
      expect(pgRow.memoryCount).toBe(sqliteRow.memoryCount);
      expect(pgRow.entityCount).toBe(sqliteRow.entityCount);
      expect(pgRow.beliefCount).toBe(sqliteRow.beliefCount);
    }
    // The fixture's expected counts, stated once so a double drift cannot pass.
    const a = sqliteMap.get(`${nonce}/agent-1/proj-a/(unset)/prod`);
    expect(a?.memoryCount).toBe(3);
    expect(a?.entityCount).toBe(2);
    expect(a?.beliefCount).toBe(2);
    const unset = sqliteMap.get(`${nonce}/agent-1/(unset)/(unset)/prod`);
    expect(unset?.memoryCount).toBe(2);
    expect(unset?.entityCount).toBe(1);
    expect(unset?.beliefCount).toBe(0);
  });

  it("partition-view row sets are equal (memories by content, graph rows by id)", () => {
    const sorted = (xs: string[]) => [...xs].sort();
    expect(sorted(pgvector.listingA)).toEqual(sorted(sqlite.listingA));
    expect(sorted(pgvector.listingUnset)).toEqual(sorted(sqlite.listingUnset));
    expect(sorted(pgvector.listingOtherTenant)).toEqual(sorted(sqlite.listingOtherTenant));
    expect(sorted(pgvector.entitiesA)).toEqual(sorted(sqlite.entitiesA));
    expect(sorted(pgvector.entitiesUnset)).toEqual(sorted(sqlite.entitiesUnset));
    expect(sorted(pgvector.relationshipsA)).toEqual(sorted(sqlite.relationshipsA));
    // And the views hold exactly the fixture's rows.
    expect(sorted(sqlite.listingA)).toEqual(["a-memory-1", "a-memory-2", "a-memory-3"]);
    expect(sorted(sqlite.listingUnset)).toEqual(["u-memory-1", "u-memory-2"]);
    expect(sorted(sqlite.listingOtherTenant)).toEqual(["c-memory-1"]);
  });

  it("recent-additions views agree as sets and stay partition-confined", () => {
    const sorted = (xs: string[]) => [...xs].sort();
    expect(sorted(pgvector.recentA)).toEqual(sorted(sqlite.recentA));
    expect(sorted(sqlite.recentA)).toEqual(["a-memory-2", "a-memory-3"]);
    expect(sorted(pgvector.recentUnset)).toEqual(sorted(sqlite.recentUnset));
    expect(sorted(sqlite.recentUnset)).toEqual(["u-memory-2"]);
  });
});
