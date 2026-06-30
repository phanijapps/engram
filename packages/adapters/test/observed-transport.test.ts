import { describe, expect, it } from "vitest";

import type { EngramTransport } from "@engram/client";

import { createObservedTransport, type AdapterEvent } from "../src/index.js";

describe("@engram/adapters observed transport", () => {
  it("preserves results and emits ordered operation events", async () => {
    const events: AdapterEvent[] = [];
    const transport = createObservedTransport({
      transport: fakeTransport(),
      observer: {
        emit(event) {
          events.push(event);
        }
      },
      now: scriptedClock()
    });

    const response = await transport.writeMemory({} as never);

    expect(response.record).toEqual({ id: "memory-1" });
    expect(events.map((event) => event.kind)).toEqual(["operation_started", "operation_succeeded"]);
    expect(events[0]).toMatchObject({ operation: "writeMemory" });
    expect(events[1]).toMatchObject({ operation: "writeMemory", durationMs: 5 });
  });

  it("emits retrieval trace counts without changing the payload", async () => {
    const events: AdapterEvent[] = [];
    const payload = {
      items: [{ id: "result-1" }],
      omitted: [{ id: "omitted-1" }],
      sourceFailures: [{ source: "vector" }],
      createdAt: "2026-06-29T12:00:00Z"
    } as never;
    const transport = createObservedTransport({
      transport: {
        ...fakeTransport(),
        async retrieve() {
          return payload;
        }
      },
      observer: {
        emit(event) {
          events.push(event);
        }
      },
      now: scriptedClock()
    });

    const result = await transport.retrieve({} as never);

    expect(result).toBe(payload);
    expect(events.map((event) => event.kind)).toEqual([
      "operation_started",
      "retrieval_trace",
      "operation_succeeded"
    ]);
    expect(events[1]).toMatchObject({
      kind: "retrieval_trace",
      itemCount: 1,
      omittedCount: 1,
      sourceFailureCount: 1
    });
  });

  it("classifies policy denial shaped failures and rethrows them", async () => {
    const events: AdapterEvent[] = [];
    const transport = createObservedTransport({
      transport: {
        ...fakeTransport(),
        async retrieve() {
          throw new Error("policy denied: restricted");
        }
      },
      observer: {
        emit(event) {
          events.push(event);
        }
      },
      now: scriptedClock()
    });

    await expect(transport.retrieve({} as never)).rejects.toThrow("policy denied");
    expect(events.map((event) => event.kind)).toEqual(["operation_started", "operation_failed"]);
    expect(events[1]).toMatchObject({
      kind: "operation_failed",
      operation: "retrieve",
      errorKind: "policy_denial"
    });
  });

  it("ignores observer failures", async () => {
    const transport = createObservedTransport({
      transport: fakeTransport(),
      observer: {
        emit() {
          throw new Error("observer failed");
        }
      },
      now: scriptedClock()
    });

    await expect(transport.forget({} as never)).resolves.toMatchObject({
      targetType: "memory",
      targetId: "memory-1",
      status: "not_found"
    });
  });
});

function fakeTransport(): EngramTransport {
  return {
    async writeMemory() {
      return {
        record: { id: "memory-1" },
        event: { id: "event-1" }
      } as never;
    },
    async retrieve() {
      return {
        items: [],
        createdAt: "2026-06-29T12:00:00Z"
      };
    },
    async forget() {
      return {
        targetType: "memory",
        targetId: "memory-1",
        status: "not_found"
      };
    }
  };
}

function scriptedClock(): () => Date {
  let tick = 0;
  return () => new Date(Date.UTC(2026, 5, 29, 12, 0, 0, tick++ * 5));
}
