# ADR-0022: Engine grid vs backend recipe — SQLite-only now, multi-engine ready

- **Status:** Accepted
- **Promoted 2026-07-16:** the SurrealDB backend (`docs/specs/surrealdb-backend`)
  is the named "second engine" trigger event; T2 of that spec executes the
  structural extraction this ADR defers (`backends/sqlite`). Decision-maker:
  phanijapps (endorsement: "consumers change backend and it works, no issues").
- **Amendment 2026-07-16 (recipe = submodule, not crate):** implementing the
  extraction found that a separate `backends/<name>` *crate* creates a Cargo
  dependency cycle. Every backend's `bootstrap_*` returns an `EngramProvider`
  — a type owned by `core/integration` — so the backend crate must depend on
  the facade, while the facade's `EngramProvider::open` must call the backend.
  Cargo forbids the cycle (feature-gating doesn't help — it is live whenever
  the feature is on). The "recipe" is therefore realized as a **feature-gated
  engine submodule** of `core/integration` (`src/sqlite/` already; `src/surreal/`
  added), each an exempt engine zone: ADR-0022 rule-1 scans the neutral facade
  files (`provider.rs`, `capability.rs`, …), not these submodules, so the facade
  stays engine-neutral and swap-by-config is unchanged. Only the physical crate
  boundary differs from the original "recipe crate" wording.
- **Amendment 2026-07-16 (one crate per backend):** a backend's database
  operations consolidate into ONE crate `engram-store-<engine>` — e.g.
  `engram-store-surreal`, the future `engram-store-sqlite`, and composite
  backends like `engram-store-mixed` (e.g. lancedb + neo4j). Each holds every
  capability cell behind a shared connection. This supersedes the per-capability
  adapter-crate grid (`adapters/<capability>/<engine>`) for consolidated engines:
  a consolidated backend lives at `adapters/<engine>/`. The grid stays valid for
  engines that genuinely ship as independent per-capability cells. Only the thin
  `bootstrap_*` wiring (which returns the facade-owned `EngramProvider`) stays
  in `engram-integration`; the cells live in the `engram-store-<engine>` crate.
- **Date:** 2026-07-09
- **Decision-makers:** phanijapps
- **Supersedes:** none
- **Related:** ADR-0001 (workspace boundaries), ADR-0005 (storage adapter semantics), ADR-0006 (first SQL adapter: SQLite), RFC-0012 (codegraph layer), `docs/product/briefs/engram-host-sdk.md`

## Decision summary

- **Decision:** Separate *engine* (a per-capability adapter cell on a capability × engine grid) from *backend* (a recipe crate that composes adapters and owns connection lifecycle + conformance); ship SQLite only, with seams that make future engines additive.
- **Because:** a host must swap storage by config/crate, not by rewrite — and that is only true if no neutral layer ever names an engine type.
- **Applies to:** `core/*`, `core/integration` (SDK facade), `bindings/node`, `adapters/*`, and a future `backends/` layer.
- **Tradeoff accepted:** two concepts (engine vs backend) to keep distinct in review, plus a structural reorg owed when the second engine actually arrives (deferred — YAGNI).
- **Revisit if:** a second storage engine is adopted — that triggers extracting `backends/sqlite`, normalizing the adapter grid, and promoting layout conventions into enforced structure.

## Context

Today SQLite is the only implemented backend, but it is wired across several
adapter crates (`memory/sqlite`, `knowledge/sqlite`, `retrieval/sqlite-vec`,
`retrieval/tantivy-lexical`, belief/hierarchy SQLite) with the wiring mixed into
`adapters/integration` alongside the conformance harness. This works, but it
leaves the line between "an adapter implementation" and "a backend" implicit.

A host-SDK consumer (the `engram-host-sdk` brief) needs backend-neutral Rust
contracts so that moving SQLite → a future engine is an engram-internal
config/crate change, never an application rewrite. The brief names SurrealDB,
pglite, pgvector, and lance as examples of engines that should slot in later.

Constraints in force right now:

- Do not destabilize the working SQLite setup.
- Do not build engines that do not exist yet (YAGNI) — SurrealDB/pg/lance are
  examples, not v1 deliverables.
- But the *seams* must be real and enforceable, so "flexibility" is a gate, not
  folklore — even with only SQLite built.

## Decision

We adopt a layered separation with **one-way dependencies** and two distinct
concepts:

> An **engine** is a per-capability implementation on a `capability × engine`
> grid (`adapters/<capability>/<engine>`), each behind a core port. A **backend**
> is a recipe crate (`backends/<name>`) that composes a set of engines for one
> deployment and owns that deployment's connection lifecycle, config validation,
> adapter composition, and per-engine conformance.

SQLite is the only implemented backend today. No adapter is renamed and no
`backends/` crate is created until a second engine is actually adopted.

Four rules govern the separation:

1. **One-way dependency direction.** `core/*`, `core/integration` (the SDK
   facade), and `bindings/node` must never name an engine type (`Sql*`,
   `pgvector`, …) or hold SQL. The only place `sqlite` appears in those layers
   is as a config string. This is what makes swap-by-config literally true.
2. **Engines live on the grid.** Each engine is one cell at
   `adapters/<capability>/<engine>`, behind a core port. A future `pg`/`lance`
   cell drops into a recipe's slot; nothing upstream changes.
3. **A backend is a recipe crate.** `backends/<name>` owns connection lifecycle
   + config validation + adapter composition + conformance, and *only* that.
   Today that mapping lives in `adapters/integration`; it moves to
   `backends/sqlite` when a second engine arrives.
4. **Conformance is backend-parametric.** The contract layer
   (`adapters/integration`) runs the same suite for any backend and contains
   zero engine symbols. Passing it is what makes a backend selectable.

Rules 1 and 4 are **enforceable today** via a neutrality lint (a test that fails
if an engine symbol leaks into a neutral layer) — so the flexibility is a gate
even with only SQLite built. Rules 2 and 3 are **naming/layout conventions** the
SQLite reorg will satisfy when we touch it.

## Decision drivers

- **Swap-by-config without rewrite** — the host-SDK brief's bar; the winning driver.
- **Don't destabilize working SQLite / YAGNI** on engines that don't exist yet.
- **Enforceability** — flexibility must be a gate, not a convention that erodes.

## Consequences

**Positive:**

- Adding an engine = new adapter cells + one recipe crate; neutral layers
  (`core`, SDK facade, bindings) are untouched.
- Engines are pluggable orthogonally: `lance`/`pgvector` are vector cells usable
  by any recipe; `tantivy` is engine-agnostic; `pglite` and full Postgres share
  `pg` adapter cells, differing only in connection driver.
- The flexibility guarantee is *enforceable today* (neutrality lint) with only
  SQLite built — no waiting for a second engine to prove the contract.

**Negative:**

- Two concepts (engine vs backend) to keep distinct during review.
- A structural reorg is owed when the second engine arrives (extract
  `backends/sqlite`, normalize adapter crate names to the grid) — deliberately
  deferred.
- Until that reorg, today's SQLite wiring still lives in `adapters/integration`
  mixed with conformance, so rules 2 and 3 are convention, not yet structure.

**Revisit if:** a second storage engine (pg/pglite, pgvector, lance, SurrealDB,
etc.) is adopted — that triggers extracting `backends/sqlite`, normalizing the
adapter grid, and promoting conventions 2 and 3 into enforced structure.

## Confirmation

- **Mode:** lint/CI (rules 1 and 4) + reviewer-checked (rules 2 and 3 until structuralized)
- **Signal:** a neutrality test/lint that fails if an engine symbol (`Sql*`,
  `sqlite`, `pgvector`, raw SQL) appears in `core/`, `core/integration`, or
  `bindings/node`, and that the conformance contract layer
  (`adapters/integration`) is engine-symbol-free. Adapter-grid naming and the
  `backends/` layout are checked at PR review.
- **Owner:** engram core team / architecture reviewer.

## Alternatives considered

- **One monolithic backend per engine** (a crate that re-implements every
  capability for SQLite, another for PG, …). Rejected: it duplicates capability
  logic per engine, prevents plugging a single store (e.g. Lance vectors) into
  different bodies, and bloats as `engines × capabilities` grows. Loses to the
  grid on the pluggability driver.
- **Keep the status quo** — SQLite wired directly in `adapters/integration`, no
  separation. Rejected: a second engine would force an application rewrite or a
  fork of the wiring, failing the swap-by-config driver. Tolerated only
  temporarily because of YAGNI on the second engine; the neutrality lint holds
  the line meanwhile.
- **Build all seams now** — rename every adapter to the grid and extract
  `backends/sqlite` immediately, regardless of a second engine. Rejected as
  premature: it destabilizes working SQLite for an engine that does not exist
  yet. The ADR + neutrality lint capture the contract without the churn.

## References

- `docs/product/briefs/engram-host-sdk.md` — the host-application brief this
  decision substrates (slices S1 provider/capability and S7 backend-parametric
  conformance).
- `AGENTS.md` boundary rules (updated by this ADR to add the `backends/` row).
- ADR-0001, ADR-0005, ADR-0006 — workspace boundaries and storage-adapter
  semantics this builds on.
