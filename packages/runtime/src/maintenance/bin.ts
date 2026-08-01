#!/usr/bin/env node
import { runMaintainFromArgs } from "./cli.js";

const handle = await runMaintainFromArgs(process.argv.slice(2));
if (handle) {
  // Periodic mode: Ctrl-C / terminate clears the interval + exits cleanly.
  const shutdown = (): never => {
    handle.stop();
    process.exit(0);
  };
  process.once("SIGINT", shutdown);
  process.once("SIGTERM", shutdown);
}
