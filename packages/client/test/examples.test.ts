import { describe, expect, it } from "vitest";

import { runInjectedTransportExample } from "../examples/injected-transport.js";

describe("@engram/client examples", () => {
  it("runs the injected transport usage example", async () => {
    const context = await runInjectedTransportExample();

    expect(context.items).toHaveLength(1);
    expect(context.items[0]?.targetType).toBe("memory");
  });
});
