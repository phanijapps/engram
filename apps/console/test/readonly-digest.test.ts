/**
 * TASK-015 focused proofs (GATE-004, AC-004): a full console session leaves
 * the store's content digest unchanged and issues no mutating engine call.
 *
 * Two legs, both required:
 *  1. REAL-store digest leg (runs when the warm sqlite recipe artifact
 *     exists): opens a real NativeProvider over a fresh sqlite store, writes
 *     a fixture through the engine write path, hashes the governed store
 *     files byte-for-byte, drives EVERY console GET endpoint, then re-hashes.
 *  2. Recording leg (always runs): a full session against a recording
 *     provider stub, asserting every engine call the console made is a read —
 *     no write/forget/put/bind/reset surface is ever touched.
 */
import { describe, expect, it, afterEach } from "vitest";
import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import { mkdtempSync, readdirSync, statSync, existsSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { createRequire } from "node:module";
import { createConsoleServer, partitionKey } from "../server/app.mjs";
import type { Server } from "node:http";

const require = createRequire(import.meta.url);

/** Loads the newest warm sqlite recipe artifact (same contract as the
 * @engram/node test helper's `loadRecipe`; local copy because the helper is
 * not an exported package surface). */
function loadWarmSqliteRecipe(): object | null {
  const dir = join(__dirname, "../../../packages/node/test/artifacts/sqlite");
  if (!existsSync(dir)) return null;
  const candidates = readdirSync(dir)
    .filter((name) => name.endsWith(".node"))
    .map((name) => join(dir, name))
    .sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs);
  for (const candidate of candidates) {
    try {
      return require(candidate);
    } catch {
      // try the next candidate
    }
  }
  return null;
}

/** SHA-256 of one file's bytes. */
function digestFile(path: string): Promise<string> {
  return new Promise((resolve, reject) => {
    const hash = createHash("sha256");
    createReadStream(path)
      .on("data", (chunk) => hash.update(chunk))
      .on("end", () => resolve(hash.digest("hex")))
      .on("error", reject);
  });
}

const READ_ONLY_METHODS = new Set([
  "capabilitiesJson",
  "codeStatusJson",
  "requireScopeInventoryApi"
]);

const READ_ONLY_INVENTORY_METHODS = new Set([
  "inventoryJson",
  "partitionMemoriesJson",
  "partitionEntitiesJson",
  "partitionRelationshipsJson",
  "partitionRecentMemoriesJson"
]);

const servers: Server[] = [];
afterEach(() => {
  while (servers.length > 0) servers.pop()?.close();
});

/** Drives EVERY console GET surface — the "full console session". */
async function driveFullSession(base: string, partition: Record<string, unknown>) {
  const key = partitionKey(partition as never);
  const paths = [
    "/api/health",
    "/api/inventory",
    "/api/anomalies",
    `/api/partitions/${key}/memories?limit=25&offset=0`,
    `/api/partitions/${key}/entities?limit=25&offset=0`,
    `/api/partitions/${key}/relationships?limit=25&offset=0`,
    `/api/partitions/${key}/recent?limit=20`,
    `/api/partitions/${key}/graph?limit=500`,
    `/api/partitions/${key}/binding-health?repo=mem-alpha`,
    "/"
  ];
  const codes: number[] = [];
  for (const path of paths) {
    const response = await fetch(base + path);
    codes.push(response.status);
    await response.arrayBuffer();
  }
  expect(codes.every((code) => code === 200)).toBe(true);
}

describe("job012 read-only guarantee (STEP-012)", () => {
  it("readonly-digest: a recording stub session issues ONLY read calls", async () => {
    const calls: { method: string }[] = [];
    const record = (method: string) => () => {
      calls.push({ method });
    };
    const provider = {
      capabilitiesJson: record("capabilitiesJson"),
      codeStatusJson: record("codeStatusJson"),
      requireScopeInventoryApi: () => {
        calls.push({ method: "requireScopeInventoryApi" });
        return {
          inventoryJson: () => {
            calls.push({ method: "inventoryJson" });
            return "[]";
          },
          partitionMemoriesJson: record("partitionMemoriesJson"),
          partitionEntitiesJson: record("partitionEntitiesJson"),
          partitionRelationshipsJson: record("partitionRelationshipsJson"),
          partitionRecentMemoriesJson: record("partitionRecentMemoriesJson")
        };
      }
    };
    const app = createConsoleServer({ provider, backend: "sqlite", storage: "/tmp/x" });
    const server = await app.listen(0, "127.0.0.1");
    servers.push(server);
    const address = server.address();
    if (typeof address === "string" || address === null) throw new Error("tcp expected");
    const partition = { tenant: "acme", subject: null, workspace: "proj", session: null, environment: null };
    await driveFullSession(`http://127.0.0.1:${address.port}`, partition);

    expect(calls.length).toBeGreaterThan(0);
    for (const call of calls) {
      expect(
        READ_ONLY_METHODS.has(call.method) || READ_ONLY_INVENTORY_METHODS.has(call.method),
        `console issued a non-read engine call: ${call.method}`
      ).toBe(true);
    }
  });

  it(
    "readonly-digest: a real-store session leaves the store bytes unchanged",
    { timeout: 120_000 },
    async () => {
      const addon = loadWarmSqliteRecipe() as
        | { NativeProvider: new (config: string) => unknown }
        | null;
      if (!addon) {
        throw new Error(
          "warm sqlite recipe artifact missing — run node packages/node/test/helpers/build-native-recipes.mjs first (this gate never silently skips)"
        );
      }
      const base = mkdtempSync(join(tmpdir(), "job012-readonly-"));
      const storage = join(base, "store");
      const config = JSON.stringify({
        storage_path: storage,
        trusted_root: base,
        scope_policy: "Strict",
        embedding_provider: { provider_type: "none", model: "none", dimensions: 384, prompt_profile: "query" },
        migration_mode: "Apply",
        capability_policy: "FailClosed",
        enable_vector: false
      });

      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const provider: any = new addon.NativeProvider(config);
      const partition = { tenant: "acme", subject: null, workspace: "proj", session: null, environment: null };
      // SETUP (before the session): write the fixture through the engine.
      const write = (text: string) =>
        provider.requireMemoryApi().writeJson(
          JSON.stringify({
            kind: "observation",
            content: { text },
            scope: partition,
            requester: {
              actor: { id: "job012", kind: "agent", displayName: "job012" },
              roles: [],
              permissions: ["memory.write"]
            },
            provenance: {
              source: "job012-readonly",
              actor: { id: "job012", kind: "agent", displayName: "job012" },
              observedAt: new Date().toISOString(),
              confidence: 1.0
            },
            policy: { visibility: "private", retention: "durable", sensitivity: "low", allowedUses: ["retrieval"] },
            links: []
          })
        );
      write("fixture memory one");
      write("fixture memory two");

      const storeFiles = ["memory.db", "knowledge.db", "belief.db"].map((name) => join(storage, name));
      const before = await Promise.all(storeFiles.map(digestFile));

      const app = createConsoleServer({ provider, backend: "sqlite", storage });
      const server = await app.listen(0, "127.0.0.1");
      servers.push(server);
      const address = server.address();
      if (typeof address === "string" || address === null) throw new Error("tcp expected");
      await driveFullSession(`http://127.0.0.1:${address.port}`, partition);

      const after = await Promise.all(storeFiles.map(digestFile));
      expect(after).toEqual(before);
    }
  );

  it("readonly-digest: the UI build carries no engine write import (structural)", () => {
    // The server module imports no engine write surface at all: its module
    // graph is createServer/fs/path/http only. Prove the shipped server file
    // references no mutating binding method names.
    const source = require("node:fs").readFileSync(join(__dirname, "../server/app.mjs"), "utf8");
    for (const forbidden of [
      "writeJson",
      "forgetJson",
      "putEntityJson",
      "putRelationshipJson",
      "upsertBeliefJson",
      "codeBindJson",
      "codeRegisterJson",
      "codeRebuildJson",
      "codeResetApplyJson",
      "scanRepositoryJson",
      "consolidateJson",
      "backfillEmbeddingsJson"
    ]) {
      expect(source).not.toContain(forbidden);
    }
  });
});

