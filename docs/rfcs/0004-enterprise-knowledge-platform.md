# RFC 0004: Enterprise knowledge platform demo

- **Status:** Accepted
- **Author:** phanijapps
- **Approver:** phanijapps
- **Date opened:** 2026-06-30
- **Date closed:** 2026-06-30
- **Decision weight:** heavy
- **Related:** RFC-0003 (durable knowledge demo — the foundation this builds on), ADR-0007 (N-API focused-engine surface), ADR-0005 (storage adapter semantics), ADR-0006 (SQLite adapter); follow-on **ADR-0008** (ontology reversal) lands before Slice 3.

## Reviewer brief

- **Decision:** Approve a program that escalates the RFC 0003 demo from a deterministic, single-file toy into an enterprise-grade knowledge platform — durable at repo scale, LLM-extracted, with belief / contradiction / bi-temporal surfaced (hierarchy **deferred to TBD — documented, not dropped**), a **minimalist, enterprise-ready 3D visualization with navigable links**, ontology + taxonomy for an IT org, and Q&A over the graph.
- **Recommended outcome:** accept.
- **Change if accepted:**
  - New TS `ModelProvider` integration in `demo/backend` (pi SDK → ollama cloud → `gemma4:31b-cloud`) driving LLM extraction and Q&A; deterministic `GraphExtractor` stays the zero-credential baseline.
  - Reverses RFC 0003's ontology non-goal: `OntologyRepository` lands in `engram-store-knowledge-sqlite` + binding + UI + an IT-org sample (ADR-0008).
  - Durable backends for belief + contradiction (today in-memory only; `BeliefRepository` port in `engram-orchestration`) in a **new** boundary-respecting SQLite adapter + binding + UI, driven by the existing consolidation pipeline; bi-temporal surfaced as honest display-only.
  - Backend scan job for folder/repo ingestion (`.gitignore`-aware, batched, incremental) + a **minimalist, enterprise-ready 3D visualization with navigable links** replacing Cytoscape.
- **Affected surface:** `demo/backend` (LLM + scan job + Q&A), `demo/frontend` (3D viz, new panels), `adapters/knowledge/sqlite` (ontology tables), a **new belief SQLite adapter** (belief + contradiction — `BeliefRepository` port in `engram-orchestration`, kept distinct from knowledge per AGENTS.md), `bindings/node` (additive focused engines), `core/knowledge` (ontology port already exists).
- **Stakes:** costly-to-reverse. The ontology reversal reopens a frozen RFC 0003 boundary (mitigated by ADR-0008 + the existing forbidden-import gate). The LLM integration is a new external boundary (mitigated by the `ModelProvider` trait + deterministic fallback). Rust changes are additive *except* the idempotent ontology-table migration on existing demo DBs; no existing type or method changes.
- **Review focus:**
  1. The ontology reversal is deliberate and recorded (D1; ADR-0008) — not a quiet drift from RFC 0003.
  2. The LLM integration stays in TypeScript behind a `ModelProvider` trait, never leaking into Rust (D2/D3) — Rust stays deterministic.
  3. Hierarchy is **deferred to TBD and documented**, not silently dropped (D6); bi-temporal is honestly display-only (no `transaction_time`) — the demo must not overclaim.
- **Not in scope:** **hierarchical knowledge graph (deferred to TBD)** — the `HierarchyRepository` / `HierarchyBuilder` ports exist but are not surfaced in this program, recorded as deferred below, not dropped; bi-temporal time-travel / `transaction_time` / temporal queries; LLM-based belief synthesis; production Postgres / pgvector; a durable graph DB; multi-tenant auth, rate-limiting, or scaling on the demo server; a Rust LLM client / async-runtime; cross-host native prebuilds (carried from RFC 0003).

## The ask

**Recommendation (BLUF):** Approve the program above, delivered as seven vertical slices where each leaves a runnable, increasingly-impressive demo. Start with Slice 0 — a **minimalist, enterprise-ready 3D graph with navigable links** (borrowing agentzero's deterministic layout technique) — because it is the visible "stunning" win, is frontend-only (lowest risk), and every later slice's depth (provenance, confidence) builds on that canvas.

**Why now (SCQA):**
- *Situation:* RFC 0003 shipped a runnable demo — durable SQLite knowledge graph, deterministic extractor, FastEmbed semantic search, a focused N-API binding, and a Vite+React+Cytoscape UI. Engram's *differentiating* concepts (hierarchy, belief, contradiction, bi-temporal) exist as domain types and in-memory repos but are **not reachable** from the demo: no durable backend, no binding exposure, no UI.
- *Complication:* A client-grade enterprise demo must show what Engram is *for* — index an organization's docs and a polyglot repo, build a knowledge graph (with LLM-extracted relationships), surface beliefs and contradictions across sources, and answer questions over the graph — visualized in a way that lands. None of that is reachable today, and the current Cytoscape panel does not meet the bar — a minimalist, enterprise-ready 3D graph with navigable links (`~/projects/agentzero/apps/ui` sets the layout-technique reference).
- *Question:* How do we close this coherently for an enterprise-grade, durable, LLM-powered demo — surfacing Engram's real differentiators — without violating the boundary rules (contracts stay generated, infrastructure behind ports, no god-objects, model integrations in TS not Rust)?

**Decisions requested:**

| ID | Question | Recommendation | Why | Decide by | Reviewer action |
| --- | --- | --- | --- | --- | --- |
| D1 | Reverse RFC 0003's ontology non-goal — implement `OntologyRepository` (SQLite + binding + UI) + IT-org sample? | Yes; record **ADR-0008** | You asked for an IT-org ontology; domain types + in-memory impl already exist; the RFC 0003 deferral was demo-scoping, not principle | This RFC | Rule on the reversal — confirm ADR-0008 records it before Slice 3 |
| D2 | LLM relationship-extraction approach? | pi SDK (`@earendil-works/pi-coding-agent`) → ollama cloud → `gemma4:31b-cloud`, behind a TS `ModelProvider` trait; deterministic `GraphExtractor` stays the zero-cred baseline with an "enhance" toggle | Viable (spiked); showcases the integration you named; deterministic fallback keeps the demo runnable without cloud creds | This RFC | Confirm pi SDK as the LLM client + deterministic-fallback policy |
| D3 | Where does LLM work live? | Node/TS backend (`demo`), behind a `ModelProvider` trait — never in Rust | LLM clients are JS; AGENTS.md puts model integrations in TS/adapter crates; keeps the Rust core deterministic | This RFC | Confirm TS-side model integration (no Rust LLM/async-runtime) |
| D4 | Scale ingestion + incremental re-index? | Backend scan job: walk folder/repo, `.gitignore`-aware, raised file limits, batched embedding, streamed progress; git-aware changed-files-only re-index | "Point to a repo and index it" needs a scan job, not per-file calls; incremental makes re-indexing a real repo feasible | This RFC | Confirm scan-job shape + incremental scope |
| D5 | Enterprise visualization? | A **minimalist, enterprise-ready 3D graph** (`react-three-fiber` + `drei`, borrowing agentzero's deterministic Fibonacci-sphere layout) replacing Cytoscape — restrained palette, clean type, minimal chrome, **navigable links** (click a node → details + source hyperlink; click an edge → relationship provenance); encode belief confidence + source provenance | "Stunning but minimal, enterprise-ready, with links" is your bar; same stack as the demo (Vite+React); Engram already emits the entity/relationship shape it consumes | This RFC | Confirm the minimalist-enterprise aesthetic + Cytoscape removal |
| D6 | Surface belief / contradiction / bi-temporal (defer hierarchy)? | Durable backend for belief + contradiction in a **new** SQLite adapter (`BeliefRepository` port in `engram-orchestration`, distinct from knowledge) + binding + UI, driven by the existing consolidation pipeline; surface valid_time ranges (display). **Hierarchy is deferred to TBD** (`HierarchyRepository`/`HierarchyBuilder` ports exist but are not surfaced) — recorded as a documented non-goal, not silently dropped | Belief/contradiction are in-memory only today; a durable backend + consolidation make them demo-real. Honest limits: no `transaction_time`/temporal queries (verified — none exists); hierarchy deferred per your call | This RFC | Confirm belief/contradiction durable scope, bi-temporal display-only, **and** the documented hierarchy deferral |
| D7 | Query/answer over knowledge + memory? | LLM synthesis over retrieved knowledge (graph/chunks) **and** memory → grounded answer with provenance citations (same `ModelProvider`/gemma4 as D2 — ollama cloud's query role) | It is the "why build a knowledge graph" payoff; the demo only has deterministic search today | This RFC | Confirm the query/answer scope over knowledge **and** memory |
| D8 | Delivery sequence? | Seven slices (see Proposal) | Front-loads the visible win (UI) and the foundation (ingest), defers the highest-risk (LLM) and the capstone (Q&A) until after de-risk | This RFC | Confirm slice order |

## Problem & goals

**Problem — five gaps block an enterprise demo today (verified against the shipped RFC 0003 demo):**

1. **Differentiators are unreachable.** Belief, contradiction, and bi-temporal exist as domain types + in-memory repositories only (hierarchy does too, but it is deferred to TBD this program). They have no durable SQLite backend, no N-API binding method, and no UI — the demo cannot show what makes Engram distinct.
2. **No LLM extraction.** Only the deterministic `GraphExtractor` runs (code symbols; prose co-occurrence). Relationship extraction across a real corpus of docs/prose needs an LLM; there is no model integration anywhere.
3. **Ingestion is single-file.** There is no "point to a folder/repo and index it" path — only per-file `ingestExtractJson`. Hard limits (1 MB/file, no `.gitignore` filtering, no progress, no incremental re-index) make a real repo infeasible.
4. **Visualization undersells.** The Cytoscape panel is functional but does not meet the bar — a minimalist, enterprise-ready 3D graph with navigable links — and encodes none of belief confidence or source provenance.
5. **No answering.** The demo retrieves (lexical + graph + semantic) but does not synthesize — there is no "ask the knowledge base" path, which is the payoff a knowledge graph exists to enable.

**Goals:**

- A client-demo-worthy enterprise app: index docs **and** a polyglot repo → LLM-extract a knowledge graph → maintain an IT-org ontology + taxonomy → surface beliefs, contradictions, and time → visualize it in a 3D graph → ask questions answered from the graph.
- LLM extraction and Q&A via the pi SDK → ollama cloud → `gemma4:31b-cloud`, with the deterministic path always available so the demo runs without cloud credentials.
- The differentiating concepts (belief, contradiction, bi-temporal) made **demo-real**: durable, reachable from the binding, and visible in the UI — with honest limits where the underlying capability is not yet built. (Hierarchy is deferred to TBD — documented, not dropped.)

**Non-goals (deliberately not pursued — could-have-been-goals dropped):**

- **Hierarchical knowledge graph — deferred to TBD.** The `HierarchyRepository` and `HierarchyBuilder` ports and the `engram-domain` hierarchy types exist, but this program does **not** surface hierarchy (no durable backend, no binding, no UI). It is recorded here as deferred, not silently dropped; a future slice/RFC activates the existing ports rather than redesigning them. (Your call: keep it TBD for now.)
- **Bi-temporal time-travel.** Only `valid_from`/`valid_until` are displayed. There is no `transaction_time` and no temporal-query support; "as-of" queries are out of scope.
- **LLM-based belief synthesis.** Existing belief types and their in-memory synthesis are surfaced; LLM-driven belief formation is deferred.
- **Production concerns** carried from RFC 0003: Postgres/pgvector adapters, a durable graph DB (Neo4j/RDF/property-graph), multi-tenant auth, rate-limiting, horizontal scaling.
- **A Rust LLM client / async-runtime.** Model integration stays in TypeScript.
- **Cross-host native prebuilds.** The `.node` is host-specific; running on another host rebuilds it (RFC 0003).

## Proposal

### Architecture

```text
Browser  (demo/frontend: Vite + React + react-three-fiber 3D graph)
   │  HTTP / JSON
   ▼
Node app server  (demo/backend: TypeScript)
   ├── ModelProvider trait  ←── pi SDK → ollama cloud → gemma4:31b-cloud
   │     (extraction · Q&A · belief-enhance)   [+ deterministic stub for zero-cred]
   ├── scan job  (folder/repo walk · .gitignore · batched embed · progress · incremental)
   └── composes native engines via @engram/node
   ▼
engram-node  (N-API binding) — ADDITIVE focused engines (ADR-0007 rule):
   NativeKnowledgeEngine (+ ontology put/get) · NativeIngestEngine (exists)
   NativeMemoryEngine · NativeRetrievalEngine (+ durable vector)
   + NativeBeliefEngine (belief + contradiction — mirrors BeliefRepository)
   (hierarchy deferred — HierarchyRepository/HierarchyBuilder ports exist, not surfaced)
   │  JSON round-trips (existing pattern)
   ▼
Rust core (libraries, unchanged boundary rules):
   engram-store-knowledge-sqlite (+ OntologyRepository tables only)
   engram-store-belief-sqlite (NEW — BeliefRepository; distinct from knowledge)
   engram-store-sql (memory) · engram-store-vector (sqlite-vec + FastEmbed passage)
   engram-orchestration (Belief/Hierarchy/Consolidation ports + DryRun/Gated services)
   engram-domain (ontology/belief/contradiction/hierarchy/bi-temporal types already exist)
   engram-knowledge (OntologyRepository port exists)
   engram-ingest (GraphExtractor — deterministic baseline)
```

Rust stays a library; the Node layer is the only place HTTP and LLM calls live. The browser is a pure client. Each new capability follows RFC 0003's pattern: a focused module in the right crate, a port where storage-neutral, a JSON-transport binding method, and a UI panel.

### LLM integration — `ModelProvider` in TypeScript (D2, D3, D7)

`ollama cloud` / `gemma4:31b-cloud` has **exactly two LLM jobs** in this program — and nothing else: (1) **extract** entities + relationships on ingest (D2), and (2) **query/answer** over the indexed knowledge **and** memory (D7). Retrieval itself stays deterministic (FastEmbed semantic + lexical + graph); the LLM synthesizes answers *over* retrieved context — it does not replace the retriever. A small trait in `demo/backend` (the correct home for a model integration per AGENTS.md):

```ts
interface ModelProvider {
  extract(text: string, hint?: ExtractHint): Promise<ExtractedGraph>; // entities + relationships
  answer(query: string, context: RetrievedContext): Promise<GroundedAnswer>; // D7 Q&A
}
```

- **pi SDK implementation** (`@earendil-works/pi-coding-agent`): `createAgentSession({ model: getModel("ollama-cloud", "gemma4:31b-cloud"), tools: [], sessionManager: SessionManager.inMemory() })` → `session.prompt(extractionPrompt)` → capture `text_delta` events → JSON-parse into `KnowledgeEntity`/`KnowledgeRelationship` → write via `NativeKnowledgeEngine.putEntityJson`/`putRelationshipJson`. Auth via `auth.json` / env (ollama-cloud key).
- **Deterministic implementation**: the existing `GraphExtractor` (zero cloud credentials). The UI offers an "LLM enhance" toggle; extraction always runs deterministic first, then optionally enhances.
- **Fallback**: if the pi agent harness proves too heavy for repo-scale batch extraction (concrete trigger: per-file extraction latency exceeding the deterministic baseline by more than ~5 s, or session-setup overhead dominating), a direct OpenAI-compatible ollama-client `ModelProvider` is a same-day swap behind the same trait.

### Scale ingestion + incremental re-index (D4)

A backend scan job (not per-file calls): walk a folder or git repo, filter via `.gitignore` + raised size limits, run `ingestExtractJson` per file with batched FastEmbed embedding, and stream progress to the UI. Two distinct capabilities live here — the **scan job** (walk + filter + ingest + progress) and **incremental re-index** (git-aware, changed-files-only, keyed on the existing source-version field) — sequenced so the scan job ships first and incremental layers on it; together they make re-indexing a real repo feasible rather than a full re-walk.

### Ontology + taxonomy for an IT org (D1)

`OntologyRepository` (port already in `engram-knowledge`; types + in-memory impl already in `engram-domain`) implemented in `engram-store-knowledge-sqlite` + binding (`putOntology*Json`) + a UI editor. Authored IT-org sample data: classes (Service, Team, Incident, Runbook, Dependency) + properties; taxonomy (severity, service-tier, environment). Loaded on first run / via a seed route. **Schema migration:** the adapter's create-on-open path adds the ontology tables idempotently (RFC 0003's schema-as-JSON-blob pattern), so existing `demo-engram.db` files upgrade in place without data loss. The reversal of RFC 0003's deferral is recorded in **ADR-0008**.

### Knowledge-depth surface (D6)

Belief and contradiction are **orchestration** concepts — their ports (`BeliefRepository`, `BeliefSynthesizer`, `ContradictionDetector`, `ConsolidationService`) live in `engram-orchestration`, and AGENTS.md keeps them **distinct from knowledge**. So:

- **Belief + contradiction:** the in-memory `BeliefRepository` impl exists but is **not durable** (lost on restart), and the distinctness boundary forbids folding these into the knowledge adapter — so a **new** belief SQLite adapter implements `BeliefRepository` durably, behind the orchestration ports + a forbidden-import gate. The existing consolidation pipeline (`DryRunConsolidationService` / `GatedConsolidationService`) drives `BeliefSynthesizer` + `ContradictionDetector`; results persist and surface in the UI (contradictions list; belief network with confidence) via a focused `NativeBeliefEngine`. Deterministic detectors run by default; the TS `ModelProvider` may additionally propose beliefs / semantic contradictions, writing them back via the binding (LLM stays in TS — D3).
- **Bi-temporal:** display `valid_from`/`valid_until` on beliefs/memories (the only temporal fields in `engram-domain` today). **No `transaction_time`, no temporal queries** — adding `transaction_time` / "as-of" queries is a future core change, deferred here (non-goal).
- **Hierarchy (deferred to TBD):** the `HierarchyRepository` and `HierarchyBuilder` ports and the `engram-domain` hierarchy types exist, but this program does **not** surface them — no durable backend, no binding, no UI. The deferral is recorded in Non-goals; a future slice or RFC activates the existing ports rather than redesigning them.

### Enterprise 3D visualization (D5)

A **minimalist, enterprise-ready 3D graph** on the demo frontend (same Vite+React stack). Borrow agentzero's deterministic Fibonacci-sphere *layout* (stable positions, no physics sim), but skin it restrained: a neutral/dark palette with a single accent, clean typography, minimal chrome, generous whitespace, smooth (not flashy) camera motion — *stunning through restraint, not effects*. **Navigable links throughout**: click a node → a detail panel with a source hyperlink (file:line / chunk / doc); click an edge → relationship provenance; breadcrumbs + back-links for traversal; keyboard-navigable. Consumes the entity/relationship shape Engram already produces. Encodes **provenance + confidence** subtly (node sizing/weight by belief confidence; source link on hover). Cytoscape is removed.

### Query + answer over knowledge and memory (D7)

The second LLM job: the user asks a question; the backend retrieves relevant knowledge (entities/relationships/chunks via the deterministic retriever) **and** memories, then the same `ModelProvider` (gemma4) synthesizes a grounded answer returned with provenance citations back to sources/chunks/memories. This is the "query the knowledge base" path — ollama cloud's query role, distinct from its extraction role (D2).

### Security & data-custody controls (acceptance criteria for the slice specs)

The LLM integration and the scan job open two new boundaries. The RFC names the controls; the slice specs carry them as acceptance criteria:

- **Third-party data-flow disclosure (Slice 2/6).** Ingested documents and indexed source code are sent to ollama cloud for LLM processing. The UI must disclose this and require explicit opt-in before any cloud LLM call; the deterministic path is the default. (OWASP A01:2025, API7:2023.)
- **Scan-job path confinement (Slice 1).** Canonicalize every file path and verify the resolved path stays under the configured root before reading — reject `..` traversal and symlink escape. (CWE-22, CWE-73.)
- **Secret-laden file blocklist (Slice 1).** A default blocklist (`.env`, `*.key`, `*.pem`, `id_rsa`, `*.cert`, `*.p12`) excludes secret-bearing files from ingestion; overridable, with the override off by default. (CWE-312, OWASP A02:2025.)
- **File-size bounds (Slice 1).** Enforce a per-file size limit before reading; skip oversized files with a logged warning. (CWE-770, OWASP A04:2025.)
- **LLM-output validation before graph write (Slice 2).** LLM JSON is validated against a strict schema (type checks, field allowlists, size bounds) before `putEntityJson`/`putRelationshipJson`; untrusted content never writes the graph unchecked. (OWASP LLM01, CWE-502.)
- **LLM call bounds (Slice 2).** Per-call timeout (~30 s) + response-size limit (~100 KB). (OWASP LLM06, CWE-770.)
- **Credential isolation (Slice 2).** The ollama-cloud key is read server-side only, never sent to the frontend, and redacted from logs/error messages; missing credentials trigger a silent deterministic fallback with a one-time console notice, not an error traceback. (CWE-532, CWE-312.)
- **Supply-chain review (Slice 2).** `@earendil-works/pi-coding-agent` is pinned to a specific version with integrity check and reviewed (its transitive deps are broad); `pnpm audit` runs in CI. (OWASP A10:2025.)

### Slice plan (D8)

Each slice ends with a runnable demo and its own spec under `docs/specs/`:

- **Slice 0 — Enterprise 3D UI.** Build the minimalist, enterprise-ready 3D graph (D5) over the existing entity/relationship data, with navigable links; remove Cytoscape. Frontend-only and isolated (cannot block backend slices), but it establishes the 3D canvas **and** the entity/relationship data-shape contract (confirm Engram's `KnowledgeEntity`/`KnowledgeRelationship` map to agentzero's `{id,name,entity_type,mention_count,…}` shape) that Slice 4's provenance/confidence encoding builds on — which is why it goes first.
- **Slice 1 — Scale ingestion + repo indexing.** Backend scan job + `.gitignore` filter + raised limits + batched embedding + streamed progress + git-aware incremental re-index (D4); carries the path-confinement / secret-blocklist / size-bound controls above. UI: point to a folder/repo, watch it index.
- **Slice 2 — LLM relationship extraction.** `ModelProvider` (pi SDK → ollama cloud → gemma4:31b-cloud) + deterministic baseline + "enhance" toggle (D2/D3); carries the output-validation / call-bounds / credential-isolation / supply-chain controls above. **Deployment gate:** before the LLM path ships, verify ollama-cloud auth resolves and `getModel("ollama-cloud","gemma4:31b-cloud")` returns a callable model in the target environment (Open question 1) — a negative result keeps the deterministic path and triggers the direct-ollama fallback. UI: deterministic-vs-LLM graph comparison.
- **Slice 3 — Ontology + taxonomy (IT org).** `OntologyRepository` SQLite impl + idempotent schema migration + binding + UI editor + IT-org sample (D1). **ADR-0008 lands here.**
- **Slice 4 — Provenance + confidence viz.** Encode belief confidence + source provenance in the 3D graph and wire the navigable links to live sources (D5); no hierarchy (deferred). UI: sourced, confidence-aware graph.
- **Slice 5 — Belief + contradiction + bi-temporal.** New belief SQLite adapter (`BeliefRepository`) + consolidation wiring + focused binding engine (`NativeBeliefEngine`) + UI panels (contradictions, belief network, valid_time ranges) (D6). The differentiators made demo-real. (Hierarchy remains deferred to TBD.)
- **Slice 6 — Query + answer over knowledge and memory.** LLM synthesis (gemma4) over retrieved knowledge + memory, grounded answers with citations (D7) — ollama cloud's query role. The capstone.

## Options considered

**Axis: LLM client (D2).** MECE over where the model call originates, *scoped to the user's chosen model (`gemma4:31b-cloud` via ollama cloud)* — other providers (OpenAI/Anthropic, local llama.cpp/vLLM) are out of scope by the user's constraint, not by exhaustion.
- **Do-nothing (deterministic only).** Rejected: relationship extraction across real docs/prose needs an LLM; the deterministic extractor is code-symbol + co-occurrence only.
- **pi SDK (chosen).** `@earendil-works/pi-coding-agent` driving `gemma4:31b-cloud`. Viable (spiked); showcases the integration the user named. Cost: it is a coding-agent harness (heavier than a completion call) and needs ollama-cloud auth.
- **Direct ollama client.** OpenAI-compatible call, ~10 lines, fittest for batch. Rejected as the *primary* only because the user named the pi SDK; retained as the documented same-day fallback if pi is too heavy for repo-scale batch.
- **Rust model adapter.** Rejected: pulls an async-runtime + HTTP client into the Rust core, against the "Rust is a library" + "model integrations in TS" decisions (D3).

**Axis: model-integration home (D3).** MECE over the layer.
- **TypeScript backend behind a trait (chosen).** Correct home per AGENTS.md; keeps Rust deterministic; swappable + testable (deterministic stub).
- **Rust adapter behind a port.** Rejected (above).

**Axis: visualization (D5).** MECE over the rendering approach.
- **Do-nothing (keep Cytoscape).** Rejected: does not meet the minimalist-enterprise bar; encodes no depth.
- **Minimalist 3D on agentzero's layout technique (chosen).** Borrow the deterministic Fibonacci-sphere positioning; skin it restrained/enterprise with navigable links. Same stack, proven layout, consumes Engram's existing data shape.
- **Port agentzero's full cinematic aesthetic.** Rejected: the cream-on-dark glassmorphism is striking but maximalist — you asked for minimalist-stunning-enterprise.
- **Custom WebGL from scratch.** Rejected: reinvents a proven solution.

**Axis: ontology (D1).** MECE over how an IT-org ontology is represented.
- **Reverse the deferral, implement `OntologyRepository` (chosen).** Types + in-mem already exist; user asked for it; honest and durable.
- **Fake ontology in the UI.** Rejected: mislabels structure; undermines the enterprise pitch.
- **Keep deferring.** Rejected by the user's explicit ask.

**Axis: hierarchy in this program (D6).** MECE over how much hierarchy to surface now.
- **Defer entirely to TBD (chosen).** The ports exist; this program does not surface them. Honest, bounds scope, and leaves the ports ready for a future slice.
- **Visual grouping by class only.** Cheap, but still requires hierarchy wiring + UI the program is otherwise avoiding; rejected as a half-measure.
- **Build real aggregation/clustering.** Rejected for this program (unbuilt core) — a future core RFC.

## Risks & what would make this wrong

**Pre-mortem (assume it shipped and failed):**

- *LLM extraction is too slow/noisy at repo scale under the pi agent harness.* Mitigation: deterministic baseline always runs first; direct-ollama-client fallback behind the same trait; batch sizing + per-file extraction keep the unit small.
- *ollama-cloud credentials are unavailable in the demo environment.* Mitigation: the deterministic path is the default; the demo runs end-to-end with zero cloud creds; LLM paths degrade gracefully.
- *The ontology reversal destabilizes the storage boundary.* Mitigation: ADR-0008 records the reversal; the RFC 0003 forbidden-import gate still forbids cross-adapter SQL; ontology stays behind its port.
- *Belief/contradiction durable backends are bigger than estimated.* Mitigation: Slice 5 is sequenced late; if it slips, the demo can fall back to in-memory belief/contradiction for the session (honest, labeled) without blocking the rest.
- *Folding belief/contradiction into the knowledge adapter would breach the distinctness boundary.* Mitigation: they go in a **new** belief SQLite adapter behind the `engram-orchestration` `BeliefRepository` port, enforced by a forbidden-import gate mirroring RFC 0003's; Open question 2 pins the crate shape.
- *The 3D viz port is heavier than estimated.* Mitigation: it is proven in agentzero on the same stack; Slice 0 is frontend-only and isolated, so it cannot block the backend slices.
- *Scope creep across 8 decisions / 7 slices.* Mitigation: each slice is independently shippable and leaves a runnable demo; slices are sequenced to front-load value and back-load risk.

**Key assumptions (falsifiable):**

- The pi SDK's `getModel("ollama-cloud", "gemma4:31b-cloud")` resolves to a working model in the user's environment (auth configured). (Open question 1; deterministic fallback covers a miss.)
- Deterministic + LLM extraction yields a graph useful enough to visualize and answer over at repo scale. (Validated incrementally per slice.)
- The agentzero 3D approach ports cleanly to the demo's Vite+React frontend. (Same stack; de-risked by the reference existing.)

**Drawbacks:**

- Adds a TS `ModelProvider` + pi SDK dependency to `demo/backend` (demo-only, non-contract-bearing).
- Grows `engram-store-knowledge-sqlite` with **ontology** tables (an idempotent schema migration on existing demo DBs, not purely additive) and the binding with additive methods; belief/contradiction go in a **new** adapter — no existing type/method changes anywhere.
- The demo now has an external cloud dependency for its headline features (mitigated by the deterministic baseline).
- Honest under-claiming: hierarchy is deferred (not shown at all) and bi-temporal is display-only — a reviewer must accept that the demo *shows* belief/contradiction durably and bi-temporal as display, and defers hierarchy entirely.

## Evidence & prior art

**Spike / de-risk result (the riskiest assumption).** *The pi SDK can drive `gemma4:31b-cloud` for structured extraction.* Result: **designed and API-confirmed, not yet deployment-verified.** Fetched https://pi.dev/docs/latest/sdk — `createAgentSession()` returns an `AgentSession`; `session.prompt(text)` streams `text_delta` events; the model is selected via `getModel(provider, id)` from a `ModelRegistry` over `AuthStorage`. The extraction path (prompt → capture `text_delta` → JSON-parse → write via `putEntityJson`/`putRelationshipJson`) is confirmed viable at the API level. The pi SDK is a *coding-agent harness* (ResourceLoader, AuthStorage, ModelRegistry, SettingsManager, compaction) — heavier than a plain completion call, which is why the deterministic `GraphExtractor` stays the baseline and a direct-ollama-client fallback is documented. The remaining gate is a **deployment dependency** (ollama-cloud credentials + the exact model id), explicitly *not* design-risk — Slice 2 carries a deployment-verification gate before the LLM path ships (Open question 1).

**Repo precedent:**

- **RFC-0003** is the foundation: the loadable N-API bridge (plain cdylib + `build:native`), `engram-store-knowledge-sqlite` (the KG + taxonomy adapter to extend), the deterministic `GraphExtractor`, FastEmbed passage embeddings, and the Vite+React+Cytoscape demo. This program is its escalation.
- **ADR-0007** sets the focused-native-engine rule every new binding method follows (no god-engine).
- `engram-domain` carries the `Ontology*`, belief, contradiction, hierarchy, and bi-temporal (`valid_from`/`valid_until`) types; `engram-knowledge` carries the `OntologyRepository` port; `engram-orchestration` carries `BeliefRepository`, `HierarchyRepository`, `BeliefSynthesizer`, `ContradictionDetector`, `HierarchyBuilder`, and the `DryRun`/`Gated` consolidation services. This program makes **belief/contradiction** durable and reachable; **hierarchy** ports exist but are deferred to TBD (documented).
- `adapters/knowledge/sqlite` is extended with **ontology** tables only (behind the RFC 0003 forbidden-import gate); belief/contradiction durable storage goes in a **new** belief adapter, keeping those concepts distinct from knowledge (Open question 2). Hierarchy stays deferred.
- `adapters/ingest` `GraphExtractor` + readers + `CodeSymbolChunker` are the deterministic baseline and the scan-job substrate.

**External prior art (fetched/confirmed):**

- **pi.dev SDK** (`@earendil-works/pi-coding-agent`) — coding-agent harness; `createAgentSession`/`prompt`/events; `getModel(provider, id)`; auth via `auth.json`/env. Confirmed from https://pi.dev/docs/latest/sdk.
- **agentzero `apps/ui`** (local reference, `~/projects/agentzero/apps/ui`) — 3D Fibonacci-sphere graph via `react-three-fiber` + `drei`; we borrow the **deterministic layout technique + data shape** `{entities:[{id,name,entity_type,mention_count,first/last_seen_at}], relationships:[{source_entity_id,target_entity_id,relationship_type,mention_count}]}` (which Engram already emits), **not** its cream-on-dark cinematic aesthetic — this demo skins it minimalist/enterprise per D5.
- **ollama cloud / gemma4** — `gemma4:31b-cloud` is available via ollama cloud (its multimodal capability is unused here; the demo is text-only). Exact provider/model id strings to confirm at Slice 2 (Open question 1).
- `react-three-fiber` + `drei` — standard, well-established React 3D stack.

## Open questions

1. **Exact `ollama-cloud` provider id and `gemma4:31b-cloud` model id in pi's `ModelRegistry`.** Default: confirm via `modelRegistry.getAvailable()` at Slice 2 build; if the id differs, fall back to the deterministic extractor + a direct-ollama-client `ModelProvider` behind the same trait. Owner: author. Decide-by: Slice 2.
2. **Durable belief/contradiction storage: a new belief SQLite adapter, or fold into an existing adapter?** Default: a new adapter (e.g. `engram-store-belief-sqlite`) implementing `BeliefRepository`, behind the `engram-orchestration` port + a forbidden-import gate — explicitly **not** folded into `engram-store-knowledge-sqlite` (the distinctness boundary in AGENTS.md). Owner: author. Decide-by: Slice 5.

(Hierarchy is deferred to TBD — recorded in Non-goals; not an open question.)

## Follow-on artifacts

When accepted:

- **ADR-0008: Reverse RFC 0003's ontology non-goal.** Records that `OntologyRepository` is now in scope and implemented in `engram-store-knowledge-sqlite`. **Lands before or with Slice 3.**
- **Specs (one per slice):**
  - `docs/specs/enterprise-3d-graph/` (Slice 0)
  - `docs/specs/scale-repo-ingestion/` (Slice 1)
  - `docs/specs/llm-relationship-extraction/` (Slice 2, includes the `ModelProvider` trait + pi SDK integration)
  - `docs/specs/it-org-ontology/` (Slice 3, includes ADR-0008)
  - `docs/specs/provenance-confidence-viz/` (Slice 4)
  - `docs/specs/belief-contradiction-bitemporal/` (Slice 5, new belief adapter + consolidation + binding + UI)
  - `docs/specs/qa-over-knowledge/` (Slice 6)
- No `docs/CONVENTIONS.md` change anticipated — `demo/` stays standalone and non-contract-bearing; the ontology reversal is recorded in ADR-0008, not a convention edit.
