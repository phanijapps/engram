# Plan: pgvector backend recipe (backends/pgvector)

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done <!-- Drafting | Executing | Done -->

> **Plan contract:** the implementation strategy; changes noted in the changelog.

## Approach

Move the pgvector composition out of the `engram-integration` SDK facade into a
new `backends/pgvector` recipe crate that is the canonical host entry. Five
moves, ordered to avoid a window where pgvector is unreachable:

1. **(T1)** Create `backends/pgvector` with `open(config)` — copy the composition
   logic (cells → `EngramProviderBuilder`) + `PgUnifiedRecall` from
   `core/integration/src/postgres/bootstrap.rs`, retargeting `crate::` imports to
   `engram_integration::`.
2. **(T2)** Remove pgvector from `core/integration`: delete `postgres/`, drop the
   `pgvector` feature + `engram-store-pgvector` dep, remove the `#[cfg]` route in
   `EngramProvider::open`. `open` becomes the sqlite default.
3. **(T3)** Wire the N-API binding to the recipe: `NativeProvider::new` routes a
   `pgvector_connection_string` config to `backends_pgvector::open` (behind a new
   binding `pgvector` feature), else `EngramProvider::open`; clear error when the
   feature is off.
4. **(T4)** Repoint the pgvector tests (`pgvector_bootstrap`, `pg_round_trip`) at
   the recipe; add a recipe-level conformance test against the live Postgres.
5. **(T5)** Gates (engine-neutrality, surface-parity, workspace, clippy, fmt) +
   a recipe README + the existing docker-compose reference.

Riskiest part: T2 changes `EngramProvider::open`'s contract (no more pgvector
auto-routing) — mitigated by T3 wiring the binding + T4 repointing tests in the
same PR, so no host is left stranded.

## Constraints

- **ADR-0022** — the recipe is the only place a backend identity exists; the
  facade stays neutral. No `integration → backends` dependency (cycle).
- **RFC-0017 Phase B** — sqlite stays in `open` as the default; full
  `backends/sqlite` extraction is explicitly deferred.
- **RFC-0005** — retrieval composition stays backend-agnostic.

## Construction tests

**Integration tests:**
- A recipe-level test opens a provider via `backends_pgvector::open` against the
  live Postgres and asserts the capability report + a memory write/read round-trip.
- The existing `pg_round_trip` + `pgvector_bootstrap` tests, repointed at the
  recipe, still pass (regression net for the move).

**Manual verification:**
- `docker compose -f docs/how-to-pg/docker-compose.yaml` is up; run the ignored
  pgvector tests with `--ignored` and record the capability report.

## Design (LLD)

### Design decisions

- **Recipe is the host entry, not `open`** — forced by the dependency cycle
  (`integration` cannot depend on `backends`). Traces to: AC1, AC2.
- **sqlite stays in `open`** — RFC-0017 scopes Phase B to pgvector; full
  neutrality (`backends/sqlite` + neutral `open`) is a documented deferral.
  Traces to: AC2.
- **`pgvector_connection_string` stays in `EngramConfig`** — a config string is
  the one engine-name allowance in the neutral facade (ADR-0022). Traces to: AC2.
- **Rejected: a thin sham recipe** (keep composition in integration, recipe just
  re-exports) — leaves ADR-0022 composition-ownership unrealized; the user chose
  the full recipe.

### Component / module decomposition

- `backends/pgvector/` (NEW, `engram-backend-pgvector`) — `src/lib.rs` with
  `open(config)`, `src/recall.rs` with `PgUnifiedRecall` (moved), `Cargo.toml`
  deps on `engram-integration` + `engram-store-pgvector` + `engram-runtime`.
- `core/integration/src/postgres/` — **deleted**; `lib.rs` `pgvector` mod + the
  `pgvector` cargo feature + `engram-store-pgvector` dep removed; `open`'s
  `#[cfg(feature="pgvector")]` block removed.
- `bindings/node/src/provider.rs` — `NativeProvider::new` gains pgvector routing
  to the recipe; `engram-node/Cargo.toml` gains a `pgvector` feature pulling
  `backends/pgvector`.

### Failure, edge cases & resilience

- A pgvector config opened through a binding built without the `pgvector` feature
  yields a clear `pgvector feature not enabled` error, not a silent sqlite open.
- Connection / schema failures surface as `CoreError::Adapter` from the cells
  (unchanged).

### Dependencies & integration

- `backends/pgvector` → `engram-integration` + `engram-store-pgvector` (no cycle:
  neither depends back).
- `engram-node` → `backends/pgvector` (optional, behind its `pgvector` feature).
- No new external crate; tokio-postgres + the `OnceLock<Runtime>` pattern stay in
  the cells.

## Tasks

### T1: Create backends/pgvector recipe crate with open(config)

**Depends on:** none

**Tests:**
- `cargo test -p engram-backend-pgvector` (a `#[ignore]` integration test) opens a
  provider via `open(&config)` against the live Postgres and asserts `memory`,
  `knowledge`, `graph`, and `vectors` report `Supported` in the capability report
  + a memory write/read round-trips — mirroring `pgvector_bootstrap`'s assertions,
  not a weak "non-empty".

**Approach:**
- Create `backends/pgvector/Cargo.toml` + `src/lib.rs` (+ `src/recall.rs`).
- Move `bootstrap_pgvector` body → `pub fn open(config: &EngramConfig) ->
  CoreResult<EngramProvider>`; move `PgUnifiedRecall`. Retarget imports from
  `crate::` to `engram_integration::` / `engram_store_pgvector::`.
- Add the crate to the workspace `Cargo.toml` members.

**Done when:** `cargo check -p engram-backend-pgvector` passes; the recipe test
opens a real provider.

### T2: Remove pgvector from core/integration

**Depends on:** T1

**Tests:**
- `cargo check -p engram-integration` (no pgvector feature) passes.
- `.codex/hooks/check-engine-neutrality.sh` passes — no `pgvector`/`Pg*`/`postgres`
  references remain in `core/integration`.

**Approach:**
- Delete `core/integration/src/postgres/`; remove the `pub mod postgres;` +
  `#[cfg(feature="pgvector")]` lines in `lib.rs`.
- Remove the `pgvector` feature + `engram-store-pgvector` optional dep from
  `core/integration/Cargo.toml`.
- Remove the `#[cfg(feature = "pgvector")] ... bootstrap_pgvector` block in
  `EngramProvider::open`. Keep `EngramConfig::pgvector_connection_string`.
- Add a guard at the top of `open`: if `config.pgvector_connection_string.is_some()`,
  return a clear `InvalidRequest`/`CapabilityUnsupported` error directing the caller
  to `backends_pgvector::open` — no silent sqlite fallthrough (covers the AC).

**Done when:** integration builds without pgvector; neutrality lint clean;
`open` is sqlite-default.

### T3: Wire the N-API binding to the recipe

**Depends on:** T1, T2

**Tests:**
- `cargo check -p engram-node` passes (default + `pgvector` feature).
- The T4 binding-level pgvector path is reachable.

**Approach:**
- `engram-node/Cargo.toml`: add `backends/pgvector` behind a new `pgvector`
  feature.
- `NativeProvider::new`: if `config.pgvector_connection_string.is_some()` →
  `#[cfg(feature="pgvector")] backends_pgvector::open(&config)`; else a clear
  error when the feature is off; otherwise `EngramProvider::open(&config)`
  (sqlite).
- `packages/node/scripts/build-native.mjs` is unchanged (it hardcodes
  `--features fastembed`); a pgvector-enabled binding is built explicitly via
  `cargo build -p engram-node --features fastembed,pgvector`. Default binding
  builds stay sqlite + fastembed — pgvector is opt-in.

**Done when:** binding opens pgvector via the recipe when the feature is on;
clear error when off; sqlite path unchanged.

### T4: Repoint pgvector tests + recipe conformance

**Depends on:** T3

**Tests:**
- `pgvector_bootstrap` + `pg_round_trip` pass against the live Postgres via the
  recipe (`--ignored`).
- A recipe-level conformance test runs the harness fixtures through
  `backends_pgvector::open`.

**Approach:**
- Move `core/integration/tests/pgvector_bootstrap.rs` →
  `backends/pgvector/tests/bootstrap.rs`, repointed to call
  `backends_pgvector::open` (this is the only stranded host).
- Leave `adapters/pgvector/tests/pg_round_trip.rs` untouched — it calls
  `engram_store_pgvector` cell methods directly (not the provider), so it is the
  unchanged cell-level regression net, not a recipe test.
- Add `backends/pgvector/tests/conformance.rs` running the conformance fixtures
  through `backends_pgvector::open` (gated/ignored, live Postgres).

**Done when:** all pgvector tests green via the recipe.

### T5: Gates + docs

**Depends on:** T4

**Tests:**
- `cargo fmt --all`, `cargo clippy --workspace`, `cargo check --workspace`,
  `.codex/hooks/check-engine-neutrality.sh`, `.codex/hooks/check-surface-parity.sh`
  all green.
- Authoritative neutrality check: `grep -rn "pgvector\|postgres\|Pg[A-Z]" core/integration/src`
  returns only `pgvector_connection_string` (the lint's regex misses lowercase
  `crate::postgres`, so this grep is the real gate).

**Approach:**
- Run the full gate suite; fix any fallout from the move.
- Add `backends/pgvector/README.md` (recipe purpose, the docker-compose
  reference, the host-entry contract).

**Done when:** all gates green; `git status` clean.

## Rollout

- **Delivery:** additive crate + a host-contract change (pgvector configs no
  longer auto-route through `open`). Reversible by re-adding the integration
  module. No data migration (schema applied by the cells, unchanged).
- **Deployment sequencing:** T1 (recipe) before T2 (remove from integration) so
  pgvector is never unreachable mid-PR; T3/T4 rewire hosts in the same PR.

## Risks

- **Host-contract change** — the only stranded host is
  `core/integration/tests/pgvector_bootstrap.rs` (repointed in T4). The N-API
  binding is sqlite-only today (`NativeProvider::new` has no pgvector branch), so
  T3 *adds* a pgvector path rather than re-wiring one; MCP / engram-conformance /
  examples / packages have zero pgvector refs (audited).
- **Feature-flag plumbing** — the binding's `pgvector` feature must thread
  through `build-native.mjs` only when desired (default builds stay sqlite +
  fastembed); documented, not forced.
- **Neutrality lint fallout** — removing pgvector from integration should make
  the lint cleaner, not fail it; verify in T5.

## Changelog

- 2026-07-31: initial plan (full mode, RFC-0017 Phase B; user chose the full
  ADR-0022-compliant recipe over the lean wrapper).
