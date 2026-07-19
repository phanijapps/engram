// Library entry for the prototype backend.
//
// The HTTP server entry is `main.ts`; this module re-exports the pieces other
// workspace packages need — currently the stdio MCP server (`engram-mcp`),
// which reuses the same tool registration and environment bootstrap as the
// HTTP /mcp route.

export { registerEngramTools } from "./mcp/tools.js";
export { buildToolDeps } from "./mcp/deps.js";
export { applyEnvDefaults } from "./bootstrap.js";
export { SCAN_SCOPE } from "./data/scan-defaults.js";
