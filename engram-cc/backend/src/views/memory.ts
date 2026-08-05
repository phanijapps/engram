//! Memory-surface projections — pick the UI-facing fields from engram memory /
//! belief / procedure records (the `record_json` shapes). Beliefs/procedures are
//! empty in the agentzero store today, so their projections are minimal + exercised
//! only by the empty-state path until those surfaces are populated.

import type { BeliefView, MemoryView, ProcedureView } from "./types.ts";

interface MemoryRecord {
  id: string;
  kind?: string;
  content?: { text?: string };
  status?: string;
  createdAt?: string;
  provenance?: { source?: string; confidence?: number };
}

export function projectMemory(record: unknown): MemoryView {
  const r = record as MemoryRecord;
  return {
    id: r.id,
    kind: r.kind ?? "memory",
    text: r.content?.text ?? "",
    status: r.status,
    createdAt: r.createdAt,
    source: r.provenance?.source,
    confidence: r.provenance?.confidence,
  };
}

export function projectBelief(record: unknown): BeliefView {
  const r = record as { id: string; claim?: string; statement?: string; subject?: string; status?: string };
  return {
    id: r.id,
    text: r.claim ?? r.statement,
    subject: r.subject,
    status: r.status,
  };
}

export function projectProcedure(record: unknown): ProcedureView {
  const r = record as { id: string; content?: { text?: string }; body?: string; steps?: unknown[] };
  return { id: r.id, text: r.content?.text ?? r.body ?? (Array.isArray(r.steps) ? `${r.steps.length} steps` : "") };
}
