# TencentDB Agent Memory vs Engram — Capability Comparison & Delta Analysis

**Date:** 2026-08-22
**Analyzed repo:** `~/projects/TencentDB-Agent-Memory` (clone of `TencentCloud/TencentDB-Agent-Memory`, v2.0.1-beta.1, MIT)
**Method:** five parallel read-only deep-dive subagents (MemoryCore, MemoryProxy, MemoryKnowledge, MemoryPanel/deploy, engram baseline from docs), synthesized here. All Tencent claims anchored to that checkout; engram claims anchored to `docs/product/engram.md`, `docs/specs/`, `core/integration/src/capability.rs`, `mcp/engram-mcp/src/`.

---

## 0. TL;DR

TencentDB Agent Memory (TDAI) is a **multi-tenant memory SaaS product**: four services (core gateway, knowledge service, LLM proxy, web panel) + SDK, TypeScript throughout, ~179k LOC. Its center of gravity is **product mechanics** — teams/agents/ACLs, a unified asset model (Chat Memory, Skill, Wiki, CodeGraph), a human control panel, and zero-code agent integration via an LLM-traffic proxy. Its memory engine is comparatively shallow: L0–L3 chat distillation with LLM dedup, BM25+vector+RRF, no KG/ontology, no contradiction model, no decay, no provenance depth.

Engram's center of gravity is the **opposite**: a contract-first, engine-neutral memory *engine* (KG + SKOS, beliefs/contradiction bi-temporal, decay/consolidation, 6 retrieval lanes + fusion, graph analytics, maintenance API, eval harness) with thin product surface (MCP server + engram-cc viz).

**The delta that matters:** TDAI ships the product layer engram has deliberately not built (identity/ACL, assets, skills, panel, proxy); engram ships the engine layer TDAI lacks almost entirely. Nothing in TDAI's engine is ahead of engram's engine except conversation-shaped layering (L0–L3), skill extraction, CJK tokenization, and adaptive pipeline triggering.

---

## 1. What TDAI Is

Four services + SDK (`deploy/global-images/.env.example:48-51`):

| Service | Port | Role |
|---|---|---|
| **MemoryCore** (`agentmemory/memory-core`) | 8420 | Memory engine gateway: L0–L3 storage, extraction pipeline, entity/asset/ACL metadata, skill service, v2/v3 REST |
| **MemoryKnowledge** | 8424 | Wiki (LLM-built page graph) + CodeGraph (wraps `@colbymchenry/codegraph`), Hono, `/v3` API, 28 endpoints |
| **MemoryProxy** | 8096 | LLM-traffic proxy: OpenAI `/v1/chat/completions`, Anthropic `/v1/messages`, Codex `/v1/responses`; memory injection + write-back |
| **MemoryPanel** ("Memory Hub") | 8125 | Web UI: teams, agents, assets, bindings, imports, memory browsing |

- **Stack:** Node ≥22, TypeScript, Hono, zod, Drizzle; local mode = SQLite (`node:sqlite` + `sqlite-vec` + FTS5 + jieba) — notably the *same local substrate engram uses* (validation of that stack choice). Service mode swaps in Tencent Cloud VectorDB (TCVDB) + COS + Redis ("TencentDB" branding = TCVDB, not MySQL/TDSQL).
- **Deploy modes** (`README.deployment.md`): standalone (SQLite, in-process state) vs service (TCVDB/COS/Redis, k8s multi-replica with health-check degradation states).
- **Maturity:** first public beta 2026-07-21, v2.0.0 open-source 2026-08-03, v2.0.1-beta.1 2026-08-13. Very young, moving fast.
- **Provenance of parts:** CodeGraph reuses `colbymchenry/codegraph` npm pkg; skill management reuses Hermes Agent code; Wiki informed by Karpathy's LLM-wiki gist (all credited in README).

---

## 2. Component Findings (condensed; file refs from the checkout)

### 2.1 MemoryCore — the engine

- **L0 Conversation** (`core/conversation/l0-recorder.ts`): raw messages → SQLite `l0_conversations` + `l0_vec` (vec0) + JSONL daily shards. Isolation dims: team/user/agent/session/task.
- **L1 Atoms** (`core/record/l1-writer.ts`, `prompts/l1-extraction.ts`): 7 types — chat mode `persona|episodic|instruction`, work mode `work_fact|work_task|work_method|work_artifact`. `l1_records` + `l1_vec` + `l1_fts` (FTS5). Fields incl. `priority` (0–100), `scene_name`, `version`.
- **L2 Scenario** (`prompts/scene-extraction.ts`): *markdown files* (`scene_blocks/*.md`) or TCVDB profiles, template with META block + narrative; UPDATE/MERGE/CREATE consolidation; "heat" = update frequency; cap 15 scenes (chat)/10 (work).
- **L3 Persona** (`prompts/persona-generation.ts`): `persona.md` — user narrative profile with archetype + 4 chapters (chat) or Team Operating Doctrine (work), ≤2000 chars, "evolution trajectory."
- **Async pipeline** (`services/pipeline-worker.ts`, `timer-scanner.ts`): L1 every N conversations with **warmup adaptive threshold** (1→2→4→…→N), idle timeout 600s; L2 delayed 10s after L1, min interval 900s; L3 every 50 L1-completions. Worker permit pool for concurrency.
- **Dedup** (`prompts/l1-dedup.ts`): batch LLM conflict resolution → `store|update|merge|skip`, cross-type merge (episodic+persona→persona), multi-target replace (`target_ids[]`), monotonic `version`. **No contradiction table, no bi-temporal** — conflicts are whatever the dedup LLM says.
- **Retrieval** (`core/tools/memory-search.ts`): strategy ∈ {nativeHybrid (TCVDB server-side dense+sparse+RRF), hybrid (FTS5 ∥ sqlite-vec → client RRF k=60), embedding, keyword}. jieba `cutForSearch` tokenization for CJK FTS5 + stop-words. Budgets: maxResults 5, scoreThreshold 0.3, per-memory + total char caps, timeout 5s. Layer routing: L2/L3 pre-fetched via profile-sync (bootstrap), L1 searched per-turn. Injection: L1 prepended to user prompt; L3+scene nav in system prompt.
- **Lifecycle:** v2/v3 CRUD (`/atomic/update|delete`, `/scenario/*`, `/core/write`); optional audit append; retention = TTL days + daily cleanup cron reconciling JSONL↔VectorStore. **No decay curve, no forgetting semantics** beyond delete/TTL.
- **Metadata/tenancy** (`metadata/types.ts`, `sqlite-init.sql`): `meta_users/teams/team_members/agents/tasks/tasks_agents/assets/agent_fixed_assets/asset_acl/config_params`; roles system_admin / team_admin / member / reviewer.
- **API:** v2 (loose isolation, legacy compat) + v3 (strict `team_id+agent_id+user_id`), Bearer + `x-tdai-service-id`.

### 2.2 MemoryProxy — the integration surface

- **Interception** (`server.ts`, `handler.ts`, `anthropicHandler.ts`, `codexHandler.ts`): serves OpenAI + Anthropic + Responses protocols; SSE passthrough with `x-request-id` propagation. Adapter pipeline: `raw → Adapter.parse → AgentContext → hooks → Adapter.serialize` (`injection/pipeline.ts`).
- **Injection point:** `user.before` (before last user message), XML-tagged block (`tdai-l1-recall-injector.ts`), e.g. `<tdai_recalled_l1_memories>` incl. own + *borrowed* agent collections.
- **Write-back:** per-turn `recordTdaiTurn()` → `/v3/atomic/conversation`, 8192-char chunking, retry/idempotency; stateless `turnSeq` derivation (tool loops share a turn); Redis session store TTL 1800s with in-memory degraded fallback.
- **Agent adapters** (`agent-adapters/`): claude-code (skips `<system-reminder>` in user-text extraction), codex, codebuddy, workbuddy, dsh, default — per-client classify/normalize only.
- **mem: commands** (`mem-command/`): `mem:sync`, `mem:create-skill`, `mem:help` intercepted before upstream forward.
- **Injection policy:** YAML injector whitelist, priority-sorted hook registry, cache strategies `none|session_init|hybrid`, prewarm (20s budget, COS-backed hook cache).
- **Identity/auth:** session id derived from `x-conversation-id|x-session-id|x-chat-id|x-thread-id|...`; user keys verified via `/v3/meta/auth/verify`; system-user bypass list.
- **Observability:** ClickHouse per-turn tokens/credits/cache tokens + model routing; Langfuse traces (turn = SHA-256(sessionKey+turnSeq)); Opik spans; per-model pricing table; JSONL request logs to COS.
- **Reliability:** memory-down → hooks return `[]`, LLM traffic still flows (graceful); health endpoint reports degraded storage fallbacks for k8s.

### 2.3 MemoryKnowledge — wiki, codegraph

- **Wiki** (`engines/wiki/`): sources (md/txt) → LLM 2-stage prompts (analysis: summary/entities/concepts/xrefs; generation: typed pages `source|entity|concept|comparison|synthesis` via FILE blocks) → per-wiki `index.db` (FTS5 + page_meta + graph_edge + source sha256). Incremental: sha256 per source, cascade delete on removal. **BM25 only — no embeddings.** Page-level attribution only (no chunk-level citations). Communities field exists but always empty.
- **CodeGraph** (`engines/code/bridge.ts`): wraps `@colbymchenry/codegraph` v1.2.0 (tree-sitter); per-repo SQLite; queries: search, **explore (NL→grouped code)**, callers, callees, impact (depth ≤10), node, status, files. **Public HTTPS repos only** (no credentials in API); full-load memory model (same ceiling engram's RFC-0012 analysis found).
- **Async:** SerialQueue per wiki (concurrency 1), BuildQueue per repo, FIFO auto-sync w/ 3 workers; status draft→pending→processing→ready/failed; ingest progress callbacks throttled 500ms. **No retries** — failed stays failed.
- **Also:** `src/mcp/` contains an MCP stdio server for the knowledge tools (16 tools: 7 wiki + 9 codegraph).
- **No fusion** with chat-memory retrieval — knowledge tools are called explicitly by the agent.

### 2.4 MemoryPanel / asset governance

- **Entities:** User (normal/system_admin; `sk-mem-*` keys; **no SSO**), Team, TeamMember (admin/member/reviewer), Agent (visibility, prompt, chat_memory_rel), Task (source manual/tapd/github), Asset (4 types), FixedAssetBinding, ACL grants.
- **Asset model** (`metadata/types.ts:172-200`): `asset_type ∈ {skill, llm_wiki, code_graph, chat_memory}`; `visibility ∈ {private, team, restricted, agent, task}`; `status ∈ {draft, candidate, approved, deprecated, archived, failed}`; `version`, `usage_count`, `last_used_at`, owner.
- **ACL** (`permission-checker.ts`): principals `user|team_role|agent`, permissions `read|write|delete|assign|share|use`, allow/deny effects; evaluation order: owner-allow short-circuit → non-member deny → visibility → role defaults (admin=r/w/a/s, member=read) → explicit ACL → default deny. `restricted`/`task` assets are **not bindable to agents** (`canBindAsset` false).
- **Loadout:** fixed bindings per agent with `injection_mode ∈ {direct, summary, tool, reference}` + priority; chat-memory special rel: `memory_shared_with_team`, `imported_agent_ids` (hard cap **2** borrowed agents).
- **Panel workflows:** team CRUD, asset browse/filter, binding, cold-start imports (conversation paste/file ≤100 msgs → L0→async L1-L3; skill directory or session extract; wiki doc ingest; repo URL → codegraph), view/delete memory, time-range filtering. **L1–L3 editing and L0/L1 search are roadmap, not shipped.** No approval workflow in practice (direct publish; reviewer role = member defaults).
- **Boot:** auto-creates default team + default agent; prints a ready-to-paste `ANTHROPIC_AUTH_TOKEN` claude command.

---

## 3. Capability Matrix

✅ shipped · 🚧 partial/draft · ❌ absent

| Capability | Engram | TDAI | Notes |
|---|---|---|---|
| Memory CRUD + lifecycle states | ✅ active→archived→redacted→forgotten→expired | 🚧 active/archived + delete/TTL | Engram forget modes are a contract; TDAI deletes |
| Idempotency on write | ✅ `idempotency_key` | 🚧 gateway retry + LLM dedup | Different layer |
| Provenance/evidence | ✅ source/actor/observed_at/evidence/confidence on every record | 🚧 `source_message_ids`; page-level wiki attribution | |
| Bi-temporal validity | ✅ valid_from/until + as_of | ❌ monotonic version counter only | |
| Contradiction detection | ✅ reviewable contradictions + authority-aware reconciliation | ❌ LLM dedup prompt only | |
| Belief synthesis | ✅ recomputable derived beliefs (+ LLM via pi-mono) | ❌ closest = persona synthesis (not evidence-linked) | |
| Decay/consolidation | ✅ policy-expiry + Ebbinghaus + gated dry-run plans | ❌ TTL cleanup cron | |
| Knowledge graph | ✅ entities/relations grounded in chunks, retraction convergence | ❌ wiki link-graph only (regex wikilinks) | |
| Ontology/taxonomy | ✅ SKOS, durable OntologyRepository, drift detection | ❌ fixed 5 page types | |
| Doc ingestion | ✅ deterministic chunkers + LLM extraction opt-in | 🚧 md/txt → LLM pages | |
| Code ingestion | ✅ native tree-sitter, 13+ langs, qualified identities, provenance | 🚧 wraps npm pkg; public HTTPS repos only | TDAI has NL `explore`; engram queries are structured |
| Code queries | ✅ dead_code/blast_radius/dependency_path/architecture/api_topology/whats_changed | 🚧 search/callers/callees/impact/explore | |
| Retrieval lanes | ✅ 6 modes + PPR associative + GraphRAG community + cross-encoder adapter | 🚧 BM25 + vector + RRF (chat only; wiki BM25-only) | |
| Fusion | ✅ configurable RRF (weights, k) + budget compression + policy filter | 🚧 fixed RRF k=60 client-side | |
| CJK lexical support | ❌ (Tantivy default tokenization) | ✅ jieba cutForSearch + stop-words | Genuine gap for zh recall |
| Hierarchy / context compression | ✅ HierarchyNode aggregates + navigation | 🚧 L2 scene blocks (markdown) | Different shapes, similar intent |
| Chat-memory layering (L0–L3) | ❌ (flat records + links; procedures) | ✅ full cascade + adaptive triggers | See §4 |
| Persona / user profile layer | ❌ | ✅ L3 persona with evolution trajectory | |
| Skills as assets | 🚧 procedures (RFC-0016 L6) + engram-distill agent skill | ✅ SKILL.md + resources + versions + 2-step LLM extraction + import/export + equip | See §4 |
| Graph maintenance | ✅ dry-run plan + tx apply + merge/archive/restore + graph_health | ❌ raw CRUD only | |
| Graph analytics | ✅ PageRank/betweenness/Louvain/reachability | ❌ (wiki communities field always empty) | |
| Identity/tenancy | 🚧 Scope (tenant/workspace/subject) as ownership boundary, policy-authorizer traits | ✅ User/Team/Agent/Task + roles + service_id isolation | See §4 |
| ACL / principals | ❌ (policy governs data lifecycle, not who) | ✅ deny-by-default ACL, 6 permissions, 3 principal types | See §4 |
| Asset governance | ❌ | ✅ unified registry: status/version/visibility/usage | |
| Agent loadout / bindings | ❌ | ✅ fixed bindings + injection modes + priority | |
| Cross-agent memory sharing | ❌ | 🚧 borrow ≤2 agents' collections; team-shared chat memory | |
| Agent integration — MCP | ✅ unified engram-mcp, 37 tools, 5 profiles | 🚧 knowledge-only stdio MCP; REST elsewhere | |
| Agent integration — proxy | ❌ | ✅ zero-code OpenAI/Anthropic/Responses interception + injection + write-back + mem: cmds | See §4 |
| Agent integration — SDK | ✅ Rust facade + N-API + TS client (surface-parity gated) | ✅ TS SDK (`@tencentdb-agent-memory/memory-tencentdb`) | |
| HTTP API | 🚧 engram-mcp-http (Phase F queue/webhook open) | ✅ full v2/v3 REST + OpenAPI (knowledge) | |
| Human UI | 🚧 engram-cc (read-mostly viz; no governance) | ✅ Memory Hub: teams/assets/bindings/imports/analytics | |
| UI memory editing | ❌ | ❌ (view/delete today; editing = roadmap) | Both gaps |
| Usage analytics | 🚧 observability-api capability | ✅ per-asset usage_count/last_used_at + per-turn token/cost (ClickHouse/Langfuse/Opik/pricing) | |
| Cold-start import UX | 🚧 operations exist as MCP tools; no guided flows | ✅ panel workflows (paste/file/dir/repo) | |
| Distributed deployment | ❌ single process per store | 🚧 service mode (Redis/TCVDB/COS, k8s) | |
| Engine neutrality / swap | ✅ ADR-0022 grid, SQLite shipped + pgvector draft | ❌ SQLite or TCVDB fork; cloud lock-in in service mode | |
| Contracts / codegen | ✅ contract-first, generated TS types, parity lint | ❌ zod per service; metadata duplicated across services | |
| Eval harness | ✅ deterministic fixtures + regression + code-intel suites | ❌ none in repo | |
| Published benchmark | ❌ | 🚧 PersonaMem 48%→76% claim (methodology not in repo; claimed # beats published SOTA ~63% — treat as unverified marketing) | |
| Export/import | ✅ cross-store ImportData + dry-run | 🚧 v2→v3 migration script; skill export | |
| Multi-LLM config | ✅ TS-side pi-mono (memory ops) | ✅ proxy-group + memory-group LLM configs, model routing | |

---

## 4. Deltas — what TDAI has that engram lacks (ranked)

Each item notes where it would fit engram's architecture (Rust core port / TS `@engram/runtime` / engram-cc), per AGENTS.md boundaries.

### Tier 1 — strategic gaps (product shape)

**D1. Identity + ACL + tenancy.** Principals (user/role/agent), deny-by-default grants, visibility levels, role defaults (`metadata/types.ts`, `permission-checker.ts`). Engram has `Scope` as an ownership boundary and policy-authorizer *traits* in `engram-runtime`, but no principal model and no authz evaluation anywhere. *Fit:* extend `engram-runtime` policy gates with principal/permission types (port-shaped, engine-neutral), evaluation in `core/` — hosts inject identity; engram-mcp-http/engram-cc become enforcing surfaces. This is the prerequisite for D2/D3/D5.

**D2. Asset governance model.** One registry unifying heterogeneous memory artifacts (chat memory, skills, wikis, codegraphs) with owner/version/status/visibility/usage + bindings. Engram treats everything as records/graphs with no asset abstraction. *Fit:* a TS-runtime (or thin core) `AssetRegistry` layer above existing stores; assets wrap existing capability outputs (a wiki = doc ingest output; a codegraph = `scan_repo` output; a skill = D4).

**D3. Agent loadout + injection modes.** Per-agent fixed bindings with `direct|summary|tool|reference` injection and priority — "memory is the agent's loadout, not a global prompt." Engram's recall is query-driven only; nothing assembles a per-agent standing context. *Fit:* natural extension of the unified-recall compose layer (a "standing context lane" resolved from bindings), surfaced via MCP/payload API.

**D4. Skills as first-class memory assets.** SKILL.md + resource files + versions + head/TTL + 2-step LLM extraction (propose→confirm) from conversations, executable allowlists, directory import/export, agent equipping. Engram already has 80% of the substrate: `procedure_put/list/increment` (RFC-0016 L6), `engram-distill` agent skill, doc store for resources. This was already on engram's power-feature backlog. *Fit:* procedures + stored resources + status/version fields + extraction op in engram-maintain. Highest-leverage single delta: it converts engram's procedures into a managed asset.

**D5. Human control panel with governance.** Memory Hub does team/asset/binding/import/usage workflows in a web UI. Engram has engram-cc (Memory/Observatory/Graph tabs, read-mostly). *Fit:* extend engram-cc with a governance tab once D1–D3 exist; import workflows are thin wrappers over existing MCP tools (`index_docs`, `scan_repo`, conversation import is new).

**D6. Proxy-based zero-code integration.** Protocol-level interception (OpenAI/Anthropic/Responses), `user.before` XML-tagged injection, per-turn write-back, per-client adapters, `mem:` commands, prewarm caching, graceful degradation. Converts "agent must call MCP tools" into "agent is memory-aware by default." *Fit:* a new `@engram/runtime` module (e.g. `engram-proxy`) — TS owns transport ergonomics; reuses engram-mcp-http patterns + unified recall for the injection payload. Independent of D1–D5 (could ship first as single-user).

### Tier 2 — memory-mechanism gaps

**D7. L0–L3 conversation distillation cascade.** Raw turns → atoms → scenario blocks → persona, with adaptive async triggers, scene heat, persona evolution. Engram has flat memories + links + hierarchy aggregates + reflection→beliefs, but no conversation-shaped cascade and no persona artifact. Mapping onto engram primitives: L0 = episodes/records, L1 = memories (extraction op exists), L2 ≈ hierarchy aggregates / RFC-0015 context-packets, L3 ≈ derived profile beliefs. *Fit:* mostly composition — an `engram-maintain` pipeline stage + a "profile lane" in unified recall; reuse belief/confidence machinery instead of raw LLM dedup verdicts (engram can do this *better*: contradicted personas are detectable).

**D8. Cross-agent memory sharing/borrowing.** Team-shared chat memory + borrow-up-to-2-agents' collections in recall. Engram scopes isolate; nothing shares selectively. *Fit:* depends on D1; retrieval-side = filter fusion across allowed scopes.

**D9. CJK lexical retrieval.** jieba tokenization + Chinese stop-words on the BM25 lane. Engram's Tantivy adapter defaults won't match zh queries well. *Fit:* tokenizer policy in `adapters/retrieval/tantivy-lexical` (or chunk-level pre-tokenization like TDAI does for FTS5).

**D10. Adaptive extraction triggering.** Warmup threshold (1→2→4→…→N turns), idle timeouts, min/max intervals, heat-driven consolidation timing in the scheduler. *Fit:* `engram-maintain` scheduler policy — small, self-contained.

**D11. LLM write-path dedup semantics.** Batch store/update/merge/skip with cross-type merge and multi-target replace at extraction time. Engram's dedup lives in maintenance (deterministic candidates + merge API) not on the ingest path. *Fit:* optional LLM op in engram-maintain after write; record outcomes via provenance (`method: llm-dedup`) so it stays auditable.

### Tier 3 — ops/product polish

**D12. Per-turn cost/usage telemetry.** Token/credit accounting per turn (ClickHouse), traces (Langfuse/Opik), per-model pricing, per-asset usage_count. Engram has an observability-api capability (S6) without cost/usage semantics. *Fit:* extend observability reporting in `@engram/runtime`.
**D13. Guided cold-start imports.** Paste/file conversation import, skill-directory import, repo-URL import with progress. *Fit:* engram-cc + existing tools.
**D14. Service-mode deployment.** Redis state, object-store artifacts, multi-replica health states. *Fit:* deferred until there's demand; engram's single-store model is a feature (embedded).
**D15. Published benchmark numbers.** PersonaMem claim (unverified) vs engram's unpublished evals. *Fit:* run engram's eval harness against PersonaMem/LongMemEval (both already identified in engram's SOTA-gap backlog) and publish — cheap credibility, and engram's harness is deterministic where TDAI's is not.

### What TDAI has that is *not* worth copying
- Two-version API surface with legacy-compat mode (v2 loose/v3 strict) — contract debt engram avoids by design.
- Duplicated metadata across services (meta_assets in core vs knowledge_wiki/code_graph rows in KS) — exactly the god-store drift engram's boundary rules prevent.
- Markdown files as L2/L3 store in standalone mode — cute but unqueryable; engram's durable hierarchy is the right call.
- Cloud lock-in (TCVDB/COS) as the scale path — contradicts ADR-0022.

---

## 5. Deltas the other way — engram leads (defense/positioning)

1. **Only engram has a real knowledge layer**: typed entity/relationship graph grounded in source chunks, SKOS ontology/taxonomy, retraction convergence, cross-repo identity (RFC-0008/0009/0014). TDAI's "knowledge" is a wikilink regex graph over LLM pages.
2. **Epistemics**: bi-temporal beliefs, contradiction detection with resolution workflow, confidence/provenance on every record, authority-aware reconciliation. TDAI trusts a dedup prompt.
3. **Forgetting**: policy/retention/sensitivity/delete-mode contracts vs TDAI's delete + TTL cron.
4. **Retrieval depth**: 6 modes + PPR + GraphRAG community summaries + (wired) cross-encoder rerank + configurable fusion — TDAI has one hybrid path (and BM25-only for wiki).
5. **Maintenance**: dry-run plan + transactional apply + merge/archive/restore + graph_health — TDAI has raw CRUD.
6. **Codegraph**: native tree-sitter, 13+ languages, qualified identities, dead-code/dependency-path/architecture queries, private repos. TDAI wraps an npm package, public repos only. (TDAI's NL `explore` is the one engram-lacking query — a prompt over search results, cheap to add.)
7. **Engineering guarantees**: contract-first generated types, engine neutrality + backend recipes, surface-parity lint, deterministic eval fixtures, Rust core (perf/memory safety), N-API + MCP + SDK parity. TDAI: 3 TS services, zod-per-service, no evals, no type generation.
8. **Single-binary embeddedness**: one SQLite file, in-process — TDAI needs 3 services + panel even standalone (920MB image).

**Positioning read:** these aren't the same product. TDAI competes with mem0/Zep/Letta-style *memory SaaS* on team-collaboration UX; engram competes as the *memory engine* those products lack. The deltas in §4 are how engram stops conceding the product layer; §5 is why the engine stays differentiated while doing it.

---

## 6. Recommended sequencing for "this level info in engram"

Opinionated, respecting current momentum (feat/mcp-parity, graph-maintenance just shipped):

1. **D4 Skills** (procedures→assets) — smallest distance, highest product value; no new tenancy needed.
2. **D7 L0–L3 cascade** as engram-maintain pipeline + profile lane — reuses beliefs/hierarchy/reflection; makes engram's chat-memory story legible.
3. **D6 Proxy module** in @engram/runtime — zero-code integration story; independent of tenancy; directly serves agentzero/claude-code/codex users today.
4. **D1+D2+D3+D5 identity/ACL/assets/loadout/panel** as one arc (they're inseparable) — unlocks team memory (D8) after.
5. **D9+D10+D11** as engine polish alongside.
6. **D15 published benchmarks** opportunistically (eval harness exists).

---

## 7. Deep-dive: engram tree-sitter indexing vs `@colbymchenry/codegraph`

*(Added 2026-08-23. Sources: engram `adapters/ingest` + `mcp/engram-mcp` codegraph tools + specs (read from repo); codegraph v1.5.0 source (cloned from github.com/colbymchenry/codegraph). Two parallel subagent reports, synthesized here.)*

| Dimension | engram (native) | @colbymchenry/codegraph |
|---|---|---|
| Parsing stack | 17 tree-sitter grammars, Rust in-process; AST walk (no .scm queries); line-based regex fallback for unsupported files | Rust N-API kernel (20 grammars) **+ WASM fallback** (web-tree-sitter); ~40 languages total incl. framework/template extractors (Svelte/Vue/Astro/Razor/Liquid/MyBatis/COBOL/VB/Pascal/Terraform); `.h` C++/ObjC content heuristics |
| Node kinds | Declarations only: fn/class/struct/interface/trait/enum/type-alias (+Repository) | **22 kinds**: + fields, properties, parameters, variables, constants, enum members, namespaces, imports/exports, routes, components |
| Edge kinds | **`calls` only** (plus deprecated `mentions` for prose) | **13 kinds**: contains, imports, exports, extends, implements, overrides, instantiates, decorates, type_of, returns, references, calls |
| Symbol attributes | name, kind, source_refs (path/lines), git provenance; container chunks are anchor-only (empty text) | + signature, docstring, visibility, is_exported/async/static/abstract, decorators, type params, return type, precise columns |
| Edge resolution | Best-effort bare-name matching (local set + global name index); cross-file via global name index | Scoped/qualified/dotted-receiver chains; import-based resolution; conformance-chain (`this.delegate.method()` via protocol type); **framework callback synthesis** (React handlers, routes for 17 web frameworks, Vue stores, Swift↔ObjC bridging) |
| Identity | `entity.name` = **bare logical name** (RFC-0020 Phase 1); id = hash(graph_id+name); bare-name collisions = last-write-wins; `::{suffix}` resolution at query time | id = **SHA256(filePath::qualifiedName)**; `qualified_name` first-class column; overloads distinct; unique edge-identity index |
| Unresolved edges | Silent best-effort (no ledger) | **`unresolved_refs` table** (pending/failed + candidates) with orphan sweep — edges "resurrect" and re-resolve when the target file appears later |
| Storage | Inside the unified knowledge graph: sources → documents (content_hash) → chunks → entities/relationships, git provenance, optional chunk embeddings (sqlite-vec) | Purpose-built per-repo SQLite: nodes/edges/files/unresolved_refs/name_segment_vocab + FTS5 over name/qualified_name/docstring/signature, rich indexes |
| Incrementality | content_hash upsert; delete-prior-graph per (source_key, path); **retraction cascade** embeddings→chunks→doc→graph; removed-path post-pass vs manifest; rayon parallel ingest | (size,mtime) fast-check → content hash; **git diff-index fast path**; FK CASCADE delete on removed files; unresolved refs re-swept; WASM worker recycling every 250 files |
| Scale | 1MiB/file cap; traversal bounds (64/direction); lazy embedding (re-scan ≈ 0 embeds); 442k chunks OK; kernel-scale ceiling admitted (full-load analytics) | Measured: Swift 27k files ~100s; **Linux kernel 70k files / 2M symbols < 12min on 2-core**; cgroup-aware parse pools (≤16); no repo-size cap; 1MB/file cap |
| Query surface | symbol_context, change_impact (d2), search, get_context (**fused recall: lexical+vector+graph**), code_health, architecture, api_topology, whats_changed; codegraph layer: dead_code, blast_radius, dependency_path; PageRank/betweenness/Louvain/temporal | explore (**NL entry**: FTS5 seeds + graph expansion, budget scales w/ project size), search, node, callers/callees, impact (BFS d3), files, dependencies (file-level), status |
| Philosophy | Code as one instance of the source-grounded KG — provenance, embeddings, cross-modal fused recall, deterministic Rust core | A precise *structural* code index — universal languages, rich edges, framework heuristics, agent-optimized snippets; no semantics/dataflow |

### Verdict

**The borrowed engine is extraction-richer; engram is integration-richer.** codegraph's extraction layer is 1–2 phases ahead of engram's shipped Phase 1: 13 edge kinds vs 1, 22 node kinds vs ~8, qualified identity vs bare names, an honest unresolved-refs ledger vs silent best-effort, and a framework-heuristics layer (routes, React callbacks, bridging) that engram has no equivalent of at all. It also publishes scale numbers (kernel-scale!) engram can't yet quote.

Engram's advantages are everything *around* the index: code symbols live in the same provenance-carrying KG as docs/chunks/beliefs, are reachable through fused recall (lexical+vector+graph) and unified MCP, feed graph analytics (PageRank/communities/dead-code/blast-radius) and temporal scoring, and retraction convergence is a graph-wide guarantee rather than an index-local one. codegraph's graph is an island — per-repo SQLite with no cross-modal story.

### Deltas worth importing into engram (candidates for the RFC-0020 backlog)

1. **Typed edges** (imports/extends/implements/contains/overrides) — already engram Phase 2 intent; this validates priority.
2. **Qualified identity/symbol table** (filePath::qualifiedName as the key) — kills the last-write-wins collision class; also Phase 2 intent.
3. **`unresolved_refs` ledger + orphan sweep** — record best-effort failures instead of dropping them; re-resolve when new files land. Cheap, big honesty win.
4. **Framework resolvers** (route extraction for web frameworks, React callback synthesis) — a heuristic TS-side layer, consistent with engram's Rust-LLM-free/TS-intelligence split.
5. **File-level dependency query** (project over `imports` edges — gated on #1).
6. **FTS5/lexical over signatures+docstrings** in the code retrieval lane.
7. **NL `explore` entry point** — FTS5 seed + bounded traversal; a thin composition over existing search+neighbors.
8. **Publish scale benchmarks** (codegraph quotes kernel-at-12min; engram should quote its own).

## 8. Sources

- Checkout: `~/projects/TencentDB-Agent-Memory` @ v2.0.1-beta.1 (README, ROADMAP, CHANGELOG, INSTALL, README.deployment.md, README.docker.md; MemoryCore/, MemoryProxy/, MemoryKnowledge/ incl. openapi.yaml, MemoryPanel/ incl. web/, deploy/).
- §7 sources: engram `adapters/ingest/` (tree_sitter_chunker.rs, extractor.rs, scanner.rs, reconcile.rs), `mcp/engram-mcp/src/codegraph.rs`, `codegraph/` crates, specs (ast-symbol-extraction, extraction-quality/RFC-0020, scale-repo-ingestion, knowledge-graph-retraction); `@colbymchenry/codegraph` v1.5.0 source (github.com/colbymchenry/codegraph, MIT — cloned to /tmp/codegraph-pkg for analysis; types.ts, extraction/, resolution/, db/, sync/).
- Engram: `docs/product/engram.md`, `docs/specs/` + `docs/specs/README.md`, `docs/adr/0022`, `docs/adr/0027`, `docs/rfcs/0015/0016/0017`, `core/integration/src/capability.rs`, `mcp/engram-mcp/src/main.rs`, `docs/domain-data-model.md`, `engram-cc/`.
- PersonaMem: [arXiv 2504.14225](https://arxiv.org/html/2504.14225v1), [github.com/bowen-upenn/PersonaMem](https://github.com/bowen-upenn/PersonaMem) (COLM 2025). TDAI's 76% claim is not reproducible from their repo; published SOTA on this benchmark is lower (O-Mem ~63%) — treat as marketing until methodology ships.
