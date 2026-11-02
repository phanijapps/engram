/**
 * Console server core (job 012-admin-console-v1, STEP-007).
 *
 * A loopback-only HTTP server that serves the built UI plus a private JSON
 * API over the engine's read surfaces. The ONLY data access is the
 * `@engram/node` provider handle supplied by the caller — this file holds no
 * engine driver and no store query text (GATE-010's scan enforces that).
 *
 * Security posture (REQ-007 / AC-011): binds 127.0.0.1 only, single trusted
 * operator, deliberately NO authentication or authorization surface (B-1,
 * LED-009). Read-only by construction: every engine call this module can make
 * is a read.
 */
import { createServer } from "node:http";
import { createReadStream, existsSync } from "node:fs";
import { extname, join, normalize } from "node:path";
import { readFile } from "node:fs/promises";

/** Hosts the console may bind. Loopback only — anything else is a defect. */
export const LOOPBACK_HOSTS = new Set(["127.0.0.1", "localhost", "::1"]);

/**
 * Throws unless `host` is a loopback address. Exported so tests can pin the
 * negative (any non-loopback bind is a defect, REQ-007).
 */
export function assertLoopbackHost(host) {
  if (!LOOPBACK_HOSTS.has(host)) {
    throw new Error(`refusing to bind non-loopback host: ${host}`);
  }
}

/** URL-safe encoding of one partition tuple (the five scope values). */
export function partitionKey(scope) {
  return Buffer.from(JSON.stringify(scope), "utf8").toString("base64url");
}

/** Decodes a partition key back into the partition tuple. */
export function parsePartitionKey(key) {
  const scope = JSON.parse(Buffer.from(key, "base64url").toString("utf8"));
  if (typeof scope?.tenant !== "string" || scope.tenant.length === 0) {
    throw new Error("partition key must encode a scope with a tenant");
  }
  return scope;
}

/**
 * Maps an engine error string to a named degraded state (NFR-005): degraded
 * inputs render as explicit states, never a crash and never a partial
 * inventory. The classifier is deliberately string-shape based — the console
 * sees engine errors as N-API error text, not typed values.
 */
export function classifyDegradedState(errorText) {
  const text = String(errorText ?? "");
  if (/locked|busy|SQLITE_BUSY/i.test(text)) {
    return { state: "store-locked", message: "The store is locked or busy; retry shortly." };
  }
  if (/connection refused|unreachable|timed out|connect|tls|dns/i.test(text)) {
    return { state: "backend-unreachable", message: "The configured backend is unreachable." };
  }
  if (/unable to open database file|not a database|no such table|does not exist/i.test(text)) {
    return { state: "store-invalid", message: "The store directory does not hold a valid store." };
  }
  if (/configuration validation failed/i.test(text)) {
    return { state: "configuration-invalid", message: "The console configuration is invalid." };
  }
  return { state: "engine-error", message: text };
}

/**
 * The minimal documented anomaly set (REQ-010, LED-003 Show tier), computed
 * ONLY from engine reads: (1) the same subject value present in more than one
 * partition; (2) the same entity identity (name) present in more than one
 * partition. STRICTLY read-only: this function performs no writes and offers
 * no actions; repair, prune, and enforcement are out of scope for v1 (W-001).
 *
 * Bounds: subject anomalies derive from the inventory summary directly;
 * entity anomalies scan up to `maxPartitions` partitions with one bounded
 * entity page each — the same bounded-summary doctrine as the inventory.
 */
export async function detectAnomalies(api, partitions, { maxPartitions = 200 } = {}) {
  const anomalies = [];

  // (1) same subject under multiple partitions (tenant-scoped: a subject is
  // an identity inside its tenant).
  const bySubject = new Map();
  for (const partition of partitions) {
    const subject = partition.scope?.subject ?? null;
    if (!subject) continue;
    const tenant = partition.scope.tenant;
    const groupKey = `${tenant}||${subject}`;
    if (!bySubject.has(groupKey)) bySubject.set(groupKey, []);
    bySubject.get(groupKey).push(partition.scope);
  }
  for (const [groupKey, scopes] of bySubject) {
    if (scopes.length > 1) {
      const [tenant, subject] = groupKey.split("||");
      anomalies.push({
        kind: "subject-partitions",
        title: `Subject '${subject}' appears in ${scopes.length} partitions`,
        tenant,
        subject,
        partitions: scopes,
        partitionCount: scopes.length
      });
    }
  }

  // (2) same entity identity (name) under multiple partitions.
  const entityPartitions = new Map();
  const scanned = partitions.slice(0, maxPartitions);
  for (const partition of scanned) {
    let page = null;
    try {
      page = JSON.parse(
        api.partitionEntitiesJson(
          JSON.stringify({ partition: partition.scope, page: { limit: 500, offset: 0 } })
        )
      );
    } catch {
      continue; // a partition that cannot be listed is not an anomaly signal
    }
    for (const entity of page.items ?? []) {
      const name = entity.name;
      if (!name) continue;
      if (!entityPartitions.has(name)) entityPartitions.set(name, []);
      const seen = entityPartitions.get(name);
      if (!seen.some((scope) => JSON.stringify(scope) === JSON.stringify(partition.scope))) {
        seen.push(partition.scope);
      }
    }
  }
  for (const [name, scopes] of entityPartitions) {
    if (scopes.length > 1) {
      anomalies.push({
        kind: "entity-identity",
        title: `Entity '${name}' appears in ${scopes.length} partitions`,
        name,
        partitions: scopes,
        partitionCount: scopes.length
      });
    }
  }
  return anomalies;
}

const MIME = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".svg": "image/svg+xml",
  ".json": "application/json; charset=utf-8",
  ".png": "image/png",
  ".ico": "image/x-icon"
};

/**
 * Builds the console server.
 *
 * @param {object} options
 * @param {object} options.provider a `NativeProviderBinding`-shaped handle
 *   (the real addon in production; a stub in tests) — the console's only
 *   engine surface.
 * @param {string} options.backend "sqlite" | "pgvector" (reported, never used
 *   for data access here).
 * @param {string} options.storage the store path (reported).
 * @param {string} [options.uiDir] built UI directory; when absent the server
 *   serves a named "ui-not-built" state page.
 */
export function createConsoleServer({ provider, backend, storage, uiDir }) {
  if (!provider) throw new Error("createConsoleServer: a provider handle is required");

  const inventoryApi = () => provider.requireScopeInventoryApi();

  function sendJson(res, status, body) {
    const payload = JSON.stringify(body);
    res.writeHead(status, {
      "content-type": "application/json; charset=utf-8",
      "content-length": Buffer.byteLength(payload),
      // Local single-operator surface; the UI is same-origin — no sharing.
      "cache-control": "no-store"
    });
    res.end(payload);
  }

  function readBody(req) {
    return new Promise((resolve, reject) => {
      let data = "";
      req.on("data", (chunk) => {
        data += chunk;
        if (data.length > 1 << 20) reject(new Error("request body too large"));
      });
      req.on("end", () => resolve(data));
      req.on("error", reject);
    });
  }

  async function handleApi(req, res, url) {
    // Health: always answers, names the session's backend + store.
    if (req.method === "GET" && url.pathname === "/api/health") {
      return sendJson(res, 200, {
        ok: true,
        backend,
        storage,
        ui: uiDir && existsSync(uiDir) ? "built" : "ui-not-built"
      });
    }

    if (req.method === "GET" && url.pathname === "/api/inventory") {
      try {
        const partitions = JSON.parse(inventoryApi().inventoryJson());
        return sendJson(res, 200, {
          state: "ready",
          backend,
          partitionCount: partitions.length,
          partitions
        });
      } catch (error) {
        const degraded = classifyDegradedState(error?.message ?? error);
        return sendJson(res, 200, { state: degraded.state, message: degraded.message, partitions: [] });
      }
    }

    if (req.method === "GET" && url.pathname === "/api/anomalies") {
      try {
        const api = inventoryApi();
        const partitions = JSON.parse(api.inventoryJson());
        const anomalies = await detectAnomalies(api, partitions);
        return sendJson(res, 200, { state: "ready", anomalies, anomalyCount: anomalies.length });
      } catch (error) {
        const degraded = classifyDegradedState(error?.message ?? error);
        return sendJson(res, 200, { state: degraded.state, message: degraded.message, anomalies: [] });
      }
    }

    // Partition-scoped reads: /api/partitions/<key>/(memories|entities|relationships|recent|graph)
    const partitionMatch = url.pathname.match(/^\/api\/partitions\/([A-Za-z0-9_-]+)\/([a-z]+)$/);
    if (req.method === "GET" && partitionMatch) {
      const [, key, view] = partitionMatch;
      let partition;
      try {
        partition = parsePartitionKey(key);
      } catch {
        return sendJson(res, 400, { state: "bad-request", message: "malformed partition key" });
      }
      const limit = Number(url.searchParams.get("limit") ?? 50);
      const offset = Number(url.searchParams.get("offset") ?? 0);
      const api = inventoryApi();
      const request = JSON.stringify({ partition, page: { limit, offset } });
      try {
        if (view === "memories") {
          return sendJson(res, 200, JSON.parse(api.partitionMemoriesJson(request)));
        }
        if (view === "entities") {
          return sendJson(res, 200, JSON.parse(api.partitionEntitiesJson(request)));
        }
        if (view === "relationships") {
          return sendJson(res, 200, JSON.parse(api.partitionRelationshipsJson(request)));
        }
        if (view === "recent") {
          return sendJson(res, 200, {
            items: JSON.parse(api.partitionRecentMemoriesJson(JSON.stringify({ partition, limit })))
          });
        }
        if (view === "graph") {
          // The partition-confined graph view: entities + relationships of
          // exactly this partition, joined client-side into nodes/links. The
          // page envelopes unwrap to plain arrays (bounded by the page limit).
          const entities = JSON.parse(api.partitionEntitiesJson(request));
          const relationships = JSON.parse(api.partitionRelationshipsJson(request));
          return sendJson(res, 200, {
            entities: entities.items ?? [],
            relationships: relationships.items ?? []
          });
        }
        return sendJson(res, 404, { state: "not-found", message: `unknown view: ${view}` });
      } catch (error) {
        const degraded = classifyDegradedState(error?.message ?? error);
        return sendJson(res, 200, { state: degraded.state, message: degraded.message });
      }
    }

    return sendJson(res, 404, { state: "not-found", message: url.pathname });
  }

  async function serveUi(res, pathname) {
    if (!uiDir || !existsSync(uiDir)) {
      res.writeHead(200, { "content-type": MIME[".html"] });
      res.end(
        `<!doctype html><title>engram console</title><body style="font-family:system-ui;padding:2rem">` +
          `<h1>engram console</h1><p><strong>State: ui-not-built.</strong> Run <code>pnpm --filter @engram/console build</code> ` +
          `then restart. The JSON API is live at <code>/api/health</code>.</p></body>`
      );
      return;
    }
    const relative = pathname === "/" ? "index.html" : pathname.slice(1);
    const candidate = normalize(join(uiDir, relative));
    if (!candidate.startsWith(normalize(uiDir))) {
      res.writeHead(403).end("forbidden");
      return;
    }
    const file = existsSync(candidate) ? candidate : join(uiDir, "index.html");
    const type = MIME[extname(file)] ?? "application/octet-stream";
    res.writeHead(200, { "content-type": type });
    createReadStream(file).pipe(res);
  }

  const server = createServer(async (req, res) => {
    const url = new URL(req.url ?? "/", "http://127.0.0.1");
    try {
      if (req.method === "POST" && url.pathname === "/api/shutdown") {
        await readBody(req);
        sendJson(res, 200, { ok: true });
        server.close();
        return;
      }
      if (url.pathname.startsWith("/api/")) {
        await handleApi(req, res, url);
        return;
      }
      await serveUi(res, url.pathname);
    } catch (error) {
      sendJson(res, 500, { state: "engine-error", message: String(error?.message ?? error) });
    }
  });

  /** The single approved listen path: loopback only (REQ-007). The guard
   * runs inside the promise so a non-loopback bind surfaces as a rejection
   * for every caller shape. */
  function listen(port, host = "127.0.0.1") {
    return new Promise((resolve, reject) => {
      try {
        assertLoopbackHost(host);
      } catch (error) {
        reject(error);
        return;
      }
      server.once("error", reject);
      server.listen(port, host, () => resolve(server));
    });
  }

  return { server, listen };
}
