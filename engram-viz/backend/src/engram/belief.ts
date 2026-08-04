//! In-process belief + contradiction transport. Serves the Memory tab's
//! beliefs/contradictions sub-tabs (S3). Empty today (the agentzero store has
//! 0 beliefs) — the routes render honest empty-states.

import {
  createNativeBeliefTransport,
  type NativeBeliefTransport,
} from "@engram/node";

import { dbPath, type VizConfig } from "../config.ts";

let cached: NativeBeliefTransport | null = null;

export function getBelief(cfg: VizConfig): NativeBeliefTransport {
  if (cached) return cached;
  cached = createNativeBeliefTransport({ dbPath: dbPath(cfg) });
  return cached;
}

export function _resetBeliefForTests(): void {
  cached = null;
}
