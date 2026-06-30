import { describe, expect, it } from "vitest";

import { createNativeEngramClient } from "../src/index.js";

describe("@engram/client native transport", () => {
  it("creates a client over the node native transport boundary", async () => {
    const client = createNativeEngramClient({
      binding: {
        NativeMemoryEngine: class {
          writeMemoryJson(): string {
            return JSON.stringify({
              record: { id: "memory-client-1" },
              event: { id: "event-client-1" },
              deduplicated: false
            });
          }

          retrieveJson(): string {
            return JSON.stringify({
              items: [{ targetId: "memory-client-1" }],
              omitted: [],
              sourceFailures: [],
              createdAt: "2026-06-29T12:00:00Z"
            });
          }

          forgetJson(): string {
            return JSON.stringify({
              targetType: "memory",
              targetId: "memory-client-1",
              status: "deleted"
            });
          }
        }
      }
    });

    const write = await client.writeMemory({} as never);
    const context = await client.retrieve({} as never);
    const forget = await client.forget({} as never);

    expect(write.record.id).toBe("memory-client-1");
    expect(context.items[0]?.targetId).toBe("memory-client-1");
    expect(forget.status).toBe("deleted");
  });
});
