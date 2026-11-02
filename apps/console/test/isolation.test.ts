/**
 * TASK-010 focused proofs (GATE-003, AC-003): selecting a partition issues
 * EXACT-TUPLE engine-scoped reads only — every listing request the console
 * sends carries the selected partition verbatim, a record written to a
 * sibling partition is verifiably absent from the view, and listings
 * paginate through the engine's continuation offsets. The engine side of
 * exact-tuple semantics is proven by GATE-B/GATE-C/GATE-005; here the
 * provider stub asserts what the console asks for and answers with the same
 * exact-tuple contract.
 */
import { describe, expect, it, afterEach } from "vitest";
import { createConsoleServer, partitionKey } from "../server/app.mjs";
import type { Server } from "node:http";

/** Records planted per partition: A holds three memories, sibling B holds one. */
const PARTITION_A = { tenant: "acme", subject: "agent", workspace: "proj-a", session: null, environment: "prod" };
const PARTITION_B = { tenant: "acme", subject: "agent", workspace: "proj-b", session: null, environment: "prod" };
const SIBLING_RECORD = { id: "mem-sibling", content: { text: "belongs to B" } };
const A_RECORDS = [
  { id: "mem-a-1", content: { text: "a one" } },
  { id: "mem-a-2", content: { text: "a two" } },
  { id: "mem-a-3", content: { text: "a three" } }
];

/** Exact-tuple equality — mirrors the engine port semantics. */
function exactTuple(a: Record<string, unknown>, b: Record<string, unknown>): boolean {
  for (const key of ["tenant", "subject", "workspace", "session", "environment"]) {
    if ((a[key] ?? null) !== (b[key] ?? null)) return false;
  }
  return true;
}

/**
 * A provider stub implementing the port's exact-tuple contract; it records
 * every listing request so the tests can assert the console never widens a
 * partition read into a wildcard scope.
 */
function recordingProvider() {
  const requests: { view: string; partition: Record<string, unknown>; limit: number; offset: number }[] = [];
  const provider = {
    requests,
    capabilitiesJson: () => "{}",
    requireScopeInventoryApi: () => ({
      inventoryJson: () => "[]",
      partitionMemoriesJson: (requestJson: string) => {
        const { partition, page } = JSON.parse(requestJson);
        requests.push({ view: "memories", partition, limit: page.limit, offset: page.offset });
        const all = exactTuple(partition, PARTITION_A) ? A_RECORDS : exactTuple(partition, PARTITION_B) ? [SIBLING_RECORD] : [];
        const items = all.slice(page.offset, page.offset + page.limit);
        return JSON.stringify({
          items,
          nextOffset: page.offset + items.length < all.length ? page.offset + items.length : null
        });
      },
      partitionEntitiesJson: (requestJson: string) => {
        const { partition, page } = JSON.parse(requestJson);
        requests.push({ view: "entities", partition, limit: page.limit, offset: page.offset });
        return JSON.stringify({ items: [], nextOffset: null });
      },
      partitionRelationshipsJson: (requestJson: string) => {
        const { partition, page } = JSON.parse(requestJson);
        requests.push({ view: "relationships", partition, limit: page.limit, offset: page.offset });
        return JSON.stringify({ items: [], nextOffset: null });
      },
      partitionRecentMemoriesJson: () => "[]"
    })
  };
  return provider;
}

const servers: Server[] = [];
async function start(provider: ReturnType<typeof recordingProvider>) {
  const app = createConsoleServer({ provider, backend: "sqlite", storage: "/tmp/x" });
  const server = await app.listen(0, "127.0.0.1");
  servers.push(server);
  const address = server.address();
  if (typeof address === "string" || address === null) throw new Error("tcp address expected");
  return `http://127.0.0.1:${address.port}`;
}

afterEach(() => {
  while (servers.length > 0) servers.pop()?.close();
});

describe("job012 partition isolation (STEP-009)", () => {
  it("isolation: selecting partition A issues only exact-tuple engine reads", async () => {
    const provider = recordingProvider();
    const base = await start(provider);
    const response = await fetch(`${base}/api/partitions/${partitionKey(PARTITION_A)}/memories?limit=25&offset=0`);
    const body = await response.json();
    expect(body.items.map((m: { id: string }) => m.id)).toEqual(["mem-a-1", "mem-a-2", "mem-a-3"]);
    expect(provider.requests).toHaveLength(1);
    expect(provider.requests[0].view).toBe("memories");
    expect(exactTuple(provider.requests[0].partition, PARTITION_A)).toBe(true);
  });

  it("isolation: a sibling-partition record is verifiably absent", async () => {
    const provider = recordingProvider();
    const base = await start(provider);
    const aBody = await (await fetch(`${base}/api/partitions/${partitionKey(PARTITION_A)}/memories?limit=25&offset=0`)).json();
    const bBody = await (await fetch(`${base}/api/partitions/${partitionKey(PARTITION_B)}/memories?limit=25&offset=0`)).json();
    expect(bBody.items.map((m: { id: string }) => m.id)).toEqual(["mem-sibling"]);
    const aIds = aBody.items.map((m: { id: string }) => m.id);
    expect(aIds).not.toContain("mem-sibling");
    const text = JSON.stringify(aBody);
    expect(text).not.toContain("belongs to B");
  });

  it("isolation: the '(unset)' partition is addressable exactly", async () => {
    const provider = recordingProvider();
    const base = await start(provider);
    const unset = { tenant: "acme", subject: null, workspace: null, session: null, environment: null };
    const body = await (await fetch(`${base}/api/partitions/${partitionKey(unset)}/memories?limit=10&offset=0`)).json();
    expect(body.items).toEqual([]);
    expect(exactTuple(provider.requests[0].partition, unset)).toBe(true);
  });

  it("isolation: listings paginate through the engine's continuation offsets", async () => {
    const provider = recordingProvider();
    const base = await start(provider);
    const page1 = await (await fetch(`${base}/api/partitions/${partitionKey(PARTITION_A)}/memories?limit=2&offset=0`)).json();
    expect(page1.items).toHaveLength(2);
    expect(page1.nextOffset).toBe(2);
    const page2 = await (await fetch(`${base}/api/partitions/${partitionKey(PARTITION_A)}/memories?limit=2&offset=2`)).json();
    expect(page2.items.map((m: { id: string }) => m.id)).toEqual(["mem-a-3"]);
    expect(page2.nextOffset).toBeNull();
    const offsets = provider.requests.map((r) => r.offset);
    expect(offsets).toEqual([0, 2]);
  });

  it("isolation: malformed partition keys are rejected as bad requests", async () => {
    const base = await start(recordingProvider());
    const response = await fetch(`${base}/api/partitions/not-a-key/memories`);
    expect(response.status).toBe(400);
    const body = await response.json();
    expect(body.state).toBe("bad-request");
  });
});
