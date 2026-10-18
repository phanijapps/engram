# ADR-0030: Retire the SurrealDB experiment cell

- **Status:** Accepted
- **Date:** 2026-10-18
- **Related:** ADR-0022 (engine grid vs backend recipe), ADR-0029 (semantic
  area layout), ADR-0006 (first SQL adapter: SQLite)

## Context

`adapters/surreal` (`engram-store-surreal`, embedded SurrealKV) entered as an
experimental sibling engine behind the non-default `surreal` cargo feature of
`engram-integration`. Reviewed on 2026-10-18 against the shipped system:

- **No backend recipe ever adopted it.** ADR-0022 made a backend a *recipe*
  crate composing adapter cells; `backends/` ships `sqlite` and `pgvector`
  only. Surreal was never promoted, so nothing a host can actually launch
  uses it.
- **Absent from every guarantee surface:** not in CI (workflows never enabled
  the feature), no conformance harness run, not referenced by `bindings/node`,
  `packages/*`, `mcp/engram-mcp`, or `examples/`.
- **It carried the workspace's advisory baggage.** The surrealdb dependency
  tree was the sole source of the only documented cargo-audit ignore
  (RUSTSEC-2026-0235, rkyv 0.7 — target-conditional) plus the bulk of the
  unmaintained/unsound advisory *warnings* (serde_yml/libyml, bincode 1.x,
  event-listener, …).

## Decision

Retire the SurrealDB experiment. Removed in this change:

- `adapters/surreal` (workspace member) and `core/integration/src/surreal/`
  (the `surreal` feature, its `tokio` optional dep, the `BackendProfile::Surreal`
  config variant, and the dispatch arm in `EngramProvider::open`)
- The rkyv ignore in `.cargo/audit.toml` — the advisory left the lockfile with
  the crate; the ignore list is empty again

SurrealDB remains a legitimate future engine: per ADR-0022 it returns as a new
adapter cell + recipe crate (additive, never a rewrite), and this ADR is the
record to supersede if that happens.

## Consequences

- The backend story is unambiguous: SQLite (default) + pgvector (Postgres).
- `cargo audit` warnings drop and no advisory needs an ignore.
- The neutrality deny-list keeps `surreal|surrealdb` patterns — still correct
  as a guard against engine leakage should the cell return.
