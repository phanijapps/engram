import { parseArgs } from "node:util";

import type { Scope } from "@engram/contracts";
import { createNativeProviderTransport, type NativeProviderTransport } from "@engram/node";

import { buildEngramConfig, buildScope, type ScanSummary } from "../shared/config.js";

/** Parsed `engram-ingest` flags. */
export interface IngestArgs {
  config: string;
  path: string;
  tenant: string;
  workspace?: string;
  /** Interval in milliseconds; `undefined`/`<= 0` → one-shot. */
  every?: number;
}

/** Options for [`runIngest`]. The transport is injected so tests need no addon. */
export interface IngestOptions {
  transport: NativeProviderTransport;
  path: string;
  scope: Scope;
  every?: number;
}

/** Handle returned in periodic mode so callers (tests) can stop the schedule. */
export interface IngestHandle {
  stop: () => void;
}

/** Parses `engram-ingest` argv via `node:util/parseArgs` (Node 22 stdlib). */
export function parseIngestArgs(argv: string[]): IngestArgs {
  const { values } = parseArgs({
    options: {
      config: { type: "string" },
      path: { type: "string" },
      tenant: { type: "string" },
      workspace: { type: "string" },
      every: { type: "string" }
    },
    args: argv,
    strict: true
  });

  for (const req of ["config", "path", "tenant"] as const) {
    if (values[req] === undefined) {
      throw new Error(`engram-ingest: --${req} is required`);
    }
  }

  const rawEvery = values.every;
  const every = rawEvery !== undefined ? Number(rawEvery) : undefined;
  if (rawEvery !== undefined && !/^\d+$/.test(rawEvery)) {
    throw new Error(
      `engram-ingest: --every must be a non-negative integer (ms), got: ${rawEvery}`
    );
  }

  return {
    config: values.config!,
    path: values.path!,
    tenant: values.tenant!,
    ...(values.workspace !== undefined ? { workspace: values.workspace } : {}),
    ...(every !== undefined ? { every } : {})
  };
}

/**
 * Runs ingestion over the held-provider facade. One-shot when `every` is unset or
 * `<= 0`; otherwise schedules on `setInterval` (cron-like — first run at `every`
 * ms, not immediately). Scan errors propagate in one-shot mode and are logged +
 * swallowed in periodic mode (the schedule survives).
 *
 * This is a library function: it does NOT install signal handlers or call
 * `process.exit` (those would hijack a host process). In periodic mode it
 * returns an {@link IngestHandle} whose `stop()` clears the interval; the bin
 * wires SIGINT/SIGTERM to `stop()` + exit.
 */
export async function runIngest(
  opts: IngestOptions
): Promise<IngestHandle | void> {
  const scan = async (): Promise<void> => {
    try {
      const summary = (await opts.transport.scan({
        path: opts.path,
        scope: opts.scope
      })) as ScanSummary;
      process.stdout.write(`${JSON.stringify(summary)}\n`);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      if (!opts.every || opts.every <= 0) {
        throw error; // one-shot: propagate (bin exits non-zero)
      }
      process.stderr.write(`engram-ingest: scan failed: ${message}\n`);
    }
  };

  // One-shot: run once + return.
  if (!opts.every || opts.every <= 0) {
    await scan();
    return;
  }

  // Periodic: schedule (no immediate run). No signal handling here — see the bin.
  const interval = setInterval(() => {
    void scan();
  }, opts.every);
  return { stop: () => clearInterval(interval) };
}

/** Parses argv, builds config + scope, constructs the transport, and runs.
 *  Returns the {@link IngestHandle} in periodic mode so the bin can wire signals. */
export async function runIngestFromArgs(
  argv: string[]
): Promise<IngestHandle | void> {
  const args = parseIngestArgs(argv);
  const configJson = buildEngramConfig(args.config);
  const scope = buildScope({
    tenant: args.tenant,
    ...(args.workspace !== undefined ? { workspace: args.workspace } : {})
  });
  const transport = createNativeProviderTransport({ configJson });
  return runIngest({
    transport,
    path: args.path,
    scope,
    ...(args.every !== undefined ? { every: args.every } : {})
  });
}
