/**
 * TASK-004 focused proof — the scope-inventory capability through the REAL
 * native addon (sqlite recipe artifact): the provider reports the capability,
 * `requireScopeInventoryApi` proxies the port, the inventory enumerates
 * partitions written through the engine's own write path, "(unset)"
 * partitions are addressable exactly, and listings paginate with
 * continuation offsets. One recipe addon per test FILE (vitest process
 * isolation); the cross-recipe differential lives in
 * `scope-inventory-parity.test.ts`.
 */
import { describe, expect, it, beforeAll } from "vitest";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { loadRecipe } from "./helpers/build-native-recipes.mjs";
import { configJson } from "./helpers/code-binding-cases.mjs";

// eslint-disable-next-line @typescript-eslint/no-explicit-any
let sqliteAddon: any;

beforeAll(
  () => {
    sqliteAddon = loadRecipe("sqlite");
  },
  15 * 60 * 1000
);

interface PartitionSummary {
  scope: { tenant: string; workspace?: string | null };
  memoryCount: number;
  entityCount: number;
  beliefCount: number;
}

const TENANT = "job012-scope-inventory";

function scope(workspace?: string) {
  return { tenant: TENANT, workspace: workspace ?? null };
}

function writeMemory(provider: any, text: string, workspace: string | null): string {
  const response = JSON.parse(
    provider.requireMemoryApi().writeJson(
      JSON.stringify({
        kind: "observation",
        content: { text },
        scope: scope(workspace),
        requester: {
          actor: { id: "job012", kind: "agent", displayName: "job012" },
          roles: [],
          permissions: ["memory.write", "memory.retrieve"]
        },
        provenance: {
          source: "job012-scope-inventory",
          actor: { id: "job012", kind: "agent", displayName: null },
          observedAt: new Date().toISOString(),
          confidence: 1.0
        },
        policy: {
          visibility: "private",
          retention: "durable",
          sensitivity: "low",
          allowedUses: ["retrieval"]
        },
        links: []
      })
    )
  );
  return response.record.id;
}

describe("job012 scope-inventory (sqlite recipe)", () => {
  it("capability is reported and proxied", () => {
    const base = mkdtempSync(join(tmpdir(), "job012-inv-cap-"));
    const provider = new sqliteAddon.NativeProvider(configJson(base));
    const capabilities = JSON.parse(provider.capabilitiesJson());
    expect(capabilities.scope_inventory?.supported ?? capabilities.scope_inventory).toBeTruthy();
    expect(() => provider.requireScopeInventoryApi()).not.toThrow();
    const inventory = JSON.parse(provider.requireScopeInventoryApi().inventoryJson());
    expect(inventory).toEqual([]);
  });

  it("inventory enumerates partitions with counts and (unset) handling", () => {
    const base = mkdtempSync(join(tmpdir(), "job012-inv-enum-"));
    const provider = new sqliteAddon.NativeProvider(configJson(base));
    writeMemory(provider, "unset workspace", null);
    writeMemory(provider, "project workspace", "proj");
    writeMemory(provider, "project workspace again", "proj");

    const api = provider.requireScopeInventoryApi();
    const rows: PartitionSummary[] = JSON.parse(api.inventoryJson());
    const mine = rows.filter((row) => row.scope.tenant === TENANT);
    expect(mine).toHaveLength(2);
    const unset = mine.find((row) => row.scope.workspace == null);
    const proj = mine.find((row) => row.scope.workspace === "proj");
    expect(unset?.memoryCount).toBe(1);
    expect(proj?.memoryCount).toBe(2);
    expect(proj?.entityCount).toBe(0);
    expect(proj?.beliefCount).toBe(0);
  });

  it("partition reads are exact-tuple and paginated", () => {
    const base = mkdtempSync(join(tmpdir(), "job012-inv-page-"));
    const provider = new sqliteAddon.NativeProvider(configJson(base));
    const ids = [
      writeMemory(provider, "page-1", "paged"),
      writeMemory(provider, "page-2", "paged"),
      writeMemory(provider, "page-3", "paged"),
      writeMemory(provider, "elsewhere", "other")
    ];

    const api = provider.requireScopeInventoryApi();
    const partition = scope("paged");
    const page1 = JSON.parse(
      api.partitionMemoriesJson(JSON.stringify({ partition, page: { limit: 2, offset: 0 } }))
    );
    expect(page1.items).toHaveLength(2);
    expect(page1.nextOffset).toBe(2);
    const page2 = JSON.parse(
      api.partitionMemoriesJson(JSON.stringify({ partition, page: { limit: 2, offset: 2 } }))
    );
    expect(page2.items).toHaveLength(1);
    expect(page2.nextOffset).toBeNull();
    const listed = [...page1.items, ...page2.items].map((m: { id: string }) => m.id);
    expect(listed).toEqual(ids.slice(0, 3).sort());
    expect(listed).not.toContain(ids[3]);

    // The "(unset)" partition is addressable exactly through the same proxy.
    const unset = JSON.parse(
      api.partitionMemoriesJson(
        JSON.stringify({ partition: scope(null), page: { limit: 10, offset: 0 } })
      )
    );
    expect(unset.items).toHaveLength(0);

    // Recent additions: newest first, partition-confined.
    const recent = JSON.parse(
      api.partitionRecentMemoriesJson(JSON.stringify({ partition, limit: 2 }))
    );
    expect(recent.map((m: { content: { text: string } }) => m.content.text)).toEqual([
      "page-3",
      "page-2"
    ]);

    // Entities + relationships list through the same exact-tuple predicate.
    const entities = JSON.parse(
      api.partitionEntitiesJson(JSON.stringify({ partition, page: { limit: 10, offset: 0 } }))
    );
    expect(entities.items).toEqual([]);
    const relationships = JSON.parse(
      api.partitionRelationshipsJson(JSON.stringify({ partition, page: { limit: 10, offset: 0 } }))
    );
    expect(relationships.items).toEqual([]);
  });

  it("invalid page windows throw typed errors", () => {
    const base = mkdtempSync(join(tmpdir(), "job012-inv-bad-"));
    const provider = new sqliteAddon.NativeProvider(configJson(base));
    const api = provider.requireScopeInventoryApi();
    expect(() =>
      api.partitionMemoriesJson(
        JSON.stringify({ partition: scope("x"), page: { limit: 0, offset: 0 } })
      )
    ).toThrow(/limit/i);
  });
});
