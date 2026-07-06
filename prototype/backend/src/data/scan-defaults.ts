// Default scope / policy / actor for demo-local scan ingestion.
//
// Single source of truth shared by the HTTP /ingest/jobs route and the MCP
// tool path, so both send the ingest engine the same, complete request shape.
// The Rust engine requires a non-null Policy; supplying it here (not per-caller)
// keeps the two entrypoints from drifting.

export const SCAN_SCOPE = {
  tenant: "tenant-demo",
  workspace: "engram",
  environment: "local",
};

export const SCAN_POLICY = {
  visibility: "workspace",
  retention: "durable",
  sensitivity: "low",
  allowedUses: ["retrieval"],
  deleteMode: "tombstone",
};

export const SCAN_ACTOR = { id: "actor-demo", kind: "agent", displayName: "Demo Scan" };
