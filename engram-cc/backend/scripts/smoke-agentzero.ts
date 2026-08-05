//! Dev smoke — validates the prod config against the LIVE agentzero store
//! (the 2 GB single-file db), distinct from the unit test's fixture store.
//! Not part of the test suite; run manually: `tsx scripts/smoke-agentzero.ts`.

import { dbPath, loadConfig } from "../src/config.ts";
import { getProvider, _resetProviderForTests } from "../src/engram/provider.ts";
import { getKnowledge } from "../src/engram/knowledge.ts";
import { resolveScope } from "../src/scope.ts";
import { computeOverview, _resetCommunityCacheForTests } from "../src/aggregation/communities.ts";

const cfg = loadConfig();
console.log(
  "storage:", dbPath(cfg),
  "scope:", `${cfg.tenant}/${cfg.workspace}`,
  "vectors:", cfg.enableVector,
  "migration:", cfg.migrationMode,
);
_resetProviderForTests();
const scope = resolveScope(cfg);

const t0 = Date.now();
const caps = (await getProvider(cfg).capabilities()) as Record<string, unknown>;
console.log(`provider opened + capabilities in ${Date.now() - t0}ms; keys:`, Object.keys(caps).slice(0, 12));

const t1 = Date.now();
const sources = (await getKnowledge(cfg).listSources(scope)) as unknown[];
console.log(
  `listSources: ${Array.isArray(sources) ? sources.length + " rows" : "non-array"} in ${Date.now() - t1}ms`,
);

_resetCommunityCacheForTests();
const tc = Date.now();
const overview = computeOverview(cfg, scope);
console.log(
  `communities: ${overview.nodes.length} nodes, ${overview.edges.length} edges, built=${overview.built} in ${Date.now() - tc}ms`,
);

console.log("SMOKE OK");
