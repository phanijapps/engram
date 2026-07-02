# graphmind vs Engram — prior-art survey

> Discipline: applied (practitioner-pattern survey)

**Question:** `aouicher/graphmind` is "similar to engram." What can Engram extract from it, where can Engram simplify, and should Engram get rid of its in-memory adapters?

**Bottom line up front.** graphmind is *not* really similar to Engram — it is a local-first **code-intelligence product** (CLI + Tauri desktop app + MCP server + license/team-sync tiers), not a contract-first memory *library*. The genuine overlap is narrow: roughly graphmind's `graphmind-memory` crate (657 LOC of typed-JSONL facts) against Engram's memory domain. On the inmem question your instinct is **directionally right and graphmind is the existence proof** — but you cannot delete inmem today: it is Engram's *reference port implementation* (11.6k LOC, 3.3× the SQLite adapters) and `core/eval` + `adapters/ingest` depend on it. The honest path is "promote SQLite to the single implementation, then delete inmem," which is exactly the crate-modularity gap already in your notes. On retrieval and fusion, **take nothing from graphmind — Engram already has the better RRF.**

---

## What graphmind actually is

[high] graphmind is a product, not a library. Workspace is 9 crates including `graphmind-desktop/src-tauri` (Tauri app), `graphmind-license`, `graphmind-mcp` (rmcp, stdio), `graphmind-cli`. README markets installers (Homebrew, `.dmg`), a token benchmark, and **Pro/Team paid tiers** (`team sync`, `gm_team_*`). Its job: turn a codebase into a structural + semantic graph (tree-sitter, 30+ languages), expose 27 MCP query tools, and layer a persistent "memory" of decisions/patterns on top. Cites: `README.md:21-31,260-288`, `Cargo.toml:1-12`, `SKILL.md:1-10`. (`/tmp/graphmind` clone, commit at HEAD on 2026-07-02.)

[high] Engram is the opposite shape: a contract-first **library layer** (Rust core + TS bindings), storage-neutral, with no product surface, no CLI product, no desktop, no licensing. Cites: `AGENTS.md` ("contract-first Agentic Memory layer… modular: domain contracts come first, Rust owns deterministic behavior"), workspace `Cargo.toml` (11 crates, all `core/*` + `adapters/*` + `bindings/node`, zero product crates).

**Implication:** "what can we get from graphmind" should be scoped to the memory layer and to productization *patterns*, not to graphmind's code-graph/parsing engine — importing that would blow up Engram's "memory layer" focus into a full code-intelligence product. [synthesis]

---

## The memory overlap, side by side

| Dimension | graphmind `graphmind-memory` | Engram memory domain |
|---|---|---|
| Size | 657 LOC, 6 files | inmem adapter alone 11,658 LOC; SQLite adapters 3,549 LOC |
| Store | **JSONL** (`~/.graphmind/memory/*.jsonl`), atomic tmp+rename | SQLite (per adapter) + an in-memory reference impl |
| Entry model | typed facts: `decision/pattern/convention/bug/context/session`, tags, `ttl_days`, `recall_count`, `confidence`, `source{manual,consolidate,heuristic}`, `expires_at` | rich domain: Memory + distinct Belief, Hierarchy, Consolidation, Policy, Provenance, Forget, Eval |
| Retrieval over memory | naive substring term-frequency scoring (`content.contains(term) +1`, tag `+2`, type `+1.5`) | temporal + cue + predictive modes, composed, with RRF fusion |
| Belief / hierarchy / consolidation / policy / provenance / eval | **none** | all present as distinct concepts (AGENTS.md mandates they stay distinct) |
| Test strategy | integration tests against the **real** store (`memory_store.rs`, `golden_pipeline.rs`); no mock/inmem double | eval harness bound to `InMemoryMemoryService` |

Cites: graphmind `crates/graphmind-memory/src/{store.rs,search.rs}`, `tests/memory_store.rs`; Engram `adapters/memory/inmem/src/lib.rs`, `core/eval/tests/fixture_runner.rs`. [high on the structural facts]

[moderate] graphmind's memory is a *deliberate simplification* of exactly the concepts Engram has chosen to make first-class. graphmind reduces "forgetting" to `ttl_days` + manual delete, "consolidation" to a `source: consolidate` tag, "provenance" to the `source` enum, and has no belief/hierarchy machinery at all. Engram's `AGENTS.md` explicitly forbids collapsing these ("Memory, knowledge, belief, hierarchy, policy, provenance, and evaluation concepts must remain distinct unless an ADR changes the model"). **Adopting graphmind's memory model wholesale = deleting Engram's differentiation.** That is a product-direction decision, not a refactor. [synthesis]

---

## What Engram can genuinely extract

[moderate, transferable] **Token-optimization discipline for any MCP/SDK output Engram builds.** graphmind's MCP responses are one-line-per-symbol compact text (not verbose JSON), with field pruning (drop `id`, null fields, redundant counts), smart default limits (15 not 50), `+N more…` truncation markers, and content opt-in (`include_content`). This is a real, cheap, transferable craft lesson for the TS SDK / any future `bindings`-level MCP. Cite: `README.md:536-558`. Independence: single primary source (the repo), but the pattern is independently conventional in MCP design. [synthesis]

[moderate, transferable] **Auto-recall / auto-save hook ergonomics as a productization pattern.** graphmind wires `SessionStart` (inject context), `UserPromptSubmit` (pre-fetch relevant memory), and proactive save of decisions/patterns without asking, plus a 5-min per-session dedup cache. For Engram this maps to the *SDK/adapter* layer (`packages/adapters/`, `packages/client/`), not the Rust core — the core should stay storage-neutral and hook-agnostic. Cite: `SKILL.md:48-66`, `README.md:222-238`. [synthesis]

[low, idea] **`MemorySource { Manual, Consolidate, Heuristic }` enum and `recall_count` field** as a reference for simpler provenance / decay signals inside Engram's existing Provenance and Consolidation concepts — not a replacement, a vocabulary reference. Cite: `store.rs:14-26`. [inference]

[low, idea] **JSONL as an audit/export adapter**, not a core store. SQLite is the right core for queryable belief/hierarchy; JSONL's value to Engram is human-inspectable export/audit, mirroring graphmind's "everything is plaintext or SQLite — fully inspectable." Cite: `README.md:638-654`. [inference]

---

## What Engram should NOT take

[high] **The code-graph / tree-sitter / napi-rs parsing engine.** That is graphmind's product and is entirely out of Engram's stated scope. Importing it violates the "memory layer" boundary and the "do not create god packages" rule. Cite: `README.md:25-31`, Engram `AGENTS.md` boundary rules.

[high] **graphmind's retrieval/fusion.** Engram already has RRF in `core/retrieval/src/reciprocal.rs` — weighted per-source RRF with `FusionTrace`, `k=60` default, the Cormack/Clarke/Buettcher 2009 citation, plus `weighted.rs`, `predict.rs`, `composer.rs`. graphmind's `rrf_merge` (`crates/graphmind-embeddings/src/search.rs:45-128`) is a flat unweighted `1/(k+i+1)` merge with no trace. Engram's is the stronger implementation. Cite: `core/retrieval/src/reciprocal.rs:1-23,60-102`. [high]

[high] **graphmind's memory search.** Substring term-frequency scoring (`search.rs`) is far behind Engram's temporal/cue/predictive retrieval. Nothing to take.

[high] **The product surfaces** — desktop (Tauri), license crate, team-sync. Not Engram's layer.

---

## The inmem question — honest verdict

[high] **Your instinct is right; graphmind is the proof.** graphmind ships exactly one real store per concern and tests against the real store (temp file / real JSONL), with no parallel in-memory double. The grep for `InMemory`/`MockStore` trait impls in graphmind returns only runtime caches (`cache.rs`, in-process ranking), never a test substitute. This validates the end state you want: no inmem. Cite: `crates/graphmind-{memory,db}/tests/`, grep over `crates/`. [high]

[high] **But you cannot delete inmem today — it is not a fixture, it is the reference implementation.** `adapters/memory/inmem/src/lib.rs` implements `MemoryRepository`, `MemoryEventRepository`, `MemoryService`, `KnowledgeRepository`, `HierarchyRepository`, `BeliefRepository`, and `InMemoryConsolidationExecutor: ConsolidationMutationExecutor`, plus the runtime stubs (`SystemClock`, `SequentialIdGenerator`, `AllowAllPolicyAuthorizer`). Its own doc comment says it "implements core ports without making `engram-core` depend on concrete state." At 11,658 LOC it is ~3.3× the SQLite memory+knowledge adapters (3,549 LOC) and carries the full consolidation engine (7 modules: belief_synthesis, compaction, contradiction_detection, decay, hierarchy_aggregate, hierarchy_build, semantic_drift). The naming is misleading: `engram-store-memory` *is* the inmem crate; `engram-store-sql` is SQLite. Cite: `adapters/memory/inmem/src/{lib.rs,consolidation/}`, LOC counts, `adapters/memory/sqlite/Cargo.toml:2`. [high]

[high] **Dropping it today breaks eval and ingest.** `core/eval/Cargo.toml` has `engram-store-memory` (inmem) as a dev-dependency, and `core/eval/tests/fixture_runner.rs` constructs `InMemoryMemoryService::new()` in three places as the regression harness's store — there is no SQLite path in eval. `adapters/ingest/Cargo.toml` depends on both inmem crates. So inmem is load-bearing for Engram's correctness gate, not just unit tests. Cite: `core/eval/Cargo.toml`, `core/eval/tests/fixture_runner.rs:6,18,36,61`, `adapters/ingest/Cargo.toml:34-35`. [high]

[high] **The SQLite adapters are not yet behavior-complete enough to replace inmem in the memory path.** `adapters/memory/sqlite/src` has no `belief`, `hierarchy`, `consolidation`, or `knowledge` modules — those live in *separate* crates (`adapters/knowledge/sqlite`, `adapters/orchestration/belief-sqlite`, `adapters/hierarchy/sqlite`) that eval does not consume. inmem centralizes all of it in one service; SQLite is split and partial. This is precisely the crate-modularity gap in your own memory notes ("belief/hierarchy/consolidation ports stuck in core/orchestration"). Cite: `adapters/memory/sqlite/src/` file list vs `adapters/memory/inmem/src/`. [high]

### Recommended sequence (not "delete inmem" — "retire inmem")

1. **Complete the SQLite port coverage** so the split SQLite crates (`knowledge/sqlite`, `belief-sqlite`, `hierarchy/sqlite`, `memory/sqlite`) jointly satisfy every port inmem satisfies. This is the real work and it is already on your roadmap.
2. **Add a temp-SQLite test harness** (in-memory-backed `rusqlite::Connection` via `:memory:` or a temp file — fast, dependency-light, no second domain implementation) and repoint `core/eval` `fixture_runner.rs` and `adapters/ingest` at it.
3. **Run the eval fixture suite against SQLite** and close the behavioral gaps until it passes. (This is the known-unknown below.)
4. **Then delete `adapters/memory/inmem` and `adapters/knowledge/inmem`** and remove the `engram-store-memory` / `engram-store-knowledge-memory` crates from the workspace.

[honest tradeoff] inmem is genuinely useful *today*: fast, zero-dependency, easy to instantiate in a test. graphmind avoids needing it only because its memory model is simple enough that testing the real store is trivial. Engram's richer model (contradiction detection, semantic drift, decay) is costlier to exercise against SQLite. So the trade is real: you trade test-instantiation speed and a second-implementation maintenance burden for the elimination of **implementation-drift bugs** (two ports that disagree). graphmind's existence is evidence the drift burden is worth eliminating — but only after SQLite can carry the eval suite. [synthesis]

---

## Simplification opportunities (in priority order)

1. [high] **Collapse the inmem/SQLite duplication** (above). Two parallel implementations of the same ports is the single largest simplification available, and it removes a whole class of "works in inmem, fails in SQLite" drift. This is the simplification that actually matches graphmind's lesson.
2. [moderate] **Fix the misleading names** while you are there: `engram-store-memory` should not be the inmem crate. Rename or remove so the crate name says what it is.
3. [low] **Adopt compact-output discipline** in the TS SDK / any MCP surface (one-line records, field pruning, smart limits) — graphmind's cheapest transferable craft win.

Do **not** simplify by mimicking graphmind's memory model. That would delete Engram's reason to exist.

---

## Known unknowns

- **Known-unknown:** Are the split SQLite adapters (`knowledge/sqlite`, `belief-sqlite`, `hierarchy/sqlite`, `memory/sqlite`) jointly behavior-complete enough to run the `core/eval` fixture suite that currently runs on inmem? Would be closed by: repointing `fixture_runner.rs` at a temp-SQLite harness and running the suite — the set of failing fixtures is the precise gap inventory. This is the gating experiment for the whole inmem-retirement sequence.
- **Known-unknown:** Does graphmind's auto-save/auto-recall UX have failure modes (over-saving, hallucinated facts, context bloat, recall noise) that would matter before Engram adopts the pattern in its SDK? Would be closed by: reading graphmind's open issues / changelog for memory-related complaints, or a small usage trial.
- **Unknowable from the repo:** Relative retrieval *quality* of Engram vs graphmind on a shared workload — no common benchmark exists, and the two systems retrieve over different objects (Engram memories/beliefs vs graphmind code symbols). No evidence settles it; don't hunt for it in either repo.

---

## Sources

- graphmind repo, cloned to `/tmp/graphmind` at HEAD (2026-07-02): `README.md`, `Cargo.toml`, `CLAUDE.md`, `SKILL.md`, `crates/graphmind-memory/src/{store.rs,search.rs,index.rs,lib.rs}`, `crates/graphmind-embeddings/src/search.rs`, `crates/graphmind-{memory,db}/tests/`. Primary.
- Engram local repo (`demo/engram-ui`): `AGENTS.md`, `Cargo.toml`, `adapters/memory/inmem/src/`, `adapters/memory/sqlite/src/`, `adapters/knowledge/inmem/src/`, `core/retrieval/src/reciprocal.rs`, `core/eval/Cargo.toml`, `core/eval/tests/fixture_runner.rs`, `adapters/ingest/Cargo.toml`. Primary.

*Single-source caveat: both load-bearing corpora are single primary sources (the two repos themselves). Findings rated [high] rest on direct code/manifest evidence; [moderate]/[low] findings are interpretive syntheses and are marked as such. Per the practitioner-independence rule, "graphmind uses JSONL / one store / no inmem double" is one primary source's design — treated as a single prior-art data point that *validates a direction*, not as triangulated universal best practice.*
