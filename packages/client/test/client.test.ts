import { describe, expect, it } from "vitest";

import { createEngramClient, type EngramTransport } from "../src/index.js";

describe("@engram/client", () => {
  it("delegates operations to the injected transport", async () => {
    const calls: string[] = [];
    const transport: EngramTransport = {
      async writeMemory() {
        calls.push("write");
        return {
          record: {} as never,
          event: {} as never
        };
      },
      async retrieve() {
        calls.push("retrieve");
        return {
          items: [],
          createdAt: "2026-06-29T12:00:00Z"
        };
      },
      async forget() {
        calls.push("forget");
        return {
          targetType: "memory",
          targetId: "memory-1",
          status: "not_found"
        };
      }
    };

    const client = createEngramClient({ transport });

    await client.writeMemory({} as never);
    await client.retrieve({} as never);
    await client.forget({} as never);

    expect(calls).toEqual(["write", "retrieve", "forget"]);
  });
});
