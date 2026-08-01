import { defineConfig } from "tsup";

export default defineConfig({
  entry: ["src/index.ts", "src/ingest/bin.ts", "src/mcp/bin.ts", "src/maintenance/bin.ts"],
  format: ["esm"],
  dts: true,
  clean: true,
  // Keep workspace deps + the MCP SDK + zod external so the bin resolves them
  // (and the native addon) from node_modules at runtime instead of inlining.
  external: [
    "@engram/contracts",
    "@engram/node",
    "@modelcontextprotocol/server",
    "@modelcontextprotocol/node",
    "zod"
  ]
});
