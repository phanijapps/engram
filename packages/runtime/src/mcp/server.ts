import { createServer, type Server } from "node:http";

import type { NativeProviderTransport } from "@engram/node";
import {
  localhostHostValidation,
  localhostOriginValidation,
  NodeStreamableHTTPServerTransport
} from "@modelcontextprotocol/node";
import { McpServer } from "@modelcontextprotocol/server";

import { createNativeProviderTransport } from "@engram/node";

import { registerTools } from "./tools.js";

export interface McpHttpOptions {
  /** Inject a transport (tests). If unset, one is built from `configJson`. */
  transport?: NativeProviderTransport;
  configJson?: string;
  port?: number;
}

/** Builds the McpServer with the Module-2 tools registered against the facade. */
export function createMcpServer(transport: NativeProviderTransport): McpServer {
  const server = new McpServer({ name: "engram", version: "0.1.0" });
  registerTools(server, transport);
  return server;
}

/**
 * Starts the HTTP-MCP server on loopback (`127.0.0.1`) behind the SDK's host +
 * origin guards (DNS-rebinding protection). Stateless (a fresh transport per
 * request). Returns the underlying `node:http` `Server` so callers/tests can stop it.
 */
export async function startMcpHttpServer(opts: McpHttpOptions): Promise<Server> {
  const transport =
    opts.transport ??
    (() => {
      if (!opts.configJson) {
        throw new Error(
          "startMcpHttpServer: configJson is required when transport is not injected"
        );
      }
      return createNativeProviderTransport({ configJson: opts.configJson });
    })();

  const validateHost = localhostHostValidation();
  const validateOrigin = localhostOriginValidation();
  const port = opts.port ?? 3000;

  const httpServer = createServer(async (req, res) => {
    // Guard: refuse non-local Host/Origin (the SDK's DNS-rebinding protection).
    if (!validateHost(req, res) || !validateOrigin(req, res)) return;
    // Stateless: a FRESH McpServer + transport per request (the SDK's canonical
    // stateless pattern — a shared server would race on `_transport` under
    // concurrency and leak SSE state). Close the transport after handling so no
    // per-request state escapes.
    const t = new NodeStreamableHTTPServerTransport({ sessionIdGenerator: undefined });
    try {
      await createMcpServer(transport).connect(t);
      await t.handleRequest(req, res);
    } catch (err) {
      if (!res.headersSent) {
        res.writeHead(500, { "Content-Type": "application/json" });
        res.end(
          JSON.stringify({
            jsonrpc: "2.0",
            error: { code: -32603, message: String(err) }
          })
        );
      }
    } finally {
      await t.close();
    }
  });

  httpServer.on("error", (err) => {
    console.error(`engram-mcp-http: listen failed: ${err.message}`);
    process.exit(1);
  });

  await new Promise<void>((resolve) => {
    httpServer.listen(port, "127.0.0.1", resolve);
  });
  return httpServer;
}
