# RFC 0003: Durable knowledge layer and demo application

- **Status:** Draft
- **Author:** phanijapps
- **Approver:** phanijapps
- **Date opened:** 2026-06-30
- **Date closed:**
- **Decision weight:** standard
- **Related:** ADR-0003 (implementation stack), ADR-0005 (storage adapter semantics), ADR-0006 (first SQL adapter: SQLite), RFC-0002 (knowledge source extension); specs: `memory-knowledge-boundaries`, `sql-service-conformance`, `knowledge-ingestion`, `fastembed-query-provider`, `ci-vector-feature-gates`, `typescript-native-surface`, `workspace-responsibility-layout`

## Reviewer brief

- **Decision:** Approve a program that (a) makes the `engram-node` N-API binding actually loadable and extends it beyond memory, (b) adds a durable SQLite knowledge-graph adapter with a deterministic graph extractor and a taxonomy port, (c) turns FastEmbed into a real (gated) passage-embedding path, and (d) ships a client-demo-worthy Vite + React + Cytoscape app over a Node backend — so Engram is runnable end-to-end.
- **Recommended outcome:** accept.
- **Change if accepted:**
  - `engram-node` becomes loadable from Node and exposes knowledge / ingest / retrieval / taxonomy (not only memory).
  - New `engram-store-knowledge-sqlite` crate + a `TaxonomyRepository` port in `engram-knowledge` + a deterministic `GraphExtractor` in `engram-ingest`.
  - New standalone `demo/` (Node/TS backend + Vite/React frontend); FastEmbed passage embeddings wired behind the existing feature gate.
- **Affected surface:** `bindings/node`, `core/knowledge` (new port), `adapters/knowledge` (new SQLite crate), `adapters/ingest` (extractor module), `adapters/retrieval/sqlite-vec` (passage embeddings), `packages/node`, `packages/client` (transport surface may widen), new `demo/`.
- **Stakes:** reversible. All changes are additive (new crate, new demo app, additive binding methods). Two semi-permanent effects: growth of the N-API public surface, and a new `TaxonomyRepository` port in `engram-knowledge` (a storage-neutral core contract surface whose removal would be breaking). Both are mitigated — the binding by the focused-struct rule (D2), the port by its minimal, SKOS-aligned shape (D4).
- **Review focus:**
  1. The N-API binding extension stays a set of focused structs, not a god-engine (D2; see Options axis "binding shape").
  2. The shared SQLite file stays an **adapter-internal** composition detail, never a core/binding assumption, enforced by a forbidden-import gate in the Slice 1 spec — so production can swap memory/KG/vectors to Postgres + pgvector without touching contracts (D3, Non-goals).
- **Not in scope:** production Postgres / pgvector adapters; a durable graph DB (Neo4j / RDF / property-graph store); ontology management (the demo is taxonomy-only — the existing `OntologyRepository` port and `Ontology*` types stay in the codebase untouched); any v1 contract field change; multi-tenant hardening, auth, or rate-limiting on the demo server.

## The ask

**Recommendation (BLUF):** Approve the program above, delivered as five vertical slices where each slice leaves a runnable demo. Start with Slice 0 — completing the N-API bridge and proving a real browser → Node → Rust round-trip over the *existing* memory service — because that bridge does not load today and every later slice depends on it.

**Why now (SCQA):**
- *Situation:* Engram's core is mature — memory write/retrieve/forget, knowledge/graph/ontology ports, in-memory + SQLite memory adapters, ingest (filesystem/Git + chunkers), sqlite-vec retrieval, FastEmbed provider, and a TS client are all built (52 phases, PHASE00–PHASE51, all done).
- *Complication:* Almost none of it is reachable from a browser. The N-API binding compiles but produces no loadable `.node`; it is memory-only; there is no durable knowledge backend; taxonomy has no persistence port; ingestion produces chunks but no graph entities/edges; and FastEmbed is test-only with no passage embeddings.
- *Question:* How do we close these gaps coherently for a SQLite-backed, demo-worthy app without violating the boundary rules (no god-objects, contracts stay generated, infrastructure behind ports)?

**Decisions requested:**

| ID | Question | Recommendation | Why | Decide by | Reviewer action |
| --- | --- | --- | --- | --- | --- |
| D1 | Complete the N-API build pipeline so `engram-node` is loadable from Node? | Accept: add `@napi-rs/cli`, a `build:native` script emitting `engram_node.node`, and a real-load smoke test | The binding compiles but cannot load today (spike, Evidence); every demo path depends on it | This RFC | Confirm adding `@napi-rs/cli` as a workspace dev tool |
| D2 | Shape of the extended binding? | Focused native structs (`NativeMemoryEngine` + new `NativeKnowledgeEngine`, `NativeIngestEngine`, `NativeRetrievalEngine`, `NativeTaxonomyEngine`), composed in the Node backend | Alternatives (god-engine, single facade) modeled and rejected in Options ("binding shape"); chosen option matches the existing `NativeMemoryEngine` precedent and the facade rule | This RFC | Rule on granularity — confirm focused structs over a single facade/god-engine (see Options: binding shape) |
| D3 | Add `engram-store-knowledge-sqlite` over a shared SQLite file? | Accept: implement `KnowledgeRepository` + `KnowledgeGraphRepository` (+ `TaxonomyRepository` per D4); **defer `OntologyRepository`** (demo is taxonomy-only). One shared DB file with memory + sqlite-vec, mirroring `engram-store-sql` | Durable KG is the named gap; template + reference impl already exist. Shared-file-with-sqlite-vec-in-connection is contingent on Open question 2 | This RFC | Confirm crate location `adapters/knowledge/sqlite`, the shared-DB choice (contingent on OQ2), and that `OntologyRepository` is deferred |
| D4 | Add a taxonomy persistence port? | Accept: minimal `TaxonomyRepository` trait in `engram-knowledge` + SQLite impl (concept schemes, concepts, relations) | Taxonomy has no port today; "maintain taxonomy" needs it; SKOS-shaped domain types already exist | This RFC | Confirm the minimal port shape |
| D5 | Source of graph content? | Accept: deterministic `GraphExtractor` in `engram-ingest` (code-symbol + document strategies) emitting `KnowledgeEntity`/`KnowledgeRelationship` | Ingest yields chunks only; the graph would be empty otherwise; deterministic keeps it on-philosophy. LLM-based extraction is deferred — a later model-provider adapter can add it behind the same `put_entity`/`put_relationship` ports | This RFC | Confirm deterministic (no LLM) and placement in `engram-ingest` |
| D6 | FastEmbed passage path? | Accept: passage embeddings on ingest (populate `embedding_refs` + sqlite-vec vectors); feature stays gated, demo enables it; hashing-embedding fallback for default/no-model | FastEmbed is query-only and test-only today; closes semantic retrieval end-to-end. Vector storage assumes OQ2's co-loading outcome | This RFC | Confirm feature stays off-by-default in default CI |
| D7 | Demo app shape? | Accept: `demo/backend` (Node/TS HTTP server loading `@engram/node`) + `demo/frontend` (Vite + React + Cytoscape), standalone (not workspace packages, not contract-bearing) | Browser needs an HTTP surface; Node is the intended app layer; Rust stays a library | This RFC | Confirm `demo/` is standalone and non-contract-bearing |
| D8 | Delivery sequence? | Accept five vertical slices: 0 bridge+shell, 1 SQLite KG + taxonomy, 2 extractor, 3 FastEmbed passage, 4 UI polish | Each slice leaves a runnable demo; front-loads the riskiest dependency (the bridge) | This RFC | Confirm slice order |

## Problem & goals

**Problem — six gaps block a browser demo today (all verified):**

1. **No browser → Rust path.** No HTTP server exists anywhere (no axum/actix/hyper; no `[[bin]]`, no `main.rs`). The N-API binding is memory-only and — per the spike — does not actually load from Node.
2. **Binding is memory-only.** `engram-node` exposes only `writeMemoryJson`/`retrieveJson`/`forgetJson` over `SqlMemoryService` (`bindings/node/src/lib.rs`). Ingest, knowledge, retrieval, taxonomy are unreachable from TypeScript.
3. **No durable knowledge backend.** Knowledge/graph/ontology ports exist and an in-memory reference impl exists, but there is no SQLite (or any durable) knowledge adapter.
4. **Taxonomy has no persistence port.** No `TaxonomyRepository` trait exists in `core/`; `ConceptScheme`/`Concept` are referenced via `ConceptRef` but never stored.
5. **Ingest produces no graph.** `KnowledgeIngestor::ingest()` in `adapters/ingest` returns `IngestedKnowledge { source, document, chunks }` (the trait method in `engram-knowledge` returns only chunks); `put_entity`/`put_relationship` default to "unsupported" and nothing calls them. Visualizing "the knowledge graph" would show an empty graph.
6. **FastEmbed is test-only; no passage embeddings.** `FastEmbedBgeSmallQueryProvider` is feature-gated and `#[ignore]`d; nothing constructs it in production; only query embeddings exist, so `KnowledgeChunk.embedding_refs` is never populated and vector retrieval over an ingested corpus cannot run end-to-end.

**Goals:**

- A runnable, client-demo-worthy app: ingest knowledge (files/text) → maintain taxonomy → query (lexical + graph + semantic) → visualize the memory/knowledge graph.
- A real, loadable N-API bridge so Rust is consumed as a library from a Node application server (the architecture the binding was created for).
- Durable SQLite storage for memory, knowledge graph, and vectors — with each concern remaining behind its port so the storage choice is swappable.
- FastEmbed as a real (opt-in) end-to-end retrieval path, not a test stub.

**Non-goals (deliberately not pursued — could-have-been-goals dropped):**

- Production Postgres / pgvector adapters. Separability is a *goal* enforced by the Slice 1 forbidden-import gate (see Proposal), not a promise made here.
- A durable graph database (Neo4j / RDF / property-graph) — the SQLite edge tables are a demo-grade backing store, explicitly flagged as such.
- Ontology management (classes, properties, axioms, validation findings) — the demo exposes **taxonomy only**. The existing `OntologyRepository` port, its in-memory impl, and the `Ontology*` domain types stay in the codebase untouched; `engram-store-knowledge-sqlite` does not implement ontology in this program, and `KnowledgeGraph.ontology_refs` is left empty.
- Any change to v1 JSON contract fields or generated TypeScript types.
- Demo-server production concerns: auth, multi-tenancy, rate-limiting, horizontal scaling.

## Proposal

### Architecture

```text
Browser  (demo/frontend: Vite + React + Cytoscape)
   │  HTTP / JSON
   ▼
Node app server  (demo/backend: TypeScript)
   │  loads native module via @engram/node
   ▼
engram-node  (N-API binding) — EXTENDED, focused structs:
   NativeMemoryEngine (exists) · NativeKnowledgeEngine · NativeIngestEngine
   NativeRetrievalEngine · NativeTaxonomyEngine
   │  JSON round-trips (existing pattern)
   ▼
Rust core (libraries, unchanged boundary rules):
   engram-domain · engram-runtime · engram-memory · engram-knowledge (+ TaxonomyRepository)
   engram-retrieval (composer/weighted fan-in)
   engram-store-sql (memory, adapters/memory/sqlite — exists)
   engram-store-knowledge-sqlite (NEW — SQLite KG + taxonomy)
   engram-ingest (+ GraphExtractor — NEW)
   engram-store-vector  (adapters/retrieval/sqlite-vec — exists; + FastEmbed passage wiring)
```

Rust stays a library. The Node layer is the application server and the only place HTTP lives. The browser is a pure client. (The compiled `.node` is host-specific — OS + arch. The demo is local-first: running it on another host requires rebuilding the native module for that triple; the Slice 0 spec evaluates `@napi-rs/cli` multi-triple prebuilds if cross-platform client demos become a goal.)

### Storage composition (D3)

One shared SQLite database file for the demo, with the **sqlite-vec extension loaded into the same connection** as the memory and knowledge tables — *contingent on Open question 2*. This is a demo convenience only. Two hard rules keep it swappable:

- Memory, knowledge graph, taxonomy, and vectors each live behind their existing/new repository traits. No core crate, no binding method, and no Node endpoint assumes a single file or a single engine.
- The shared file is an **adapter-internal** composition, enforced by a **forbidden-import gate** added in the Slice 1 spec — a new goal-based gate of the same family as `sql-service-conformance`'s checks but `cargo tree` / import-graph based (no such boundary test exists in the repo today): `engram-store-knowledge-sqlite` must not depend on `engram-store-sql` or `engram-store-vector` (and vice versa). "Adapter-internal" degrades into cross-adapter SQL the moment a JOIN looks convenient; the gate exists to make that a build failure, not a code-review hope. If Open question 2's co-loading check fails, the layout becomes one file + a dedicated vector connection — which re-opens the storage-layout Options axis rather than being silently absorbed.

### New port — `TaxonomyRepository` (D4)

Minimal, SKOS-aligned, in `engram-knowledge`, mirroring the existing repository-trait style:

```rust
#[async_trait]
pub trait TaxonomyRepository: Send + Sync {
    async fn put_concept_scheme(&self, scheme: ConceptScheme) -> CoreResult<ConceptScheme>;
    async fn put_concept(&self, concept: Concept) -> CoreResult<Concept>;
    async fn put_concept_relation(&self, relation: ConceptRelation) -> CoreResult<ConceptRelation>;
    async fn get_concept_scheme(&self, id: &ConceptSchemeId, scope: &Scope) -> CoreResult<Option<ConceptScheme>>;
    async fn list_concepts(&self, scheme_id: &ConceptSchemeId, scope: &Scope) -> CoreResult<Vec<Concept>>;
}
```

Implemented by `engram-store-knowledge-sqlite`. Domain types (`ConceptScheme`, `Concept`, `ConceptRelation`) already exist in `engram-domain` (`taxonomy.rs`).

### Deterministic extractor (D5)

New module in `engram-ingest` (`GraphExtractor`) that runs after chunking and persists via the `KnowledgeRepository`:

- **Code-symbol strategy:** reuses the existing `CodeSymbolChunker` detection. Emits `Function`/`Class`/`Struct`/`Trait` entities and `defines` / `calls` / `contains` edges.
- **Document strategy:** emits section/heading entities and `mentions` edges (co-occurrence of recognized terms within a section).

Deterministic, no model calls. Low, fixed edge vocabulary so the graph is predictable and testable. LLM-based extraction is deliberately deferred (D5).

### FastEmbed passage path (D6)

- Introduce a passage-embedding trait alongside the existing `VectorQueryProvider`; `FastEmbedBgeSmallQueryProvider` (or a sibling passage provider) implements it.
- On ingest, embed each `KnowledgeChunk`, populate `embedding_refs`, and write the vector to sqlite-vec.
- Retrieval injects the FastEmbed query provider into `VectorRetrievalIndex`; its candidates feed `engram-retrieval` fusion.
- The feature stays gated (`fastembed-provider`) and off-by-default in CI; the **demo enables it** (model download accepted by decision). A deterministic hashing-embedding fallback keeps default/no-model builds working. Vector storage assumes OQ2's co-loading outcome; a negative result moves vectors to a dedicated connection without changing the trait surface.

### Demo layout (D7)

```text
demo/
  backend/        Node/TS HTTP server: composes native engines, exposes REST
                  (ingest, query, graph, memory, taxonomy). Loads @engram/node.
  frontend/       Vite + React + TypeScript. Cytoscape graph viz, query panel,
                  memory browser, taxonomy view.
  README.md       how to build the .node, run backend + frontend, seed data.
```

`demo/` is standalone: not a `packages/` workspace member, carries no contract, defines no generated types. It depends on `@engram/node` and `@engram/contracts` only.

### Slice plan (D8)

Each slice ends with a runnable demo and its own spec under `docs/specs/`:

- **Slice 0 — Bridge + shell.** Complete N-API pipeline (D1); prove the *existing* memory bridge loads for real and wire `demo/backend` + `demo/frontend` shell (write/retrieve/forget UI). De-risk the foundation. (Binding extension beyond memory begins in Slice 1, after ADR-0007 — see Follow-on artifacts.)
- **Slice 1 — Durable knowledge + taxonomy.** `engram-store-knowledge-sqlite` (D3, with the forbidden-import gate) + `TaxonomyRepository` (D4) + binding `NativeKnowledgeEngine`/`NativeTaxonomyEngine`. UI: browse sources/documents/chunks; maintain taxonomy.
- **Slice 2 — Graph content.** `GraphExtractor` (D5) + binding exposure of entities/relationships/neighbors. UI: Cytoscape graph from real ingestion.
- **Slice 3 — Semantic retrieval.** FastEmbed passage embeddings (D6) + `NativeRetrievalEngine`; wire knowledge-chunk and vector `RetrievalIndex`es into `engram-retrieval` fusion. UI: fused query (lexical + graph + semantic).
- **Slice 4 — Polish.** Graph styling/layout, query explainability (fusion trace), taxonomy editing, memory browser polish. Client-demo-worthy finish.

## Options considered

**Axis: how the browser reaches Rust (transport).** MECE over where the HTTP surface lives.

- **Do-nothing.** No browser demo possible. Cost of delay: the stated goal is unmet.
- **Rust HTTP server (axum).** A new server binary crate. Rejected by the user: the N-API binding was created precisely so Rust is a library and Node is the app layer; a Rust server would duplicate that intent and introduce the repo's first server/async-runtime binary (an avoidable convention change).
- **Extend N-API + Node server (chosen).** Broaden `engram-node`, put HTTP in `demo/backend`. Matches existing architecture; Rust stays a library; reuses `@engram/contracts`. Cost: the binding surface grows (additive) and must stay focused (D2).
- **Tauri/Electron desktop shell.** Embed the native module; no HTTP. Rejected: heavier toolchain, yields a desktop app rather than a web demo.

**Axis: durable storage layout.** MECE over physical separation.

- **Do-nothing (in-memory).** Rejected: not durable, does not meet "SQLite for knowledge graph."
- **Shared SQLite file (chosen).** One file, sqlite-vec in-connection (contingent on OQ2). Simplest local demo, one artifact to back up. Risk: accidental cross-concern coupling — mitigated by the forbidden-import gate (D3).
- **Separate files per concern.** More isolated, but more wiring and multiple connections for no demo benefit. Rejected for the demo; remains trivially reachable later via the port seam (and becomes the layout if OQ2 fails).

**Axis: source of graph content.** MECE over how the graph is populated.

- **Do-nothing (empty graph).** Rejected: visualization has nothing to show.
- **Deterministic extractor (chosen).** Predictable, testable, on-philosophy (no LLM in the deterministic core).
- **LLM-based extraction.** Rejected for this program (Non-goal); a later adapter/model-provider spec can add it behind the same `put_entity`/`put_relationship` ports.

**Axis: binding surface shape.** MECE over the granularity of the native struct(s).

- **Do-nothing (memory-only).** No knowledge/ingest/retrieval/taxonomy in TS; demo impossible past memory.
- **Single god-engine `NativeEngramEngine`.** One struct owning all five services' state and methods. Rejected — it is exactly the no-god-object anti-pattern called out in `AGENTS.md` (a file/struct owning construction, state, orchestration, and persistence across five domains at once).
- **Single facade struct with namespaced methods.** One struct, methods grouped by domain. Rejected — it still owns five services' connections/state, carrying the god-engine's hidden-coupling risk behind a thinner disguise.
- **Focused structs, one per service (chosen).** `NativeMemoryEngine` + `NativeKnowledgeEngine` + `NativeIngestEngine` + `NativeRetrievalEngine` + `NativeTaxonomyEngine`, composed in the Node backend. Each owns one connection surface and one trait family; matches the existing `NativeMemoryEngine` precedent and the facade rule.

## Risks & what would make this wrong

**Pre-mortem (assume it shipped and failed):**

- *The N-API build is brittle across platforms / Node versions.* Mitigation: pin `@napi-rs/cli`; the demo build produces the `.node` for its host triple; Slice 0 proves load before anything else depends on it. (Cross-host is a rebuild by default — see Proposal.)
- *The shared SQLite file becomes a coupling that blocks a later Postgres + pgvector swap.* Mitigation: the forbidden-import gate (D3) makes cross-adapter SQL a build failure; each concern stays behind its port. Reviewer focus #2.
- *The extractor produces a noisy or misleading graph.* Mitigation: deterministic, fixed low edge vocabulary, unit-tested against fixtures; graph is labeled demo-grade.
- *The FastEmbed model download breaks offline/CI.* Mitigation: feature stays off-by-default; hashing-embedding fallback; only the demo opts in.
- *The binding grows into a god-object.* Mitigation: D2 focused-struct rule; reviewer focus #1; alternatives modeled in Options.

**Key assumptions (falsifiable):**

- `@napi-rs/cli` can build the existing `napi`/`napi-derive` 3.x crate into a loadable `.node`. (Standard for napi-rs, but Slice 0 exists to prove it on *this* crate.)
- `sqlite-vec` loads as an extension in the same `rusqlite` (bundled) connection used by memory + knowledge tables. (Already a workspace dependency at `=0.1.9`; this is Open question 2 — a negative result re-opens the storage-layout axis, not silently absorbed.)
- Deterministic code-symbol + document extraction yields a graph useful enough to visualize for a demo. (Believable given the existing `CodeSymbolChunker`; validated by fixtures in Slice 2.)

**Drawbacks:**

- Adds a `demo/` surface to maintain (standalone, low-cost, but non-zero).
- Adds `@napi-rs/cli` as a workspace dev dependency and a Rust build step to the demo's run path; the `.node` is host-specific (rebuild per OS/arch).
- The N-API public surface grows, and `engram-knowledge` gains a new core port — both additive only; no existing method/type changes.
- The demo requires a FastEmbed model download to show semantic search (accepted by decision; lexical + graph paths work without it).

## Evidence & prior art

**Spike / de-risk result (the riskiest assumption).** *The N-API bridge actually loads from Node.* Result: **it does not today.**

- `cargo build -p engram-node` succeeds and emits `libengram_node.so` (cdylib).
- There is **no `@napi-rs/cli` and no `napi build` script** anywhere in the workspace; `packages/node` builds TypeScript only (`"build": "tsup"`).
- No `.node` artifact is produced or placed where the loader expects; `packages/node/src/binding.ts` would throw *"Unable to load @engram/node native addon"* at runtime.
- The green `@engram/node` test runs against an injected `FakeNativeMemoryEngine` (`packages/node/test/transport.test.ts`) — it proves the TS wrapper, not the bridge.

This is why Slice 0 exists and why D1 is non-negotiable.

**Repo precedent:**

- `engram-store-sql` is the SQLite-adapter template (schema-as-JSON-blob + scope index; `engine.rs` dependency injection; `open_file()`/`open_in_memory()` at `adapters/memory/sqlite/src/engine.rs:39,53` — file-backed construction landed in `ddea32d` / `docs/specs/sqlite-file-backed-construction`).
- `engram-store-knowledge-memory` is the behavioral reference for the three knowledge traits.
- `adapters/retrieval/sqlite-vec` + `docs/specs/fastembed-query-provider` define the gated-provider pattern to extend; `docs/specs/ci-vector-feature-gates` governs D6's "off-by-default in CI" promise.
- `core/retrieval` (`composer`/`weighted`/`ports`) already fan-in `RetrievalIndex` candidates; the demo query path plugs indexes into it.
- This program **resolves RFC-0002's open question** ("Should knowledge chunks live in the same SQLite database initially or in a separate adapter?", `docs/rfcs/0002` §Open Questions) in favor of a shared file with separate adapter crates, and diverges from RFC-0002's separate `knowledge/graph` crate sketch by keeping graph tables inside `engram-store-knowledge-sqlite` behind the existing `KnowledgeGraphRepository` port.
- Boundary constraints: `docs/specs/memory-knowledge-boundaries` (no memory↔knowledge persistence coupling), `docs/specs/sql-service-conformance` (no god-module, focused modules), `docs/specs/workspace-responsibility-layout` and `docs/specs/typescript-native-surface` (N-API surface changes and adding a gateway/HTTP-server runtime are *"Ask first"* — hence the follow-on ADR-0007).

**External prior art (honestly sourced):**

- `sqlite-vec` is **already a workspace dependency** (`Cargo.toml`: `sqlite-vec = "=0.1.9"`); using it as a loadable extension in-connection is its intended mode.
- The taxonomy domain types are **already SKOS-shaped** (`engram-domain` `taxonomy.rs`: `ConceptScheme`/`Concept`/`ConceptRelation`); the port follows that established vocabulary.
- `@napi-rs/cli` is the canonical build toolchain for the `napi`/`napi-derive` crates already in use (`napi = "3.9.4"`, `napi-derive = "3.5.7"`, `napi-build = "2.3.2"`). Version/target specifics to be confirmed in the Slice 0 spec (see Open questions); not asserted here.

## Open questions

1. **Exact `@napi-rs/cli` version and target triples.** Default: latest stable `@napi-rs/cli`; build for the host triple for local runs, and evaluate the `@napi-rs/cli` CI prebuild matrix for additional triples only if a cross-platform client demo is required. Owner: author. Decide-by: Slice 0 spec.
2. **Does `sqlite-vec` load cleanly in the same `rusqlite` bundled connection as memory + knowledge, or does it need its own connection?** Default: same connection via `load_extension`; fall back to a dedicated vector connection if there is a conflict (which re-opens the storage-layout axis). Owner: author. Decide-by: Slice 1 spike (Slice 3 consumes the result; Slice 1's storage-layout wiring blocks on it).
3. **Demo backend framework.** Default: Hono (minimal, TS-native, fast) unless a workspace convention emerges. Owner: author. Decide-by: Slice 0 spec.

## Follow-on artifacts

When accepted:

- **ADR-0007: N-API binding public-surface extension** (the *"Ask first"* item from `workspace-responsibility-layout` / `typescript-native-surface`). **ADR-0007 lands before or with Slice 1** — it authorizes the binding extension that Slice 1 begins. Slice 0 (build pipeline + memory-only load) does not need it.
- **Specs (one per slice):**
  - `docs/specs/napi-bridge-completion/` (Slice 0)
  - `docs/specs/sqlite-knowledge-graph/` (Slice 1, includes `TaxonomyRepository` + the forbidden-import gate)
  - `docs/specs/knowledge-graph-extractor/` (Slice 2)
  - `docs/specs/fastembed-passage-embeddings/` (Slice 3)
  - `docs/specs/engram-demo-app/` (Slice 4, frontend/backend wiring + polish)
- No `docs/CONVENTIONS.md` change anticipated — `demo/` is standalone and non-contract-bearing.
