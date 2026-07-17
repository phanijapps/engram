# Spec: SQLite consolidation — one `engram-store-sqlite` crate

- **Status:** Draft
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0022 (one-crate-per-backend amendment, 2026-07-16)
- **Brief:** none
- **Contract:** none — relocates existing `Sql*` port impls; no contract change
- **Shape:** integration <!-- crate/dependency restructuring; no new behavior -->

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

Every SQLite database operation in engram lives in ONE crate,
`engram-store-sqlite`, instead of today's five scattered adapter crates. A
consumer imports SQLite storage from a single place; the engine-agnostic
adapters (Tantivy lexical, associative-graph, community-summary, decay, ingest)
stay separate because they are shared with the Surreal backend (and any future
backend). Behavior is unchanged — this is pure relocation. The thin
`bootstrap_sqlite` wiring (which returns the facade-owned `EngramProvider`)
stays in `engram-integration` to avoid the Cargo cycle, exactly as the Surreal
backend already does.

## Scope — what merges vs. what stays

| Crate (today) | LOC | Disposition |
|---|---|---|
| `engram-store-sql` (memory) | 1,979 | **merge → `engram-store-sqlite::memory`** |
| `engram-store-knowledge-sqlite` | 1,980 | **merge → `engram-store-sqlite::knowledge`** |
| `engram-store-belief-sqlite` | 759 | **merge → `engram-store-sqlite::belief`** |
| `engram-store-hierarchy-sqlite` | 402 | **merge → `engram-store-sqlite::hierarchy`** |
| `engram-store-vector` (sqlite-vec) | 930 | **merge → `engram-store-sqlite::vector`** |
| `core/integration/src/sqlite/{consolidation_adapters,recall_lanes}.rs` | 316 | **move → `engram-store-sqlite`** |
| `engram-store-lexical` (Tantivy) | 626 | **stays** — engine-agnostic, shared w/ Surreal |
| `engram-store-associative-graph` | 820 | **stays** — engine-agnostic |
| `engram-store-community-summary` | 423 | **stays** — engine-agnostic |
| `engram-decay` | 301 | **stays** — engine-agnostic |
| `engram-ingest` | 4,967 | **stays** — source reader, not a store |

Consumer surface to rewire: `engram-conformance` (re-exports `Sql*`), `bindings/node` (~17 legacy modules import `Sql*` directly), `core/integration/src/sqlite/*`, and assorted tests.

## Boundaries

### Always do
- Preserve the `Sql*` public paths during transition — start with a `engram-store-sqlite` re-export facade so consumers switch import source before any code moves.
- Keep the workspace green + the neutrality hook green at every phase; each phase is a behavior-preserving relocation.
- Leave the engine-agnostic adapters (lexical/associative/community/decay/ingest) untouched — they are shared with Surreal.
- Keep `bootstrap_sqlite` thin in `engram-integration` (returns `EngramProvider` → cycle constraint).

### Ask first
- Deleting the old adapter-crate directories after their code has moved + consumers updated.
- Any rename of public `Sql*` types (none planned; re-export preserves names).
- Sequencing relative to the `surrealdb-backend` effort (the two share `bindings/node` + conformance consumers).

### Never do
- Never change `Sql*` runtime behavior — this is relocation only; the existing tests are the regression net. [structural]
- Never fold the engine-agnostic adapters into `engram-store-sqlite` — they serve every backend, not just SQLite. [structural]
- Never break the `engram-conformance::Sql*` / `bindings/node` import surface mid-transition (the facade phase prevents this). [structural]

## Testing Strategy
- **Goal-based (regression):** the existing workspace test suite (every `Sql*` crate's unit tests + `engram-conformance` integration tests) stays green at every phase — it is the behavior-preservation gate.
- **Goal-based (neutrality):** `check-engine-neutrality.sh` stays green.
- **Goal-based (import surface):** `grep` confirms no consumer imports the deleted crate paths after each fold.

## Acceptance Criteria
- [ ] `engram-store-sqlite` is the single crate holding all SQLite storage: memory, knowledge (graph/taxonomy/ontology), belief, hierarchy, vectors, and the `Sql*` consolidation/recall-lane glue.
- [ ] The five source adapter crates are removed; no workspace member depends on them.
- [ ] `core/integration/src/sqlite/` contains only the thin `bootstrap_sqlite` wiring.
- [ ] Every consumer (`engram-conformance`, `bindings/node`, `core/integration`, tests) imports SQLite storage from `engram-store-sqlite`.
- [ ] The full workspace test suite + neutrality hook are green; SQLite behavior is unchanged (existing tests pass unmodified).
- [ ] The engine-agnostic adapters (lexical/associative/community/decay/ingest) are untouched.

## Assumptions
- Technical: the five SQLite crates are independent (each implements its own port over its own rusqlite connection); the only cross-crate `Sql*` glue is `consolidation_adapters.rs` + `recall_lanes.rs`. (source: repo grep; to confirm in T0.)
- Technical: each crate's public `Sql*` types can be re-exported from `engram-store-sqlite` without rename, preserving the `engram_conformance::Sql*` + `bindings/node` import paths during transition. (source: re-export facade pattern.)
- Process: ADR-0022's one-crate-per-backend amendment sanctions this consolidation. (source: ADR-0022.)
- Product: the user wants SQLite consolidated to match the `engram-store-<engine>` convention, manageable as phased PRs. (source: user 2026-07-16.)
