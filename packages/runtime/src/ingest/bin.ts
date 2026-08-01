#!/usr/bin/env node
import { runIngestFromArgs } from "./cli.js";

await runIngestFromArgs(process.argv.slice(2));
