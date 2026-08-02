#!/usr/bin/env node
import { parseArgs } from "node:util";

import { buildEngramConfig } from "../shared/config.js";
import { startMcpHttpServer } from "./server.js";

const { values } = parseArgs({
  options: {
    config: { type: "string" },
    port: { type: "string" },
    host: { type: "string" },
    "auth-token": { type: "string" },
    "tls-cert": { type: "string" },
    "tls-key": { type: "string" }
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

const host = values.host ?? "127.0.0.1";
const loopback =
  host === "127.0.0.1" || host === "localhost" || host === "::1";

if (!loopback && !values["auth-token"]) {
  console.error(
    "engram-mcp-http: --auth-token is required when binding to a non-loopback host"
  );
  process.exit(2);
}

const scheme = values["tls-cert"] && values["tls-key"] ? "https" : "http";

const server = await startMcpHttpServer({
  configJson,
  port,
  host,
  ...(values["auth-token"] !== undefined
    ? { authToken: values["auth-token"] }
    : {}),
  ...(values["tls-cert"] !== undefined ? { tlsCert: values["tls-cert"] } : {}),
  ...(values["tls-key"] !== undefined ? { tlsKey: values["tls-key"] } : {})
});
console.error(
  `engram-mcp-http listening on ${scheme}://${host}:${port}/mcp` +
    (loopback ? " (loopback)" : " (auth: bearer-token)")
);

const shutdown = (): void => {
  server.close();
  process.exit(0);
};
process.once("SIGINT", shutdown);
process.once("SIGTERM", shutdown);
