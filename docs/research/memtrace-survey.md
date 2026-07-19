> Discipline: applied (practitioner-pattern survey)

# Memtrace — capability & architecture survey (input for an engram-on-top build)

**Question.** What does `syncable-dev/memtrace-public` actually provide —
architecture, capabilities, and shipped skills — so we can later break down a
prioritized task list for building equivalent capabilities **on top of engram**
(engram as the base) rather than baking them into engram's core?

**Scope note.** The public repo ships **skills, docs, benchmarks, and a Claude
plugin** only. The indexer binary and the **MemDB** graph engine are
**closed-source** (Proprietary EULA). So everything below about *internals* is
read off the repo's own `docs/` and README — it describes the product's
behaviour and contracts accurately, but the closed source means we reason about
**what to replicate**, not copy. Confidence is graded accordingly.

---

## 1. Essence

Memtrace is a **bi-temporal, structural knowledge graph of source code**, served
to AI coding agents over MCP, with **zero LLM calls** during indexing — symbols
are nodes, call/import/type edges are relationships, and every symbol carries
`valid_from`/`valid_to` version history. It positions itself against Mem0 /
Graphiti (which build graphs via LLM inference) on three axes: **determinism**
(Tree-sitter AST, not inference), **cost** ($0 vs $10–50 per large codebase),
and **speed** (1,500 files in ~1.5 s vs ~31 min). `[high]` [README]

The differentiator it claims over AST-only predecessors (GitNexus,
CodeGrapherContext) is the **temporal** and **cross-service API topology**
layers on top of the same AST foundation. `[high]` [README]; `[moderate]` that
the benchmarks are vendor-run (see Known unknowns).

**Engram framing.** Memtrace is *code-structural* only — it has no notion of
agent **memory, belief, hierarchy, consolidation, policy, or forget** semantics.
Those are exactly engram's differentiators. The user's thesis — *engram as
base, build memtrace-like structure on top* — is therefore a complement, not a
rebuild. `[synthesis]`

---

## 2. Architecture (from `docs/architecture.md`)

A **single embedded graph engine** ("MemDB") shared by three process entry
points. They are *not* three databases. `[high]` [architecture.md]

```
AI tool (Claude Code / Cursor / Codex …)
   │  MCP (JSON-RPC over stdio OR streamable-HTTP)
   ▼
memtrace mcp            — agent-facing MCP server; attaches to an owner,
                          or becomes the owner if none is running
   │  localhost loopback (default gRPC 127.0.0.1:50051)
   ▼
WORKSPACE OWNER         — one per .memdb/ directory; holds:
                          · knowledge graph + vectors
                          · indexer + file watcher (notify crate)
                          · embedding model (local ONNX, jina-embeddings-v2-base-code)
                          · cross-encoder reranker
                          · full-text BM25 index (Tantivy)
                          · local dashboard UI (http://localhost:3030)
   │
   ▼
ON-DISK STATE
   <project>/.memdb/                ← per-workspace graph
   ~/.memtrace/embed-cache/         ← per-symbol embedding cache (AST-hash keyed)
   ~/.memtrace/fastembed_cache/     ← model downloads
   ~/.memtrace/rerank-models/       ← reranker downloads
```

### Three entry points share one owner `[high]` [architecture.md]
- **`memtrace index <path>`** — one-shot build/refresh; opens store, scans,
  writes symbols/edges/vectors, exits.
- **`memtrace start`** — the heavy process: opens graph, loads models, watches
  filesystem (`notify`), re-indexes incrementally on save, serves the dashboard
  at `:3030`, exposes loopback gRPC at `:50051`.
- **`memtrace mcp`** — MCP JSON-RPC server (stdio or `streamable-http`).
  Resolves the workspace, attaches to a running owner over loopback, or
  **becomes** the owner if none is running.

### Concurrency model `[high]` [architecture.md]
An OS advisory owner lock (`.memdb/daemon.pid`) + state file
(`.memdb/daemon-state.json`) guarantee **one owner per `.memdb/`**. Multiple
`memtrace mcp` agents attach to the same owner instead of opening duplicate
stores. Separate workspaces have separate owners (separate lock dirs). This is
the primitive that makes **multi-agent fleet** sharing safe.

### MemDB (the closed-source engine) `[moderate]` [architecture.md]
Embedded — "a single binary, no Postgres/SQLite to set up." Stores records
(symbols, edges, episodes, vector blobs) keyed by internal id, property/vector
(HNSW)/per-kind indexes, and a write-ahead log for durability + transactional
consistency.
- **Known tension:** the public README's setup snippet sets
  `MEMTRACE_ARCADEDB_BOLT_URL=bolt://localhost:7687`, implying an
  ArcadeDB-backed **server/multi-tenant** mode (matching the Basekick Labs
  "multi-tenant Arc instances" deployment). The local single-user default is the
  embedded MemDB. We treat **embedded MemDB** as the reference shape and the
  ArcadeDB server mode as an enterprise variant. `[low]` — exact relationship
  undocumented in the public repo.

### Indexing pipeline (8 steps) `[high]` [architecture.md]
1. Walk FS (skip `.git`, `node_modules`, `target`, `dist`, `.claude/worktrees/`,
   `.memtraceignore`).
2. Parse supported sources (Python, JS/TS, Rust, Go, Java, Ruby, C/C++, C#, …).
3. Extract symbols + relationships (calls, imports, type refs, overrides).
4. Detect HTTP endpoints (Express, Encore, NestJS, Axum, FastAPI, Flask, Gin,
   Spring Boot, …) **and their call sites** → cross-service topology.
5. Compute graph metrics — PageRank centrality, betweenness (bridge symbols),
   Louvain modules.
6. Embed Function/Method/Class/Struct/Interface bodies (first ~1,500 chars) with
   a **code-specialised** model (default `jina-embeddings-v2-base-code`, 768-d).
7. Build full-text index over symbol metadata (name, signature, path, kind).
8. **Stamp every symbol `valid_from`/`valid_to`** tied to a `git_commit` or
   `working_tree` episode → the bi-temporal layer.

### Hybrid retrieval (4 legs + rerank) `[high]` [architecture.md, tools.md]
`find_code` runs: lexical **BM25** (Tantivy, per-field boosts: name 5×,
signature 3×) → **semantic** (query embedded, HNSW nearest-neighbour) → **graph
popularity prior** (callers) → **Reciprocal Rank Fusion** of the three →
**cross-encoder rerank** of top-30 → return top-K. This is the exact hybrid
shape, and engram already implements the **RRF + vector** half of it (see §4).

### Performance envelope `[high]` [architecture.md]
`find_symbol` sub-ms; `find_code` hybrid ~450–900 ms p50; index ~3,300 files
(Django) ~14 s; incremental re-index after one save ~30–50 ms; RSS ~30 MB
querying, ≤6 GB indexing.

---

## 3. Capability inventory

### 3a. MCP tool surface — ~38 tools across 9 groups `[high]` [tools.md]
(Authoritative from `docs/tools.md`; the README's "25+" is stale.)

| Group | Tools |
|---|---|
| **Index mgmt** | `list_indexed_repositories`, `index_directory` (async, `job_id`), `delete_repository`, `cleanup_stale_records`, `check_job_status`, `list_jobs` |
| **Discovery** | `find_symbol` (exact/fuzzy Levenshtein), `find_code` (hybrid), `get_symbol_context` (callers+callees+community+processes+evolution in one call), `get_source_window` (windowed read w/ context) |
| **Impact + deps** | `get_impact` (transitive blast radius, `depth`), `find_dependency_path` (from→to call path), `analyze_relationships` (bulk) |
| **Architecture** | `list_communities` (Louvain), `find_central_symbols` (PageRank), `find_bridge_symbols` (betweenness), `get_codebase_briefing` (prose), `get_repository_stats` |
| **Time travel** | `get_evolution` (**6 modes**: recent/impact/novelty/directional/compound/overview), `get_timeline`, `get_changes_since`, `detect_changes` (diff→symbols), `get_cochange_context`, `get_episode_replay`, `replay_history`, `cleanup_episodes` |
| **Processes** | `list_processes` (auto-detected entry-point flows), `get_process_flow` |
| **API topology** | `find_api_endpoints`, `find_api_calls` (incl. cross-repo), `get_api_topology` (cross-service directed graph), `get_service_diagram` (Mermaid) |
| **Quality** | `find_dead_code` (zero-caller), `calculate_cyclomatic_complexity`, `find_most_complex_functions` |
| **Watching** | `watch_directory`, `unwatch_directory`, `list_watched_paths`, `record_external_episode` (CI/deploy events into history) |

### 3b. Temporal engine — 6 scoring algorithms `[high]` [README]
`compound` (blended "what changed"), `impact` (blast radius
`in_degree^0.7 × (1+out_degree)^0.3`), `novel` (anomaly/surprise), `recent`
(exponential decay), `directional` (added vs removed, asymmetric), `overview`
(module summary). Uses **Structural Significance Budgeting** — minimum change
set covering ≥80 % of total significance.

### 3c. Workspaces (multi-repo fusion) `[high]` [workspaces.md]
A **workspace** = several repos sharing **one** `.memdb/` so cross-repo
questions resolve (frontend `fetch("/api/users")` → backend `Router::route`).
Created by a `.memtrace-workspace` marker file, `--workspace` flag, or
auto-detect (non-git dir containing ≥2 child git repos). Resolution walks **up**
from CWD: marker → `.git/` root → CWD. Each repo's symbols are scoped by
`repo_id`; cross-language/cross-repo edges (HTTP calls, shared types, imports)
resolve at index time. **No `switch`/`list-workspaces` command** — a workspace
is just a directory.

> **Directly relevant to engram:** `docs/rfcs/0008-cross-repo-linkage.md` and
> `docs/research/cross-repo-linkage.md` already scope engram's cross-repo
> linkage. Memtrace's workspace model is prior art for the *fusion* shape.

### 3d. Agent skills — ~40 SKILL.md files `[high]` [repo tree]
Shipped under `plugins/memtrace-skills/skills/`. Far richer than the README's
"17". Grouped:

- **Single-domain:** `memtrace-search`, `-relationships`, `-evolution`,
  `-impact`, `-quality`, `-graph`, `-api-topology`, `-index`, `-cochange`.
- **Workflow (chain tools with decision logic):** `memtrace-first`,
  `codebase-exploration`, `change-impact-analysis`, `incident-investigation`,
  `refactoring-guide`, `continuous-memory`, `episode-replay`,
  `session-continuity`.
- **Fleet / multi-agent coordination (the multi-repo fleet layer):**
  `memtrace-fleet-coordination`, `-fleet-first`, `-fleet-publish-intent`,
  `-fleet-record-episode`, `-fleet-resolve`, `-intent-verification`,
  `-preflight`, `-provenance`.
- **Decision memory:** `memtrace-decision-memory`, `-decision-recall`.
- **Docs:** `memtrace-docs`, `-docs-ask`, `-docs-read`, `-docs-search`.
- **Other:** `memtrace-daily`, `-code-review`, `-style-fingerprint`.

Skills are workflow prompts that fire automatically; they're per-agent-installed
(`SKILL.md` for Claude/Cursor/Codex; Kiro uses steering files; Cline/Roo are
MCP-only). `[high]` [README]

### 3e. Dashboard UI (the screenshots) `[high]` [screenshots]
A local web UI (served at `:3030`; branded "MemCortex" internally). Top bar:
`All repositories` dropdown, `History Ready`, `Live`, `Graph`, `Fleet`,
`Cortex`, snapshot timestamp. Observed views:
- **Graph view** — interactive node/edge canvas; **41 node kinds** with
  colour legend (Function, Class, Struct, Interface, Method, Trait, APIEndpoint,
  APICall, Community, Module, CIJob/CIStep, Terraform*, SQLPolicy/Function/
  Trigger, Process, Dependency, …); `VISIBLE KINDS` filter ("17 of 41 hidden");
  `SHOW TIMELINE`. Scale observed: **48,330 nodes · 150,043 edges** across 8
  repos.
- **Node detail** — `CODE` / `INFO` / `HISTORY` tabs; shows file path + line
  range, signature, code preview, and graph context (callers/callees, endpoints).
- **Timeline** — per-symbol version history (the temporal engine, visualised).
- **Insights ("curated read")** — computed (not LLM) briefs: architecture seams,
  communities, bridge symbols, activity, code risk. Single-repo focused; refuses
  to "blend everything" in the all-repos lens.

The screenshots also leak memtrace's **own internal stack** as indexed repos:
`memtrace` (engine, 29,314 nodes), `memdb` (19,202), `memfleet` (fleet layer),
`memfleet-public`, `MemCortex` (AI/analysis UI), `Memscribe`, `mnestic`. This
confirms the closed-source product is internally decomposed the way AGENTS.md
wants engram decomposed: **engine / store / fleet / cortex / skills / docs** as
separate components. `[moderate]` — inferred from the workspace list.

### 3f. Languages & framework scanners `[high]` [README]
16+ programming langs via Tree-sitter; infra (YAML, HCL/Terraform, JSON, TOML,
SQL incl. PostgreSQL `CREATE POLICY` RLS with heuristic edges to ORM schema);
framework-aware scanners (Express, NestJS, Encore, Fastify, Vapor, Hummingbird,
FastAPI, Flask, Django, Gin, Chi, Echo, Actix, Lapis, Kong, OpenResty, Rails;
RTK/TanStack/SWR Query, axios, fetch, SwiftUI; GitHub Actions `needs:` edges,
Helm, K8s; `package.json`/`Cargo.toml`/`pyproject.toml` dep graphs).

---

## 4. Memtrace ↔ Engram: capability overlap & gap (load-bearing for the task list)

Verified against the local engram repo (`core/`, `adapters/`, `bindings/`,
`packages/`). ✅ present · ⚠️ partial · ❌ absent.

| Capability | Memtrace | Engram (base) | Build verdict |
|---|---|---|---|
| **Tree-sitter code parsing** | ✅ | ✅ `adapters/ingest/src/tree_sitter_chunker.rs` + `code_symbol.rs` + tests | **Reuse** — substrate exists `[high]` |
| **Symbol graph (nodes/edges)** | ✅ 41 kinds | ⚠️ general `KnowledgeGraphRepository`/`KnowledgeRepository` ports (entities/relationships, scope+provenance) | **Map** code symbols → knowledge entities; add code edge kinds `[high]` |
| **Code edges (CALLS/IMPLEMENTS/IMPORTS/EXPORTS/CONTAINS)** | ✅ | ❌ general relationships only | **Build** edge resolver `[high]` |
| **Bi-temporal symbol versions (valid_from/to, episodes)** | ✅ core | ⚠️ versioned `SourceDocument`; no per-symbol version timeline | **Build** temporal layer `[high]` |
| **Hybrid retrieval: BM25 (Tantivy)** | ✅ | ❌ no lexical leg (grep confirms) | **Build** lexical leg `[high]` |
| **Hybrid retrieval: vector + RRF** | ✅ | ✅ `core/retrieval` (reciprocal/weighted fusion, composer, router) + `sqlite-vec` | **Reuse** `[high]` |
| **Cross-encoder rerank** | ✅ | ❌ (grep confirms none) | **Build** rerank stage `[high]` |
| **Code-specialised embeddings** | ✅ jina-code | ⚠️ FastEmbed **bge-small** (general) | **Add** a code embedder option `[high]` |
| **Graph algorithms (PageRank/betweenness/Louvain)** | ✅ | ❌ | **Build** algorithms module `[high]` |
| **Cross-repo workspace fusion** | ✅ marker + one store | ⚠️ scope/`stable_source_key`; **RFC 0008** already scopes linkage | **Extend** existing RFC `[high]` |
| **Cross-service HTTP API topology** | ✅ | ❌ | **Build** endpoint/call detection `[high]` |
| **MCP server (38 tools)** | ✅ | ❌ no MCP server found in repo | **Build** MCP layer `[high]` |
| **Agent skills (~40)** | ✅ | ⚠️ repo hosts architect/research **packs** (`agents/`) but no graph-query skills | **Author** engram-flavoured skills `[high]` |
| **Dashboard UI (graph/timeline/insights/fleet/cortex)** | ✅ | ⚠️ `demo/engram-ui` (RFC 0003 demo) | **Build** structural UI `[moderate]` |
| **Fleet coordination (multi-agent shared graph)** | ✅ (memfleet) | ❌ | **Build** fleet/intent layer `[moderate]` |
| **Memory / belief / hierarchy / consolidation / policy / forget** | ❌ | ✅ (`core/belief`, `hierarchy`, `consolidation`, `memory`, policy/scope) | **Engram's edge** — bring memtrace-style *episodic* recall into engram memory `[synthesis]` |
| **Scope/policy/provenance on every write** | ⚠️ license/privacy only | ✅ first-class | **Engram's edge** `[high]` |

**Read of the matrix.** Engram already supplies roughly the *left half of the
hybrid retrieval stack* (vector + RRF), the *port abstractions* a symbol graph
needs, and *all the agent-memory semantics* memtrace lacks. The largest
build-outs are: (1) the **lexical leg + reranker**, (2) **bi-temporal symbol
versioning**, (3) **code edges + graph algorithms**, (4) the **MCP server &
tools**, (5) **cross-repo workspace/API-topology**, (6) the **UI**, (7) the
**fleet** layer, (8) **skills**. `[synthesis]`

---

## 5. Known unknowns

- **Known-unknown:** the closed-source **indexer + MemDB internals** — exact
  on-disk record layout, HNSW config, the symbol-resolution algorithm
  (cross-file/cross-language CALLS edges are non-trivial). Would be closed by:
  reading memtrace's published `docs/data-directories.md`, `performance-tuning.md`,
  `mcp-and-transports.md`; or black-boxing via `execute_cypher` outputs. We do
  **not** need these to scope the build — we replicate *behaviour*, not layout.
- **Known-unknown:** the **ArcadeDB vs embedded-MemDB** split. README setup uses
  `MEMTRACE_ARCADEDB_BOLT_URL`; `architecture.md` insists embedded only. Likely
  embedded=local-single-user, ArcadeDB=multi-tenant/server. Would be closed by:
  `docs/mcp-and-transports.md` + the Basekick multi-tenant writeup. For an
  engram-on-top build we'd target engram's existing **SQLite adapters** as the
  store, sidestepping both.
- **Known-unknown:** the **fleet coordination protocol** (`memtrace-fleet-*`
  skills: publish-intent / record-episode / resolve / intent-verification /
  preflight). The SKILL.md files in the repo describe the *agent workflow*, not
  the wire protocol. Would be closed by: reading those 5–6 SKILL.md files. This
  is the one capability whose shape we have the *weakest* ground on.
- **Unknowable (for us):** memtrace's real-world accuracy on *our* codebases.
  The benchmarks are vendor-run, "no system gets a home-field advantage" claims
  notwithstanding. We'd establish our own ground truth via engram's
  `core/eval` fixtures rather than trust theirs.
- **Known-unknown:** whether engram's tree-sitter ingest currently emits
  *graph* entities/edges or only chunks. `code_symbol.rs` + `tree_sitter_chunker.rs`
  exist, but the WIP `git status` (extractors/ subdir just deleted → flat files)
  means the symbol→graph wiring state is in flux and must be spike-verified
  before scoping. (Echoes `[[arch-divergence-tracker-stales-fast]]` /
  `[[inmem-is-reference-impl-not-fixture]]` — verify before scoping.)

---

## 6. Sources (all primary, from the public repo unless noted)

- README — https://github.com/syncable-dev/memtrace-public (product framing,
  numbers table, tool/skill overview, language matrix, compat matrix).
- `docs/architecture.md` — owner/indexer/mcp model, MemDB, indexing &
  retrieval pipelines, performance envelope.
- `docs/workspaces.md` — workspace marker, resolution, repo_id scoping,
  cross-repo edges.
- `docs/tools.md` — authoritative ~38-tool catalogue + agent tool-call
  sequences.
- Repo file tree (303 files via GitHub API) — confirms ~40 skills, docs/,
  benchmarks/, closed-source note.
- Screenshots (`/home/videogamer/Documents/screenshots/*.png`, 8 images) —
  dashboard UI: 41 node kinds, graph/timeline/insights/node-detail views,
  multi-repo workspace selector, internal-stack leak.
- **Engram base (local repo):** `adapters/ingest/` (tree-sitter), `core/knowledge/`
  (graph/repository ports), `core/retrieval/` (RRF + vector), `adapters/retrieval/
  sqlite-vec/`, plus `docs/rfcs/0008-cross-repo-linkage.md`,
  `docs/research/{cross-repo-linkage,graphmind-prior-art-survey}.md`.
- Secondary (context only): memtrace.sh, Basekick Labs multi-tenant writeup,
  Reddit/SkillsLLM listings (independence: same vendor = one source).
