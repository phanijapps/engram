# graphmind vs Engram — prior-art survey

> Discipline: applied (practitioner-pattern survey)

**Question:** `aouicher/graphmind` is "similar to engram." What can Engram extract from it, where can Engram simplify, and should Engram get rid of its in-memory adapters?

**Bottom line up front.** graphmind is *not* really similar to Engram — it is a local-first **code-intelligence product** (CLI + Tauri desktop app + MCP server + license/team-sync tiers), not a contract-first memory *library*. The genuine overlap is narrow: roughly graphmind's `graphmind-memory` crate (657 LOC of typed-JSONL facts) against Engram's memory domain. On retrieval and fusion, **take nothing from graphmind — Engram already has the better RRF.**

**Superseded implementation note (2026-07-02).** The original version of this
survey correctly identified the broad in-memory adapters as load-bearing at the
time it was written. That finding is now historical: `docs/specs/retire-memory-inmem`
and `docs/specs/retire-knowledge-inmem` moved local conformance to focused
SQLite-backed stores and removed the broad in-memory adapters from the active
workspace. The current lesson from graphmind is therefore narrower: testing the
real local store avoids implementation drift, but Engram preserves richer
memory, knowledge, belief, hierarchy, consolidation, policy, provenance, and
evaluation concepts than graphmind's JSONL memory layer.

---

## What graphmind actually is

[high] graphmind is a product, not a library. Workspace is 9 crates including `graphmind-desktop/src-tauri` (Tauri app), `graphmind-license`, `graphmind-mcp` (rmcp, stdio), `graphmind-cli`. README markets installers (Homebrew, `.dmg`), a token benchmark, and **Pro/Team paid tiers** (`team sync`, `gm_team_*`). Its job: turn a codebase into a structural + semantic graph (tree-sitter, 30+ languages), expose 27 MCP query tools, and layer a persistent "memory" of decisions/patterns on top. Cites: `README.md:21-31,260-288`, `Cargo.toml:1-12`, `SKILL.md:1-10`. (`/tmp/graphmind` clone, commit at HEAD on 2026-07-02.)

[high] Engram is the opposite shape: a contract-first **library layer** (Rust core + TS bindings), storage-neutral, with no product surface, no CLI product, no desktop, no licensing. Cites: `AGENTS.md` ("contract-first Agentic Memory layer… modular: domain contracts come first, Rust owns deterministic behavior"), workspace `Cargo.toml` (11 crates, all `core/*` + `adapters/*` + `bindings/node`, zero product crates).

**Implication:** "what can we get from graphmind" should be scoped to the memory layer and to productization *patterns*, not to graphmind's code-graph/parsing engine — importing that would blow up Engram's "memory layer" focus into a full code-intelligence product. [synthesis]

---

## The memory overlap, side by side

| Dimension | graphmind `graphmind-memory` | Engram memory domain |
|---|---|---|
| Size | 657 LOC, 6 files | focused Rust core crates plus SQLite adapters for memory, knowledge, belief, hierarchy, and vector retrieval |
| Store | **JSONL** (`~/.graphmind/memory/*.jsonl`), atomic tmp+rename | SQLite-backed local conformance stores, with vector retrieval in sqlite-vec |
| Entry model | typed facts: `decision/pattern/convention/bug/context/session`, tags, `ttl_days`, `recall_count`, `confidence`, `source{manual,consolidate,heuristic}`, `expires_at` | rich domain: Memory + distinct Belief, Hierarchy, Consolidation, Policy, Provenance, Forget, Eval |
| Retrieval over memory | naive substring term-frequency scoring (`content.contains(term) +1`, tag `+2`, type `+1.5`) | temporal + cue + predictive modes, composed, with RRF fusion |
| Belief / hierarchy / consolidation / policy / provenance / eval | **none** | all present as distinct concepts (AGENTS.md mandates they stay distinct) |
| Test strategy | integration tests against the **real** store (`memory_store.rs`, `golden_pipeline.rs`); no mock/inmem double | eval and ingestion harnesses now run against SQLite-backed local stores |

Cites: graphmind `crates/graphmind-memory/src/{store.rs,search.rs}`, `tests/memory_store.rs`; Engram `docs/specs/retire-memory-inmem`, `docs/specs/retire-knowledge-inmem`, workspace `Cargo.toml`. [high on the structural facts]

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

Historical note: the broad in-memory adapters were once the reference
implementation for multiple ports, but that state has been superseded by the
retirement specs. Active conformance now uses SQLite-backed stores plus focused
core behavior crates. This makes implementation drift easier to catch because
tests exercise the same local persistence path contributors run in the demo.

### Recommended sequence (not "delete inmem" — "retire inmem")

1. Keep SQLite-backed conformance as the default local test path.
2. Keep memory, knowledge, belief, hierarchy, consolidation, retrieval, and eval
   as focused crates instead of rebuilding an all-in-one test adapter.
3. Add parity fixtures for any behavior that used to exist only in the retired
   process-local implementation before calling that behavior current again.

[honest tradeoff] Engram's richer model (contradiction detection, semantic
drift, decay, hierarchy, taxonomy) is costlier to exercise against SQLite than
graphmind's JSONL memory. The trade is still worth it: one executable local
store path removes a whole class of "works in the fake store, fails in the real
store" bugs. [synthesis]

---

## Simplification opportunities (in priority order)

1. [high] **Keep the retired-adapter guard active.** Two parallel
   implementations of the same ports were the largest simplification target;
   that target is now closed by the retirement specs and should stay closed.
2. [moderate] **Rename remaining pre-move adapter package names** when
   compatibility planning allows it, so crate names match their focused
   responsibilities.
3. [low] **Adopt compact-output discipline** in the TS SDK / any MCP surface
   (one-line records, field pruning, smart limits) — graphmind's cheapest
   transferable craft win.

Do **not** simplify by mimicking graphmind's memory model. That would delete Engram's reason to exist.

---

## Known unknowns

- **Known-unknown:** Which research-architecture behaviors still lack durable
  SQLite-backed parity fixtures after the in-memory retirement? Would be closed
  by: `docs/specs/research-architecture-parity` tasks T2-T11.
- **Known-unknown:** Does graphmind's auto-save/auto-recall UX have failure modes (over-saving, hallucinated facts, context bloat, recall noise) that would matter before Engram adopts the pattern in its SDK? Would be closed by: reading graphmind's open issues / changelog for memory-related complaints, or a small usage trial.
- **Unknowable from the repo:** Relative retrieval *quality* of Engram vs graphmind on a shared workload — no common benchmark exists, and the two systems retrieve over different objects (Engram memories/beliefs vs graphmind code symbols). No evidence settles it; don't hunt for it in either repo.

---

## Sources

- graphmind repo, cloned to `/tmp/graphmind` at HEAD (2026-07-02): `README.md`, `Cargo.toml`, `CLAUDE.md`, `SKILL.md`, `crates/graphmind-memory/src/{store.rs,search.rs,index.rs,lib.rs}`, `crates/graphmind-embeddings/src/search.rs`, `crates/graphmind-{memory,db}/tests/`. Primary.
- Engram local repo (`demo/engram-ui`): `AGENTS.md`, `Cargo.toml`,
  `adapters/memory/sqlite/src/`, `adapters/knowledge/sqlite/src/`,
  `adapters/orchestration/belief-sqlite/src/`,
  `adapters/hierarchy/sqlite/src/`, `core/retrieval/src/reciprocal.rs`,
  `docs/specs/retire-memory-inmem`, and `docs/specs/retire-knowledge-inmem`.
  Primary.

*Single-source caveat: both load-bearing corpora are single primary sources (the two repos themselves). Findings rated [high] rest on direct code/manifest evidence; [moderate]/[low] findings are interpretive syntheses and are marked as such. Per the practitioner-independence rule, "graphmind uses JSONL / one store / no inmem double" is one primary source's design — treated as a single prior-art data point that *validates a direction*, not as triangulated universal best practice.*
