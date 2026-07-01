# Plan: belief-contradiction-bitemporal (RFC 0004 Slice 5 / PHASE62)

Durable belief/contradiction in a NEW orchestration SQLite adapter + focused
`NativeBeliefEngine` + routes + BeliefPanel. Two commits: **A** capability
(adapter + binding + routes), **B** UI.

## Tasks

### T1 — `engram-store-belief-sqlite` adapter (+ tests) [Commit A]
- **Tests (TDD):** `tests/repository.rs` — put/get/resolve round-trips with scope filtering; `detect_contradictions` flags same-subject differing-content + ignores uniform groups.
- **Depends on:** none
- **Approach:** New crate at `adapters/orchestration/belief-sqlite/` (add to workspace members). `Cargo.toml` deps: async-trait, chrono, engram-core (path ../../../core/orchestration), engram-domain, engram-runtime, rusqlite, serde_json. `src/{lib.rs, schema.rs, scope.rs, service.rs}`. Schema: `beliefs` + `contradictions` tables (id + scope cols + record_json), idempotent. `SqlBeliefStore` impls `BeliefRepository` (put_belief/put_contradiction/get_contradiction/resolve_contradiction — mirror inmem belief.rs + knowledge sqlite upsert/scope) + `ContradictionDetector` (detect_contradictions: group active beliefs by subject.key; a group with >1 distinct content → one Logical Contradiction, severity = max confidence, status Open, targets = the conflicting beliefs).

### T2 — `NativeBeliefEngine` binding + TS transport [Commit A]
- **Tests:** goal-based — cargo check; rebuild native; backend typecheck.
- **Depends on:** T1
- **Approach:** `bindings/node/src/lib.rs`: `NativeBeliefEngine { store: SqlBeliefStore }` + `#[napi]` putBeliefJson/putContradictionJson/getContradictionJson/resolveContradictionJson/detectContradictionsJson (mirror taxonomy/knowledge `*Json` pattern; get/resolve/detect take `{id?, scope, resolution?, beliefs?}`). `packages/node/src/binding.ts` + `transport.ts`: `NativeBeliefEngineBinding` + `NativeBeliefTransport` (createNativeBeliefTransport) + unknown-typed methods. Rebuild via `pnpm --filter @engram/node build:native && build`.

### T3 — Backend `/belief/*` routes + transport wiring [Commit A]
- **Tests:** goal-based — typecheck; curl round-trip.
- **Depends on:** T2
- **Approach:** `demo/backend/src/engram.ts`: `getBeliefTransport()` singleton (shares ENGRAM_DB). `app.ts`: `/belief/put`, `/belief/contradiction`, `/belief/get`, `/belief/resolve`, `/belief/detect` (thin pass-throughs). To list beliefs for the UI, add `/belief/list` (scan the beliefs table by scope — needs a list method on the store; implement `list_beliefs(scope)` on SqlBeliefStore, not on the port — a store-specific method exposed via a dedicated binding method + route).

### T4 — `BeliefPanel` [Commit B]
- **Tests:** goal-based — frontend typecheck/build; manual.
- **Depends on:** T3
- **Approach:** `demo/frontend/src/BeliefPanel.tsx`: list beliefs (subject.key, content, status badge, confidence bar, valid_from→valid_until); add-belief form (subject key + content + optional valid_from/valid_until + confidence); "Detect contradictions" → `/belief/detect`; contradictions review queue with a "Resolve" action (target_won/manual_ignore). Wire into App.tsx. Display valid_time; note "display-only" near the dates.

### T5 — Validate + lighter adversarial pass
- **Tests:** `cargo fmt --all && cargo check --workspace && cargo test -p engram-store-belief-sqlite`; rebuild native; backend + frontend typecheck/build; curl smoke; single-pass review focused on scope isolation, detector correctness, + boundary (no god-struct, no fold, no transaction_time).
- **Depends on:** T4

## Out of scope (logged)
- Full memory-assertion → belief consolidation driver (DryRun ConsolidationService exists for planning); enforced belief policy; temporal/as-of queries; hierarchy (deferred program-wide).
