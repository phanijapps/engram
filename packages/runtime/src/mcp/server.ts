import { readFileSync } from "node:fs";
import {
  createServer as createHttpServer,
  type IncomingMessage,
  type Server,
  type ServerResponse
} from "node:http";
import { createServer as createHttpsServer } from "node:https";

import type { NativeProviderTransport } from "@engram/node";
import { createNativeProviderTransport } from "@engram/node";
import {
  localhostHostValidation,
  localhostOriginValidation,
  NodeStreamableHTTPServerTransport
} from "@modelcontextprotocol/node";
import { McpServer } from "@modelcontextprotocol/server";

import { registerTools } from "./tools.js";

export interface McpHttpOptions {
  /** Inject a transport (tests). If unset, one is built from `configJson`. */
  transport?: NativeProviderTransport;
  configJson?: string;
  port?: number;
  /** Bind address (default `127.0.0.1` — loopback only). */
  host?: string;
  /** Bearer token for non-loopback auth. Required when `host` is non-loopback. */
  authToken?: string;
  /** Path to a TLS certificate file (enables HTTPS). */
  tlsCert?: string;
  /** Path to a TLS private key file (enables HTTPS). */
  tlsKey?: string;
  /** Ontology config JSON (loaded from --ontology). Served via ontology_read. */
  ontologyConfig?: unknown;
  /** Taxonomy config JSON (loaded from --taxonomy). Served via taxonomy_read. */
  taxonomyConfig?: unknown;
}

/** Builds the McpServer with the Module-2 tools registered against the facade. */
export function createMcpServer(
  transport: NativeProviderTransport,
  opts?: { ontology?: unknown; taxonomy?: unknown },
): McpServer {
  const server = new McpServer({ name: "engram", version: "0.1.0" });
  registerTools(server, transport, opts);
  return server;
}

function isLoopbackHost(host?: string): boolean {
  return !host || host === "127.0.0.1" || host === "localhost" || host === "::1";
}

/**
 * Starts the HTTP-MCP server. **Loopback** (default): behind the SDK's host +
 * origin guards (DNS-rebinding protection), no auth. **Non-loopback**: requires
 * `authToken` (Bearer-token check replaces the localhost guards); TLS recommended
 * via `tlsCert`/`tlsKey` or a reverse proxy. Returns the server for shutdown.
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

  const loopback = isLoopbackHost(opts.host);
  if (!loopback && !opts.authToken) {
    throw new Error(
      "engram-mcp-http: --auth-token is required when binding to a non-loopback host"
    );
  }

  const validateHost = loopback ? localhostHostValidation() : undefined;
  const validateOrigin = loopback ? localhostOriginValidation() : undefined;
  const port = opts.port ?? 3000;
  const host = opts.host ?? "127.0.0.1";

  const handler = async (req: IncomingMessage, res: ServerResponse): Promise<void> => {
    // Security gate: loopback → host/origin guards; non-loopback → Bearer auth.
    if (loopback) {
      if (validateHost && !validateHost(req, res)) return;
      if (validateOrigin && !validateOrigin(req, res)) return;
    } else if (opts.authToken) {
      if (req.headers.authorization !== `Bearer ${opts.authToken}`) {
        res.writeHead(401, { "Content-Type": "application/json" });
        res.end(
          JSON.stringify({
            jsonrpc: "2.0",
            error: { code: -32001, message: "unauthorized" }
          })
        );
        return;
      }
    }

    // Stateless: a fresh McpServer + transport per request.
    const t = new NodeStreamableHTTPServerTransport({ sessionIdGenerator: undefined });
    try {
      await createMcpServer(transport, {
        ontology: opts.ontologyConfig,
        taxonomy: opts.taxonomyConfig,
      }).connect(t);
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
  };

  const useTls = opts.tlsCert !== undefined && opts.tlsKey !== undefined;
  const httpServer = useTls
    ? createHttpsServer(
        {
          cert: readFileSync(opts.tlsCert!, "utf8"),
          key: readFileSync(opts.tlsKey!, "utf8")
        },
        handler
      )
    : createHttpServer(handler);

  httpServer.on("error", (err: Error) => {
    console.error(`engram-mcp-http: listen failed: ${err.message}`);
    process.exit(1);
  });

  await new Promise<void>((resolve) => {
    httpServer.listen(port, host, resolve);
  });
  return httpServer as Server;
}
