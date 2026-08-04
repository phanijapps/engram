//! In-process knowledge-graph transport (entities / relationships / neighbors).
//! Used by the Graph tab + drill-down (S1 community overview, S2 drill). The
//! standalone engine takes a `dbPath` (the same single-file store the provider
//! opens); it lacks pagination + bulk counts, which the read-only `db/reader`
//! secondary path (T3) supplies.

import {
  createNativeKnowledgeTransport,
  type NativeKnowledgeTransport,
} from "@engram/node";

import { dbPath, type VizConfig } from "../config.ts";

let cached: NativeKnowledgeTransport | null = null;

export function getKnowledge(cfg: VizConfig): NativeKnowledgeTransport {
  if (cached) return cached;
  cached = createNativeKnowledgeTransport({ dbPath: dbPath(cfg) });
  return cached;
}

export function _resetKnowledgeForTests(): void {
  cached = null;
}
