import { existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { app } from "./app.js";

const here = dirname(fileURLToPath(import.meta.url));
const addonPath = join(here, "..", "..", "..", "packages", "node", "engram_node.node");

// initialize touches no tool/DB, but importing app.js loads @engram/node, so
// guard on the built addon like the other real-Rust tests.
describe.skipIf(!existsSync(addonPath))("POST /mcp Streamable HTTP", () => {
  it("completes the initialize handshake with a valid MCP result", async () => {
    const res = await app.request("/mcp", {
      method: "POST",
      headers: {
        "content-type": "application/json",
        accept: "application/json, text/event-stream",
      },
      body: JSON.stringify({
        jsonrpc: "2.0",
        id: 1,
        method: "initialize",
        params: {
          protocolVersion: "2024-11-05",
          capabilities: {},
          clientInfo: { name: "vitest", version: "0" },
        },
      }),
    });

    expect(res.status).toBe(200);
    const text = await res.text();
    // Streamable HTTP returns either a JSON body or an SSE frame
    // ("event: message\ndata: {…}"). Extract the JSON-RPC payload either way.
    const dataLine = text.split("\n").find((l) => l.startsWith("data:"));
    const payload = JSON.parse((dataLine ?? text).replace(/^data:\s*/, "").trim());
    expect(payload.result.serverInfo.name).toBe("engram");
    expect(payload.result.capabilities.tools).toBeDefined();
  });
});
