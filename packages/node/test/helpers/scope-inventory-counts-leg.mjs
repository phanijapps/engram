/**
 * TASK-006 count-truth leg (job 012): one recipe, one process. Seeds a
 * fixture through the engine write surfaces, reads the scope inventory
 * through the port, then queries the store DIRECTLY (sqlite legs via the
 * Node built-in `node:sqlite` against the store files; pgvector legs via
 * `psql` inside the disposable fixture container) and reports both sides for
 * the parent test to compare. Spawned as a child process by
 * `scope-inventory-counts.test.ts` (one addon per process).
 *
 * argv: <recipe> <nonce>
 * pgvector legs additionally require ENGRAM_TEST_PG_URL and
 * PG_FIXTURE_CONTAINER_ID (the durable-assertion precedent from the
 * code-binding suite).
 */
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { execFileSync } from "node:child_process";
import { DatabaseSync } from "node:sqlite";
import { loadRecipe } from "./build-native-recipes.mjs";

const [recipe, nonce] = process.argv.slice(2);
if (!recipe || !nonce) {
  console.error("usage: node scope-inventory-counts-leg.mjs <recipe> <nonce>");
  process.exit(2);
}

const addon = loadRecipe(recipe);

const extra = {};
if (recipe === "pgvector") {
  const url = process.env.ENGRAM_TEST_PG_URL;
  if (!url) {
    console.error("ENGRAM_TEST_PG_URL is required for the pgvector count-truth leg");
    process.exit(2);
  }
  extra.pgvector_connection_string = url;
}
const base = mkdtempSync(join(tmpdir(), `job012-counts-${recipe}-`));
const storePath = join(base, "store");
const config = JSON.stringify({
  storage_path: storePath,
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
const NOW = new Date().toISOString();
const actor = { id: "job012-counts", kind: "agent", displayName: "job012" };

const partitionA = { tenant: TENANT, subject: "agent-1", workspace: "proj-a", session: null, environment: "prod" };
const partitionUnset = { tenant: TENANT, subject: null, workspace: null, session: null, environment: null };
const partitionDeep = { tenant: TENANT, subject: "agent-1", workspace: "proj-a", session: "s-9", environment: "prod" };

function writeMemory(text, scope) {
  memory.writeJson(
    JSON.stringify({
      kind: "observation",
      content: { text },
      scope,
      requester: { actor, roles: [], permissions: ["memory.write", "memory.retrieve"] },
      provenance: { source: "job012-counts", actor, observedAt: NOW, confidence: 1.0 },
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
      provenance: { source: "job012-counts", actor, observedAt: NOW, confidence: 1.0 },
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
      provenance: { source: "job012-counts", actor, observedAt: NOW, confidence: 1.0 },
      createdAt: NOW
    })
  );
}

// --- fixture: partitions differ across subject/workspace/session dimensions
writeMemory("a-1", partitionA);
writeMemory("a-2", partitionA);
putEntity(`${nonce}-ent-a-1`, "A1", partitionA);
putEntity(`${nonce}-ent-a-2`, "A2", partitionA);
putBelief(`${nonce}-bel-a-1`, partitionA);
writeMemory("u-1", partitionUnset);
putEntity(`${nonce}-ent-u-1`, "U1", partitionUnset);
putBelief(`${nonce}-bel-u-1`, partitionUnset);
putBelief(`${nonce}-bel-u-2`, partitionUnset);
writeMemory("d-1", partitionDeep);
putEntity(`${nonce}-ent-d-1`, "D1", partitionDeep);

// --- engine side: the port's inventory, filtered to this run's tenant
const engineRows = JSON.parse(inventory.inventoryJson()).filter(
  (row) => row.scope.tenant === TENANT
);

// --- direct side: query the governed tables ourselves ----------------------
const SET = "(unset)";
const label = (v) => (v === null || v === undefined ? SET : v);
const key = (tenant, subject, workspace, session, environment) =>
  [label(tenant), label(subject), label(workspace), label(session), label(environment)].join("/");

/** Groups one governed table by the five scope columns for this tenant. */
function directCounts() {
  const counts = { memory: new Map(), entity: new Map(), belief: new Map() };
  const absorb = (map, rows) => {
    for (const row of rows) {
      const k = key(row[0], row[1], row[2], row[3], row[4]);
      map.set(k, (map.get(k) ?? 0) + Number(row[5]));
    }
  };
  if (recipe === "sqlite") {
    const query = (file, table) => {
      const db = new DatabaseSync(join(storePath, file), { readOnly: true });
      try {
        // node:sqlite returns column-named objects; project to tuples so the
        // absorb helper treats both engines identically.
        return db
          .prepare(
            `SELECT tenant AS tenant, subject AS subject, workspace AS workspace, \
             session AS session, environment AS environment, COUNT(*) AS count \
             FROM ${table} WHERE tenant = ? GROUP BY tenant, subject, workspace, session, environment`
          )
          .all(TENANT)
          .map((row) => [
            row.tenant,
            row.subject,
            row.workspace,
            row.session,
            row.environment,
            row.count
          ]);
      } finally {
        db.close();
      }
    };
    absorb(counts.memory, query("memory.db", "memories"));
    absorb(counts.entity, query("knowledge.db", "knowledge_entities"));
    absorb(counts.belief, query("belief.db", "beliefs"));
  } else {
    const container = process.env.PG_FIXTURE_CONTAINER_ID;
    if (!container) {
      console.error("PG_FIXTURE_CONTAINER_ID is required for direct pgvector count queries");
      process.exit(2);
    }
    const query = (table) => {
      const sql =
        `SELECT tenant, COALESCE(subject,'${SET}'), COALESCE(workspace,'${SET}'), ` +
        `COALESCE(session,'${SET}'), COALESCE(environment,'${SET}'), COUNT(*) ` +
        `FROM ${table} WHERE tenant='${TENANT.replaceAll("'", "''")}' ` +
        `GROUP BY tenant, subject, workspace, session, environment`;
      const out = execFileSync(
        "docker",
        ["exec", container, "psql", "-U", "engram", "-d", "engram", "-tAF", "|", "-c", sql],
        { encoding: "utf8" }
      );
      return out
        .trim()
        .split("\n")
        .filter((line) => line.length > 0)
        .map((line) => line.split("|"));
    };
    absorb(counts.memory, query("memories"));
    absorb(counts.entity, query("knowledge_entities"));
    absorb(counts.belief, query("beliefs"));
  }
  return counts;
}

const direct = directCounts();

const engine = { memory: new Map(), entity: new Map(), belief: new Map() };
for (const row of engineRows) {
  const k = key(
    row.scope.tenant,
    row.scope.subject,
    row.scope.workspace,
    row.scope.session,
    row.scope.environment
  );
  engine.memory.set(k, row.memoryCount);
  engine.entity.set(k, row.entityCount);
  engine.belief.set(k, row.beliefCount);
}

const toPlain = (m) => Object.fromEntries([...m.entries()].sort());
// The read model zero-fills kinds for partitions proven by other tables; the
// direct queries only hold keys where rows exist. Fill the direct side's
// absent keys with 0 so equality means the same thing on both sides (any
// direct key MISSING from the engine side still fails the comparison).
for (const kind of ["memory", "entity", "belief"]) {
  for (const k of engine[kind].keys()) {
    if (!direct[kind].has(k)) {
      direct[kind].set(k, 0);
    }
  }
}
console.log(
  JSON.stringify({
    recipe,
    tenant: TENANT,
    engine: { memory: toPlain(engine.memory), entity: toPlain(engine.entity), belief: toPlain(engine.belief) },
    direct: { memory: toPlain(direct.memory), entity: toPlain(direct.entity), belief: toPlain(direct.belief) }
  })
);
