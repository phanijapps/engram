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
import { createServer as createProbeServer } from "node:net";
import { existsSync } from "node:fs";
import { loadNativeBinding } from "@engram/node";
import { createConsoleServer } from "./app.mjs";

const here = dirname(fileURLToPath(import.meta.url));

function parseArgs(argv) {
  const flags = {
    storage: null,
    backend: "sqlite",
    port: 8788,
    open: false,
    pg: null,
    layout: "single",
    dbFile: "engram_data.db"
  };
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
    } else if (arg === "--layout") {
      flags.layout = next;
      i += 1;
    } else if (arg === "--db-file") {
      flags.dbFile = next;
      i += 1;
    } else if (arg === "--open") {
      flags.open = true;
    } else {
      console.error(`engram-console: unknown flag ${arg}`);
      console.error(
        "usage: engram-console [--storage <path>] [--backend sqlite|pgvector] [--port N] " +
          "[--layout single|multi] [--db-file NAME] " +
          "[--pg-connection-string URL | ENGRAM_PG_CONNECTION_STRING] [--open]"
      );
      process.exit(2);
    }
  }
  if (flags.backend !== "sqlite" && flags.backend !== "pgvector") {
    console.error(`engram-console: unknown backend '${flags.backend}' (sqlite | pgvector)`);
    process.exit(2);
  }
  if (flags.layout !== "single" && flags.layout !== "multi") {
    console.error(`engram-console: unknown layout '${flags.layout}' (single | multi)`);
    process.exit(2);
  }
  if (flags.backend === "pgvector" && !flags.pg && !process.env.ENGRAM_PG_CONNECTION_STRING) {
    console.error(
      "engram-console: --backend pgvector requires --pg-connection-string " +
        "(or the ENGRAM_PG_CONNECTION_STRING environment variable)"
    );
    process.exit(2);
  }
  return flags;
}

function providerConfig(flags, storage) {
  const base = {
    storage_path: storage,
    // Mirror the engram-mcp vocabulary: single layout + db-file default open
    // the same one-file store the CLI opens (e.g. engram_data.db); the engine
    // provider's own default is multi-file, which would open empty per-domain
    // databases beside a single-file store.
    sqlite_storage_layout:
      flags.layout === "single"
        ? { kind: "single_file", file_name: flags.dbFile }
        : { kind: "multi_file_directory" },
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
    // Flag first, env var fallback (FIND-sec-003): keeping the URL out of
    // process listings and shell history mirrors the engram-mcp convention.
    base.pgvector_connection_string = flags.pg ?? process.env.ENGRAM_PG_CONNECTION_STRING;
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

/** Early loopback port probe (FIND-ops-005): named port-in-use state with a
 * distinct exit code BEFORE the engine loads, so the diagnosis never depends
 * on native-addon availability. The probe closes before the real listen. */
function assertPortAvailable(port) {
  return new Promise((resolve, reject) => {
    const probe = createProbeServer();
    probe.once("error", (error) => {
      if (String(error?.code ?? "").includes("EADDRINUSE")) {
        console.error(
          `engram-console: state: port-in-use — 127.0.0.1:${port} is already bound. ` +
            "Stop the other listener or pass --port <N>."
        );
        process.exit(4);
      }
      reject(error);
    });
    probe.listen(port, "127.0.0.1", () => probe.close(() => resolve()));
  });
}

async function main() {
  const flags = parseArgs(process.argv.slice(2));
  await assertPortAvailable(flags.port);
  const storage = flags.storage ?? join(homedir(), ".engram");
  // Degraded-state pre-check (NFR-005): a missing store directory is a named
  // state, never a silently-created store — a read-only console must not
  // materialize data directories (REQ-004 spirit; AC-013 absent case).
  if (flags.backend === "sqlite" && !existsSync(storage)) {
    console.error(
      `engram-console: state: store-absent — storage directory '${storage}' does not exist. ` +
        "The console is read-only and never creates a store; pass the path of an existing store."
    );
    process.exit(3);
  }
  const binding = loadNativeBinding();
  const { config } = providerConfig(flags, storage);

  let provider;
  try {
    provider = new binding.NativeProvider(config);
  } catch (error) {
    console.error(`engram-console: engine open failed: ${error?.message ?? error}`);
    process.exit(1);
  }

  const uiDir = join(here, "..", "dist");
  const app = createConsoleServer({ provider, backend: flags.backend, storage, uiDir });
  let server;
  try {
    server = await app.listen(flags.port, "127.0.0.1");
  } catch (error) {
    // Named port-collision state (FIND-ops-005), distinct exit code.
    if (String(error?.code ?? error?.message ?? error).includes("EADDRINUSE")) {
      console.error(
        `engram-console: state: port-in-use — 127.0.0.1:${flags.port} is already bound. ` +
          "Stop the other listener or pass --port <N>."
      );
      process.exit(4);
    }
    throw error;
  }
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
