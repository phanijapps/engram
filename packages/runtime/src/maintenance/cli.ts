import { parseArgs } from "node:util";

import type { Scope } from "@engram/contracts";
import { createNativeProviderTransport, type NativeProviderTransport } from "@engram/node";

import { buildEngramConfig, buildScope } from "../shared/config.js";

/** Parsed `engram-maintain` flags. */
export interface MaintainArgs {
  config: string;
  tenant: string;
  workspace?: string;
  dryRun?: boolean;
  since?: string;
  until?: string;
  /** Interval in milliseconds; `undefined`/`<= 0` → one-shot. */
  every?: number;
}

/** Options for [`runMaintain`]. The transport is injected so tests need no addon. */
export interface MaintainOptions {
  transport: NativeProviderTransport;
  scope: Scope;
  dryRun?: boolean;
  since?: string;
  until?: string;
  every?: number;
}

/** Handle returned in periodic mode so callers (tests) can stop the schedule. */
export interface MaintainHandle {
  stop: () => void;
}

/**
 * A consolidation run result. The facade types `consolidate` as `Promise<unknown>`;
 * `tasks` is `#[serde(skip_serializing_if = "Vec::is_empty")]`
 * (`core/domain/src/operations.rs`), so an empty corpus omits it — hence optional.
 */
export interface ConsolidationRun {
  status: string;
  tasks?: unknown[];
}

/** Parses `engram-maintain` argv via `node:util/parseArgs` (Node 22 stdlib). */
export function parseMaintainArgs(argv: string[]): MaintainArgs {
  const { values } = parseArgs({
    options: {
      config: { type: "string" },
      tenant: { type: "string" },
      workspace: { type: "string" },
      "dry-run": { type: "boolean" },
      since: { type: "string" },
      until: { type: "string" },
      every: { type: "string" }
    },
    args: argv,
    strict: true
  });

  for (const req of ["config", "tenant"] as const) {
    if (values[req] === undefined) {
      throw new Error(`engram-maintain: --${req} is required`);
    }
  }

  const rawEvery = values.every;
  const every = rawEvery !== undefined ? Number(rawEvery) : undefined;
  if (rawEvery !== undefined && !/^\d+$/.test(rawEvery)) {
    throw new Error(
      `engram-maintain: --every must be a non-negative integer (ms), got: ${rawEvery}`
    );
  }

  return {
    config: values.config!,
    tenant: values.tenant!,
    ...(values.workspace !== undefined ? { workspace: values.workspace } : {}),
    ...(values["dry-run"] ? { dryRun: true } : {}),
    ...(values.since !== undefined ? { since: values.since } : {}),
    ...(values.until !== undefined ? { until: values.until } : {}),
    ...(every !== undefined ? { every } : {})
  };
}

/**
 * Runs consolidation (reflection + decay) over the held-provider facade. One-shot
 * when `every` is unset or `<= 0`; otherwise schedules on `setInterval` (cron-like
 * — first run at `every` ms). Errors propagate in one-shot mode and are logged +
 * swallowed in periodic mode (the schedule survives).
 *
 * Library function: no signal handlers / `process.exit` — the bin wires those.
 * Returns a {@link MaintainHandle} in periodic mode.
 */
export async function runMaintain(
  opts: MaintainOptions
): Promise<MaintainHandle | void> {
  const run = async (): Promise<void> => {
    try {
      const result = (await opts.transport.consolidate({
        scope: opts.scope,
        ...(opts.dryRun !== undefined ? { dryRun: opts.dryRun } : {}),
        ...(opts.since !== undefined ? { since: opts.since } : {}),
        ...(opts.until !== undefined ? { until: opts.until } : {})
      })) as ConsolidationRun;
      process.stdout.write(`${JSON.stringify(result)}\n`);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      if (!opts.every || opts.every <= 0) {
        throw error; // one-shot: propagate (bin exits non-zero)
      }
      process.stderr.write(`engram-maintain: consolidate failed: ${message}\n`);
    }
  };

  if (!opts.every || opts.every <= 0) {
    await run();
    return;
  }

  const interval = setInterval(() => {
    void run();
  }, opts.every);
  return { stop: () => clearInterval(interval) };
}

/** Parses argv, builds config + scope, constructs the transport, and runs.
 *  Returns the handle in periodic mode so the bin can wire signals. */
export async function runMaintainFromArgs(
  argv: string[]
): Promise<MaintainHandle | void> {
  const args = parseMaintainArgs(argv);
  const configJson = buildEngramConfig(args.config);
  const scope = buildScope({
    tenant: args.tenant,
    ...(args.workspace !== undefined ? { workspace: args.workspace } : {})
  });
  const transport = createNativeProviderTransport({ configJson });
  return runMaintain({
    transport,
    scope,
    ...(args.dryRun !== undefined ? { dryRun: args.dryRun } : {}),
    ...(args.since !== undefined ? { since: args.since } : {}),
    ...(args.until !== undefined ? { until: args.until } : {}),
    ...(args.every !== undefined ? { every: args.every } : {})
  });
}
