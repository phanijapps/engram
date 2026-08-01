#!/usr/bin/env node
import { runIngestFromArgs } from "./cli.js";

const handle = await runIngestFromArgs(process.argv.slice(2));
if (handle) {
  // Periodic mode: Ctrl-C / terminate clears the interval + exits cleanly.
  // `process.once` so repeated runs (or a host) never accumulate listeners.
  const shutdown = (): never => {
    handle.stop();
    process.exit(0);
  };
  process.once("SIGINT", shutdown);
  process.once("SIGTERM", shutdown);
}
