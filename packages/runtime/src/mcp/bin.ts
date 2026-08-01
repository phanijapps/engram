#!/usr/bin/env node
import { parseArgs } from "node:util";

import { buildEngramConfig } from "../shared/config.js";
import { startMcpHttpServer } from "./server.js";

const { values } = parseArgs({
  options: {
    config: { type: "string" },
    port: { type: "string" }
  },
  args: process.argv.slice(2),
  strict: true
});

if (!values.config) {
  console.error("engram-mcp-http: --config <json|path> is required");
  process.exit(1);
}

const configJson = buildEngramConfig(values.config);
const port = values.port ? Number.parseInt(values.port, 10) : 3000;
if (!Number.isInteger(port) || port < 1 || port > 65535) {
  console.error(`engram-mcp-http: invalid --port ${values.port ?? ""}`);
  process.exit(2);
}

const server = await startMcpHttpServer({ configJson, port });
console.error(`engram-mcp-http listening on http://127.0.0.1:${port}/mcp (loopback)`);

const shutdown = (): void => {
  server.close();
  process.exit(0);
};
process.once("SIGINT", shutdown);
process.once("SIGTERM", shutdown);
