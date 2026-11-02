/**
 * TASK-005 differential leg (job 012): one recipe, one process. Seeds the
 * shared parity fixture through the engine's own write surfaces and prints
 * one JSON line of inventory + partition-view reads. Spawned as a child
 * process by `scope-inventory-parity.test.ts` so the sqlite and pgvector
 * recipe addons never load into one process (one addon per process is the
 * recipe-isolation rule; two copies of one napi cdylib in a process
 * interpose class symbols and crash).
 *
 * argv: <recipe> <nonce> — the nonce is the run-unique tenant used by BOTH
 * legs, so the fixture is identical across recipes and never collides with
 * rows persisted by earlier runs on the shared disposable Postgres.
 */
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { loadRecipe } from "./build-native-recipes.mjs";

const [recipe, nonce] = process.argv.slice(2);
if (!recipe || !nonce) {
  console.error("usage: node scope-inventory-leg.mjs <recipe> <nonce>");
  process.exit(2);
}

const addon = loadRecipe(recipe);

const extra = {};
if (recipe === "pgvector") {
  const url = process.env.ENGRAM_TEST_PG_URL;
  if (!url) {
    console.error("ENGRAM_TEST_PG_URL is required for the pgvector parity leg");
    process.exit(2);
  }
  extra.pgvector_connection_string = url;
}
const base = mkdtempSync(join(tmpdir(), `job012-parity-${recipe}-`));
const config = JSON.stringify({
  storage_path: join(base, "store"),
  trusted_root: base,
  scope_policy: "Strict",
  embedding_provider: {
    provider_type: "none",
    model: "none",
    dimensions: 384,
    prompt_profile: "query"
  },
  migration_mode: "Apply",
  capability_policy: "FailClosed",
  enable_vector: false,
  ...extra
});

const provider = new addon.NativeProvider(config);
const memory = provider.requireMemoryApi();
const graph = provider.requireGraphApi();
const beliefs = provider.requireBeliefsApi();
const inventory = provider.requireScopeInventoryApi();

const TENANT = nonce;
const OTHER_TENANT = `${nonce}-c`;
const NOW = new Date().toISOString();
const actor = { id: "job012-parity", kind: "agent", displayName: "job012" };

/** The exact partition tuples of the fixture. */
const partitionA = { tenant: TENANT, subject: "agent-1", workspace: "proj-a", session: null, environment: "prod" };
const partitionUnset = { tenant: TENANT, subject: "agent-1", workspace: null, session: null, environment: "prod" };
const partitionOtherTenant = { tenant: OTHER_TENANT, subject: "agent-1", workspace: "proj-a", session: null, environment: "prod" };

function writeMemory(text, scope) {
  memory.writeJson(
    JSON.stringify({
      kind: "observation",
      content: { text },
      scope,
      requester: { actor, roles: [], permissions: ["memory.write", "memory.retrieve"] },
      provenance: { source: "job012-parity", actor, observedAt: NOW, confidence: 1.0 },
      policy: { visibility: "private", retention: "durable", sensitivity: "low", allowedUses: ["retrieval"] },
      links: []
    })
  );
}

function putEntity(id, name, scope) {
  graph.putEntityJson(
    JSON.stringify({
      id,
      kind: "project",
      name,
      aliases: [],
      scope,
      sourceRefs: [],
      conceptRefs: [],
      ontologyClassRefs: [],
      provenance: { source: "job012-parity", actor, observedAt: NOW, confidence: 1.0 },
      createdAt: NOW
    })
  );
}

function putRelationship(id, subjectId, objectId, scope) {
  graph.putRelationshipJson(
    JSON.stringify({
      id,
      subject: { id: subjectId, name: null, kind: null, aliases: [] },
      predicate: "relates-to",
      object: { id: objectId, name: null, kind: null, aliases: [] },
      scope,
      evidence: [],
      provenance: { source: "job012-parity", actor, observedAt: NOW, confidence: 1.0 },
      createdAt: NOW
    })
  );
}

function putBelief(id, scope) {
  beliefs.upsertBeliefJson(
    JSON.stringify({
      id,
      scope,
      subject: { key: `belief-${id}`, aliases: [] },
      content: `belief ${id}`,
      status: "active",
      confidence: 0.9,
      sources: [],
      policy: { visibility: "private", retention: "durable", sensitivity: "low", allowedUses: ["retrieval"] },
      provenance: { source: "job012-parity", actor, observedAt: NOW, confidence: 1.0 },
      createdAt: NOW
    })
  );
}

function listing(api, partition) {
  return JSON.parse(
    api.partitionMemoriesJson(JSON.stringify({ partition, page: { limit: 50, offset: 0 } }))
  ).items.map((m) => m.content.text);
}

function entityIds(api, partition) {
  return JSON.parse(
    api.partitionEntitiesJson(JSON.stringify({ partition, page: { limit: 50, offset: 0 } }))
  ).items.map((e) => e.id);
}

function relationshipIds(api, partition) {
  return JSON.parse(
    api.partitionRelationshipsJson(JSON.stringify({ partition, page: { limit: 50, offset: 0 } }))
  ).items.map((r) => r.id);
}

function recentTexts(api, partition, limit) {
  return JSON.parse(
    api.partitionRecentMemoriesJson(JSON.stringify({ partition, limit }))
  ).map((m) => m.content.text);
}

// --- the shared fixture (identical inputs for every recipe) ---------------
// Partition A: 3 memories, 2 entities, 1 relationship, 2 beliefs.
writeMemory("a-memory-1", partitionA);
writeMemory("a-memory-2", partitionA);
writeMemory("a-memory-3", partitionA);
putEntity(`${nonce}-ent-a-1`, "Entity A1", partitionA);
putEntity(`${nonce}-ent-a-2`, "Entity A2", partitionA);
putRelationship(`${nonce}-rel-a-1`, `${nonce}-ent-a-1`, `${nonce}-ent-a-2`, partitionA);
putBelief(`${nonce}-bel-a-1`, partitionA);
putBelief(`${nonce}-bel-a-2`, partitionA);
// "(unset)"-workspace partition: 2 memories, 1 entity.
writeMemory("u-memory-1", partitionUnset);
writeMemory("u-memory-2", partitionUnset);
putEntity(`${nonce}-ent-u-1`, "Entity U1", partitionUnset);
// Same workspace under a different tenant: a distinct partition.
writeMemory("c-memory-1", partitionOtherTenant);

// --- the shared reads ------------------------------------------------------
const tenants = [TENANT, OTHER_TENANT];
const rows = JSON.parse(inventory.inventoryJson()).filter((row) =>
  tenants.includes(row.scope.tenant)
);

const result = {
  recipe,
  tenant: TENANT,
  inventory: rows,
  listingA: listing(inventory, partitionA),
  listingUnset: listing(inventory, partitionUnset),
  listingOtherTenant: listing(inventory, partitionOtherTenant),
  entitiesA: entityIds(inventory, partitionA),
  entitiesUnset: entityIds(inventory, partitionUnset),
  relationshipsA: relationshipIds(inventory, partitionA),
  recentA: recentTexts(inventory, partitionA, 2),
  recentUnset: recentTexts(inventory, partitionUnset, 1)
};
console.log(JSON.stringify(result));
