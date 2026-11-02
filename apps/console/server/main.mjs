/**
 * Console entry point (job 012-admin-console-v1, STEP-007): parse the launch
 * flags (mirroring the engram-mcp vocabulary), open the engine through
 * `@engram/node`'s native binding, and serve the UI + JSON API on loopback.
 * `--open` (set by `pnpm --filter @engram/console start`) opens the browser —
 * the one-command, zero-CLI journey (REQ-006 / NFR-004).
 */
import { dirname, join, resolve } from "node:path";
import { homedir } from "node:os";
import { fileURLToPath } from "node:url";
import { spawn } from "node:child_process";
import { loadNativeBinding } from "@engram/node";
import { createConsoleServer } from "./app.mjs";

const here = dirname(fileURLToPath(import.meta.url));

function parseArgs(argv) {
  const flags = { storage: null, backend: "sqlite", port: 8788, open: false, pg: null };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    const next = argv[i + 1];
    if (arg === "--storage") {
      flags.storage = resolve(next);
      i += 1;
    } else if (arg === "--backend") {
      flags.backend = next;
      i += 1;
    } else if (arg === "--port") {
      flags.port = Number(next);
      i += 1;
    } else if (arg === "--pg-connection-string") {
      flags.pg = next;
      i += 1;
    } else if (arg === "--open") {
      flags.open = true;
    } else {
      console.error(`engram-console: unknown flag ${arg}`);
      console.error("usage: engram-console [--storage <path>] [--backend sqlite|pgvector] [--port N] [--pg-connection-string URL] [--open]");
      process.exit(2);
    }
  }
  if (flags.backend !== "sqlite" && flags.backend !== "pgvector") {
    console.error(`engram-console: unknown backend '${flags.backend}' (sqlite | pgvector)`);
    process.exit(2);
  }
  if (flags.backend === "pgvector" && !flags.pg) {
    console.error("engram-console: --backend pgvector requires --pg-connection-string");
    process.exit(2);
  }
  return flags;
}

function providerConfig(flags) {
  const storage = flags.storage ?? join(homedir(), ".engram");
  const base = {
    storage_path: storage,
    trusted_root: join(storage, ".."),
    scope_policy: "Strict",
    embedding_provider: {
      provider_type: "none",
      model: "none",
      dimensions: 384,
      prompt_profile: "query"
    },
    // A read-only console never migrates a store it did not create.
    migration_mode: "DryRun",
    capability_policy: "FailClosed",
    enable_vector: false
  };
  if (flags.backend === "pgvector") {
    base.pgvector_connection_string = flags.pg;
  }
  return { config: JSON.stringify(base), storage };
}

function openBrowser(url) {
  const opener = process.platform === "darwin" ? "open" : "xdg-open";
  const child = spawn(opener, [url], { stdio: "ignore" });
  child.on("error", () => {
    console.error(`engram-console: could not open a browser; open ${url} manually`);
  });
  child.unref();
}

async function main() {
  const flags = parseArgs(process.argv.slice(2));
  const binding = loadNativeBinding();
  const { config, storage } = providerConfig(flags);

  let provider;
  try {
    provider = new binding.NativeProvider(config);
  } catch (error) {
    console.error(`engram-console: engine open failed: ${error?.message ?? error}`);
    process.exit(1);
  }

  const uiDir = join(here, "..", "dist");
  const app = createConsoleServer({ provider, backend: flags.backend, storage, uiDir });
  const server = await app.listen(flags.port, "127.0.0.1");
  const { port } = server.address();
  const url = `http://127.0.0.1:${port}`;
  console.log(`engram-console: ${url} (backend=${flags.backend}, storage=${storage})`);
  console.log("engram-console: read-only partition console; Ctrl-C stops.");
  if (flags.open) {
    openBrowser(url);
  }
}

main().catch((error) => {
  console.error(`engram-console: ${error?.stack ?? error}`);
  process.exit(1);
});
