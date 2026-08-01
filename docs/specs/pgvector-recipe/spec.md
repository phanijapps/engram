# Spec: pgvector backend recipe (backends/pgvector)

- **Status:** Shipped <!-- Draft | Implementing | Shipped | Deferred -->
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** RFC-0017 (Phase B, Accepted), ADR-0022 (engine neutrality, backend = recipe crate, the only place a backend identity exists)
- **Brief:** none
- **Contract:** none — reuses every existing port; domain types unchanged. pgvector remains an additive engine.
- **Shape:** integration

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

Promote the pgvector (Postgres) backend from an engine-specific module inside
the `engram-integration` SDK facade into a **`backends/pgvector` recipe crate**
that owns connection lifecycle, schema, adapter composition, and per-engine
conformance — ADR-0022's "backend = recipe" shape, now that pgvector is the
second engine. The recipe is the canonical host entry for pgvector; the SDK
facade stays engine-neutral (sqlite remains its default). A pgvector config no
longer routes through `EngramProvider::open`; hosts (the N-API binding, the
conformance suite) call `backends::pgvector::open` directly. The pgvector cells
themselves are unchanged (already working against a live Postgres).

## Boundaries

The three-tier guard that keeps an implementing agent inside the lines.

### Always do

- Keep the recipe dependent on `engram-integration` (ports + `EngramProvider`)
  and `engram-store-pgvector` (cells) — never the reverse (the dependency cycle
  is what forces the recipe to be the host entry, not `open`).
- Leave `EngramConfig::pgvector_connection_string` as a config string in the
  neutral facade (ADR-0022: an engine name may appear in those layers only as a
  config string).
- Run the engine-neutrality gate after the move — `engram-integration` must no
  longer name `pgvector`/`Pg*`/`postgres` types.
- Keep the pgvector cells (`adapters/pgvector`) untouched; only the composition
  moves.

### Ask first

- Moving SQLite out of `EngramProvider::open` into `backends/sqlite` (full
  neutrality) — out of scope here; deferred to `docs/backlog.md` →
  `backends-sqlite-extraction` (`open` stays the sqlite default per RFC-0017).
- Changing the `EngramConfig` serialized shape or any port trait.

### Never do

- Re-introduce a `core/integration → backends/pgvector` dependency (cycle); the
  recipe is composed by hosts, never by the facade.
- Drop the `pgvector_connection_string` config field, or make `open` named-engine
  for sqlite beyond its current default.
- Edit the pgvector adapter cells' behavior (composition only), or weaken the
  engine-neutrality / surface-parity gates.

## Testing Strategy

- **Manual / integration QA** — the recipe's `open` composes cells into a
  provider; a Rust test (gated `#[ignore]`, run with `--ignored` against the live
  Postgres container) asserts specific capabilities `Supported` + a memory
  write/read. This is not TDD — it needs the live DB, so it does not run on CI by
  default; AC4 requires the `--ignored` run with its result recorded.
- **Goal-based check** — `cargo check --workspace` (with the new crate wired);
  `.codex/hooks/check-engine-neutrality.sh` passes (integration free of
  pgvector); `.codex/hooks/check-surface-parity.sh` passes; `cargo fmt`/`clippy`.
- **Visual / manual QA** — run the existing `pg_round_trip` +
  `pgvector_bootstrap` tests (now via the recipe) against the live
  `engram-pgvector` container, recording observed capability reports.

## Acceptance Criteria

- [x] A `backends/pgvector` crate exists and exposes
  `pub fn open(config: &EngramConfig) -> CoreResult<EngramProvider>` owning
  connection lifecycle + schema + cell composition (moved out of
  `core/integration`).
- [x] `engram-integration` no longer names pgvector: the `postgres/` module is
  gone, the `pgvector` cargo feature + `engram-store-pgvector` dep are removed,
  and `EngramProvider::open` no longer routes pgvector configs. Verified by direct
  `grep -rn "pgvector\|postgres\|Pg[A-Z]" core/integration/src` — only the
  permitted `pgvector_connection_string` config field remains (the neutrality
  lint's regex misses lowercase `crate::postgres`, so the grep is authoritative).
- [x] `EngramProvider::open` rejects a config carrying
  `pgvector_connection_string` with a clear error pointing to
  `engram_backend_pgvector::open` — it does NOT silently fall through to sqlite.
- [x] The N-API binding opens a pgvector provider through the recipe when the
  config carries a `pgvector_connection_string` (and sqlite via `open`
  otherwise).
- [x] The `pgvector_bootstrap` test passes against the live Postgres via the
  recipe (`pg_round_trip` is the unchanged cell-level regression net — it calls
  the cell directly, not the provider, so it is not repointed).
- [x] `check-engine-neutrality.sh`, `check-surface-parity.sh`, `cargo check
  --workspace`, `clippy`, and `fmt` are all green.

## Assumptions

- Technical: the pgvector cells are functional — `pg_round_trip` and
  `pgvector_bootstrap` pass against the live `engram-pgvector` container
  (`postgres://engram:engram@localhost:5432/engram`, verified 2026-07-31).
  (source: cargo test run this session)
- Technical: `bootstrap_pgvector` + `PgUnifiedRecall` live in
  `core/integration/src/postgres/bootstrap.rs` and compose
  `PgMemoryService`/`PgKnowledgeStore`/`PgBeliefStore`/`PgHierarchyStore`/
  `PgProcedureStore`/`PgVectorIndex` via `EngramProviderBuilder`. (source:
  core/integration/src/postgres/bootstrap.rs)
- Technical: `engram-integration` cannot depend on `backends/pgvector` without a
  cycle (the recipe depends on integration for `EngramProvider` + ports) — this
  is what forces the recipe, not `open`, to be the pgvector host entry. (source:
  ADR-0022, dependency analysis)
- Technical: `EngramProvider::open` currently routes pgvector via
  `#[cfg(feature = "pgvector")]`; removing it leaves `open` as the sqlite
  default, which the RFC permits. (source: core/integration/src/provider.rs,
  RFC-0017)
- Process: full-mode work-loop (structural, multi-crate, host-contract change);
  RFC-0017 Phase B + ADR-0022 govern. (source: docs/rfcs/0017, AGENTS.md)
