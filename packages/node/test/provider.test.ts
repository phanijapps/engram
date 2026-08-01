import { describe, it, expect } from "vitest";

import { createNativeProviderTransport } from "../src/provider.js";
import type { NativeProviderBinding } from "../src/binding.js";

/** A recording mock of NativeProviderBinding: each call parses its JSON arg and
 *  records it under a key; require*Api() return sub-mocks that do the same.
 *  `recorded(key)` returns the captured args (empty until something is captured). */
function mockProvider(): {
  provider: NativeProviderBinding;
  recorded: (key: string) => unknown[];
} {
  const calls: Record<string, unknown[]> = {};
  const capture = (key: string) => (arg: string) => {
    (calls[key] ??= []).push(JSON.parse(arg));
    return "{}";
  };
  const recorded = (key: string): unknown[] => calls[key] ?? [];
  const provider = {
    capabilitiesJson: () => {
      (calls.capabilities ??= []).push(null);
      return '{"families":[]}';
    },
    consolidateJson: capture("consolidate"),
    scanRepositoryJson: capture("scan"),
    requireMemoryApi: () => ({
      searchJson: capture("search"),
      writeJson: capture("write"),
      forgetJson: capture("forget")
    }),
    requireRecallApi: () => ({ recallJson: capture("recall") }),
    requireGraphApi: () => ({
      getEntityJson: capture("getEntity"),
      putEntityJson: capture("putEntity"),
      neighborsJson: capture("neighbors")
    }),
    requireBatchApi: () => ({
      ingestJson: capture("batchIngest"),
      transactionGuarantee: () => '"BestEffort"'
    }),
    requireBeliefsApi: () => ({
      getBeliefJson: capture("getBelief"),
      upsertBeliefJson: capture("upsertBelief"),
      retractBeliefJson: capture("retractBelief"),
      listStaleBeliefsJson: capture("listStaleBeliefs")
    })
  } as unknown as NativeProviderBinding;
  return { provider, recorded };
}

describe("NativeProviderTransport", () => {
  it("recall dispatches to requireRecallApi().recallJson with the serialized request", async () => {
    const { provider, recorded } = mockProvider();
    const transport = createNativeProviderTransport({ provider });
    await transport.recall({ query: "auth", scope: { tenant: "t" } } as never);
    expect(recorded("recall")).toHaveLength(1);
    expect(recorded("recall")[0]).toMatchObject({ query: "auth" });
  });

  it("write dispatches to requireMemoryApi().writeJson", async () => {
    const { provider, recorded } = mockProvider();
    const transport = createNativeProviderTransport({ provider });
    await transport.write({ key: "k", content: "c", scope: { tenant: "t" } } as never);
    expect(recorded("write")).toHaveLength(1);
    expect(recorded("write")[0]).toMatchObject({ key: "k", content: "c" });
  });

  it("scan dispatches to scanRepositoryJson with path + scope", async () => {
    const { provider, recorded } = mockProvider();
    const transport = createNativeProviderTransport({ provider });
    await transport.scan({ path: "/repo", scope: { tenant: "t" } });
    expect(recorded("scan")).toHaveLength(1);
    expect(recorded("scan")[0]).toMatchObject({ path: "/repo", scope: { tenant: "t" } });
  });

  it("consolidate forwards dryRun and injects a system requester", async () => {
    const { provider, recorded } = mockProvider();
    const transport = createNativeProviderTransport({ provider });
    await transport.consolidate({ scope: { tenant: "t" }, dryRun: true });
    expect(recorded("consolidate")).toHaveLength(1);
    const req = recorded("consolidate")[0] as Record<string, unknown>;
    expect(req).toMatchObject({ dryRun: true, scope: { tenant: "t" } });
    expect(req.requester).toMatchObject({
      actor: { id: "engram-node", kind: "agent", displayName: "engram-node" }
    });
  });

  it("consolidate omits dryRun when not specified", async () => {
    const { provider, recorded } = mockProvider();
    const transport = createNativeProviderTransport({ provider });
    await transport.consolidate({ scope: { tenant: "t" } });
    const req = recorded("consolidate")[0] as Record<string, unknown>;
    expect(req.dryRun).toBeUndefined();
    expect(req.requester).toBeDefined();
  });

  it("putEntity dispatches to requireGraphApi().putEntityJson", async () => {
    const { provider, recorded } = mockProvider();
    const transport = createNativeProviderTransport({ provider });
    await transport.putEntity({ name: "Auth", kind: "function" });
    expect(recorded("putEntity")).toHaveLength(1);
    expect(recorded("putEntity")[0]).toMatchObject({ name: "Auth" });
  });

  it("batchIngest dispatches to requireBatchApi().ingestJson", async () => {
    const { provider, recorded } = mockProvider();
    const transport = createNativeProviderTransport({ provider });
    await transport.batchIngest({ writes: [] });
    expect(recorded("batchIngest")).toHaveLength(1);
    expect(recorded("batchIngest")[0]).toMatchObject({ writes: [] });
  });

  it("capabilities dispatches to capabilitiesJson", async () => {
    const { provider, recorded } = mockProvider();
    const transport = createNativeProviderTransport({ provider });
    const report = await transport.capabilities();
    expect(recorded("capabilities")).toHaveLength(1);
    expect(report).toMatchObject({ families: [] });
  });
});
