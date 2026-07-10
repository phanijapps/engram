# Brief: Engram host-application SDK (backend-neutral provider)

- **Slug:** `engram-host-sdk`
- **Received:** 2026-07-09
- **Owner:** engram core team (from an external consumer brief)
- **Source:** `/home/videogamer/Documents/engram-host-application-requirements.md`
- **Shape:** A — no stories; spec boundaries derived from Outcome + Scope, coverage is spec-granular.

> This brief turns a consumer-authored requirements packet into shippable
> specs. The consumer uses engram **as a library** and wants a backend-neutral
> memory/knowledge persistence engine so that swapping storage (SQLite → a
> future backend) is an engram-internal config/crate change, never an
> application rewrite. Decomposition is grounded in a code-level gap analysis
> (see _Current state_); most capability areas already exist as Rust ports and
> are surfaced through the existing `core/integration` provider facade — the
> work is **completion + exposure + contract-hardening**, largely not greenfield.

## Outcome

A host application can open **one** engram provider (`EngramProvider`) from a
typed config and use memory, graph, evidence, ontology, taxonomy, belief, and
recall features through backend-neutral Rust contracts — without ever touching
a backend-specific database handle, connection, migration, vector index, or
graph table. Unsupported capabilities fail explicitly with typed errors
(never silent fallback), and the backend-abstraction contract is proven by a
backend-parametric conformance suite that the SQLite backend passes today and
any future backend must pass to be selectable.

## Success metrics

> Not given by the brief. Proposed defaults — correct me:

- A host integration test opens `EngramProvider` with an in-memory/file SQLite
  config and exercises every **Supported** capability with zero backend-specific
  types in scope.
- Every one of the 18 capability areas has an explicit `Supported` or
  `Unsupported(reason)` entry in `CapabilityReport` (no area silently absent).
- The backend conformance suite runs green against SQLite for every Supported
  area; the suite contains no SQLite-specific symbols in its contract layer.
- A second backend, when added later, is selected purely by config and passes
  the same suite without application-layer changes (validated by a stub/mock
  backend in the conformance tests, not by building SurrealDB).

## Scope / Non-goals

**In scope:**

- `EngramProvider` (core/integration) as the canonical, documented Rust SDK
  entry point, opened from `EngramConfig`.
- A complete `CapabilityReport` covering all 18 brief capability areas with
  explicit support states and typed reasons.
- Completing genuine capability gaps: episode/evidence API exposure,
  cross-store atomic batch, unified recall, export/import + migration,
  observability.
- A backend-parametric conformance suite that proves the backend-abstraction
  contract (SQLite is the reference backend).

**Non-goals (confirmed with the requester):**

- **SurrealDB is an example only, not a deliverable.** The brief names it to
  illustrate "swappable backend"; v1 does **not** build a SurrealDB adapter.
  The abstraction contract + backend-parametric conformance are how we prove
  swappability.
- **No N-API / TypeScript SDK surface in this brief.** Rust crate SDK only —
  the consumer is a Rust library user. Existing N-API bindings stay as-is.
- Engram does not own host UI/API contracts, agent orchestration, prompt
  policy, context budgeting, tool exposure, or product workflows (brief
  Non-Requirements).
- Engram does not mirror any one host's trait names / structs / table names.

## Appetite

Not given. The work is medium-large but decomposes into ~7 independent slices;
each is one feature-sized spec. The foundation slice (capability-report
completeness + SDK hardening) unblocks the rest.

## Current state (gap analysis, 2026-07-09)

Grounded in source. "Exists" = present as a Rust port/adapter and surfaced (or
surfaceable) through the provider; "gap" = genuinely absent.

| # | Capability | State | Evidence / gap |
|---|---|---|---|
| 1 | Provider facade | ✅ exists | `EngramProvider` + `EngramProviderBuilder`, `core/integration/src/provider.rs` |
| 2 | Backend abstraction | 🟡 contract only | SQLite wired; no second backend. **SurrealDB is example-only — not built.** Contract proven via conformance, not a 2nd impl. |
| 3 | Capability discovery | 🟡 partial | `CapabilityReport` exists but does not yet cover all 18 areas explicitly |
| 4 | Memory fact API | ✅ exists | `engram-memory` + `engram-store-sql` |
| 5 | Knowledge graph API | ✅ exists | `engram-knowledge` + `engram-store-knowledge-sqlite` |
| 6 | Episode / evidence API | 🟡 gap (exposure) | `SourceAssertion`/provenance rich internally; no `episodes()`/`evidence()` provider handle |
| 7 | Ontology API | ✅ exists | `OntologyRepository` |
| 8 | Taxonomy API | ✅ exists | `TaxonomyRepository` |
| 9 | Belief API | ✅ exists | `engram-belief` + `engram-store-belief-sqlite` |
| 10 | Atomic batch | ❌ gap | no cross-store transaction spanning memory+knowledge+graph+evidence |
| 11 | Embedding integration | 🟡 partial | `EmbeddingProvider` trait + config exist; dim-mismatch detection / reindex-on-change / degraded-mode gaps |
| 12 | Unified recall | ❌ gap | RRF composition exists over chunks/entities; no single recall across facts+graph+beliefs+episodes+taxonomy |
| 13 | Maintenance | 🟡 partial | migrate/dry-run present; compact/reindex/dedup partial |
| 14 | Observability | 🟡 partial | capability report + schema version only; no record-counts/index-status/slow-query diagnostics |
| 15 | Stable data model | ✅ exists | domain types: stable ID, external key, scopes, timestamps, confidence, provenance |
| 16 | Error model | 🟡 partial | typed `CoreError` + `CapabilityReason`; needs the full 10-category set (dim mismatch, txn unsupported, …) |
| 17 | Conformance | 🟡 partial | `adapters/integration` harness + fixtures exist; not yet covering all 18 areas / not asserted backend-parametric |
| 18 | Migration / export | 🟡 partial | import records + manifest exist; no export→import round-trip, dry-run report, parity validation, or backend-to-backend helper |

## Proposed slices (the cut — awaiting confirmation)

Each slice is independently shippable + independently testable. Dependency
order: **S1** is the foundation (unblocks the rest); S2–S6 add capability
areas and can follow in parallel; **S7** is the capstone that proves the
abstraction contract.

| Spec slug | Brief areas | Ships |
|---|---|---|
| **S1** `provider-sdk-capability-report` | #1, #3, #16 | `EngramProvider` documented as the canonical Rust SDK; `CapabilityReport` extended to all 18 areas with explicit `Supported`/`Unsupported(reason)` (FailClosed, no silent fallback); typed error categories completed. |
| **S2** `episode-evidence-api` | #6 | `episodes()`/`evidence()` provider handle over existing provenance; record episodes, link evidence, query by source/time/namespace/entity/fact/relationship/session. |
| **S3** `atomic-batch-ingest` | #10 | Cross-store atomic batch (episode + facts + entities + relationships + evidence + embeddings) with idempotency keys, partial-failure reporting, explicit capability flag. |
| **S4** `unified-recall-api` | #12 | One recall across facts+graph+beliefs+episodes+taxonomy; lexical/vector/hybrid; scoping; ranking metadata + trace; degraded mode. |
| **S5** `export-import-migration` | #13, #18 | Export→import round-trip, dry-run migration report, parity validation, backend-to-backend helper (extends existing migration module). |
| **S6** `observability-api` | #14 | Backend type/status, migration status, record counts by type, index status, embedding config, slow-query/retrieval diagnostics, recall explanation. |
| **S7** `backend-conformance-coverage` | #17, #2 (contract) | Conformance harness covers all 18 areas and is backend-parametric (no SQLite symbols in the contract layer; a stub backend proves swappability). SQLite passes; SurrealDB stays example-only. |

## Spec map

<!-- Shape A: one row per derived spec. Status is auto-derived by
scripts/lint-brief-coverage.py from each spec's own Status field — do not
hand-edit. Empty today; rows appear as slices are scaffolded. -->

| Spec | Status |
| --- | --- |
| _none scaffolded yet — awaiting cut confirmation_ | — |
