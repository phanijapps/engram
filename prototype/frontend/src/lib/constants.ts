// Shared demo-local defaults (single-user, local). Imported by every route so
// scope/policy/actor live in exactly one place.

export const SCOPE = {
  tenant: "tenant-demo",
  workspace: "engram",
  environment: "local",
};

export const POLICY = {
  visibility: "workspace",
  retention: "durable",
  sensitivity: "low",
  allowedUses: ["retrieval"],
  deleteMode: "tombstone",
};

export const ACTOR = {
  id: "actor-demo",
  kind: "agent" as const,
  displayName: "Demo User",
};

/** Shared requester envelope for memory retrieve/write/forget. */
export const requester = (permission: string) => ({
  actor: ACTOR,
  roles: ["maintainer"],
  permissions: [permission],
});
