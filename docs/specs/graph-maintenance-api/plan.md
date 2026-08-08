# Plan: Graph maintenance API

- **Spec:** [`spec.md`](spec.md)
- **Status:** Drafting

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog
> at the bottom.

## Approach

Build the capability as a vertical slice down the stack, domain-first: add the
archive soft-state and the maintenance plan/mutation/result domain types, then a
new `GraphMaintenanceRepository` port in `core/knowledge` (alongside
`EntityIdentityRepository`, mirroring consolidation's `Plan`/`Gate`/`Run` shapes
as a template only), then deterministic candidate detectors, then the SQLite
adapter in stages (archive/restore + active reads; transactional apply; ops;
health), then wire the capability through the four surfaces in order —
`EngramProvider` facade + `CapabilityReport`, N-API binding + `@engram/node`, and
finally the engram-mcp tools. The riskiest part is the transactional apply: there
is no public transaction port today, so the adapter stages a plan's mutations
inside one `conn.transaction()` and rolls back on a failed referential-integrity
verify — reusing the precedent already inside `delete_graph` and
`consolidate_entities`. Atomicity is declared as backend-dependent intent at the
port and realized as a single SQLite transaction in the adapter; the guarantee
level is surfaced in the apply result (ADR-0022), not in `CapabilityReport`.
Reversibility is archive/restore only — no audit table, no export snapshot
(ADR-0027).

## Constraints

- **ADR-0027** — archive+restore is the reversibility primitive; no durable audit
  table, no export-snapshot rollback; atomicity is backend-dependent; candidates
  are deterministic.
- **ADR-0022** — engine neutrality: the port and the N-API binding must name no
  engine type and hold no SQL; SQLite lives only in the adapter. Surface parity:
  the capability reaches the Rust facade, the N-API binding, `@engram/node`, and
  engram-mcp, and is reflected in `CapabilityReport` as `graph_maintenance`.
- **ADR-0018** — retraction ports + per-source convergence remain in force over
  the *active* set; archived rows sit outside convergence (retention is an open
  item, see spec *Ask first*).
- **RFC-0014** — the transactional-consolidation precedent (`consolidate_entities`
  inside one transaction with an audit-style result) is the template for the
  apply step and its facade/N-API parity wiring.
- **AGENTS.md** — update `docs/domain-data-model.md` before changing domain types;
  crate roots whitelist `pub use` (new public fns must be re-exported or
  cross-crate calls fail); no god-modules (the port is focused, behavior in
  modules).

## Construction tests

**Integration tests:** one vertical-flow test per backend — `list_candidates →
build_plan → apply_plan → verify` succeeds and persists; a second that forces a
referential-integrity verify failure mid-plan and asserts the graph is
byte-for-byte unchanged (single-transaction rollback); a third that re-applies a
completed plan and asserts zero new mutations (idempotency). A parity check
(goal-based) asserts the capability is reachable from the facade, N-API,
`@engram/node`, and the engram-mcp tool set, and listed in `CapabilityReport` as
`graph_maintenance`.

**Manual verification:** none beyond the gates (`cargo test`, `pnpm typecheck`,
`pnpm test`).

## Design (LLD)

### Design decisions

- **Archive representation: a dedicated `archived_at: Option<Timestamp>` on
  `KnowledgeEntity` and `KnowledgeRelationship`** (None = active). Chosen over
  extending `Policy`/`DeleteMode::Archive` onto these objects — minimal, does not
  overload the bi-temporal `valid_until` (ADR-0019), and `None` keeps existing
  rows active without a migration of values. *Traces to: AC archive/restore,
  active-read exclusion · ADR-0027.*
- **New port `GraphMaintenanceRepository` in `core/knowledge`**, not an extension
  of `KnowledgeRepository` (already broad) or `consolidation` (wrong domain). It
  composes the existing primitives (`consolidate_entities`, `discover_collisions`,
  `resolve_or_put`, `delete_*`, `validate_graph`) behind a focused plan/apply
  surface. *Traces to: AC surface reach · ADR-0027, AGENTS.md no-god-module.*
- **Atomic apply = one `conn.transaction()` in the SQLite adapter**, staged from
  the plan's mutation list, with a referential-integrity verify before commit;
  the port declares `Atomicity::BackendDependent` and the adapter reports
  `Atomicity::SingleTransaction` in the apply result. No public transaction
  handle is added to any core trait. *Traces to: AC atomic apply + rollback,
  idempotency · ADR-0022, RFC-0014.*
- **Idempotency by target-state**: each mutation variant has a per-variant
  "already-applied" key (see Behavior & rules); re-applying is a no-op counted as
  `unchanged`. *Traces to: AC idempotent reapply.*

### Data & schema

- `KnowledgeEntity` / `KnowledgeRelationship` gain `archived_at: Option<Timestamp>`
  (`core/domain/src/knowledge.rs`). SQLite gains a nullable `archived_at` column
  on `knowledge_entities` / `knowledge_relationships` plus an index for the
  active-read filter (`adapters/sqlite/src/knowledge/schema.rs`).
- New domain types in `core/domain/src/maintenance.rs` (or `operations.rs`):
  `MaintenanceMutation` (enum: Archive / Restore / Delete / Merge / AddAlias /
  RemoveAlias / RewriteRelationship, each carrying target ids + payload),
  `MaintenanceMutationPreview` (before/after snapshots), `MaintenancePlan`
  (graph/scope, mutation list, policy that generated it, fingerprint),
  `MaintenanceApplyResult` (applied/unchanged/failed counts per kind, verify
  findings, `Atomicity` guarantee level, plan fingerprint), `MaintenanceCandidate`
  (kind, target id, reason, confidence, source refs, optional `review_status` +
  reviewer actor for human-reviewed candidates), `Atomicity` enum. The plan
  fingerprint is echoed in `MaintenanceApplyResult` so the caller can confirm the
  plan it reviewed is the plan that was applied (stale/different-plan detection);
  it is not the idempotency key (idempotency is per-variant target-state).
- `docs/domain-data-model.md` updated for the archive field + maintenance types
  before code lands. *Traces to: AC archive, preview, apply · ADR-0027, AGENTS.md.*

### Interfaces & contracts

- Port: `GraphMaintenanceRepository` in `core/knowledge/src/maintenance.rs` —
  `list_entities`/`list_relationships` (filters → `Page<T>` via `Cursor`),
  `detect_candidates(policy)`, `build_plan(plan_request) -> MaintenancePlan`
  (dry-run preview), `apply_plan(plan, ApplyMode { Preview, Apply }) ->
  MaintenanceApplyResult`, `graph_health(scope)`. The granular ops
  (archive/restore/delete/merge/rewrite_relationship/manage_alias) are NOT
  separate trait methods — each is a `MaintenanceMutation` variant a caller
  stages in a single-mutation plan via `build_plan`+`apply_plan`, keeping the
  port focused (the MCP layer, T8, provides the granular UX). Authorization to
  mutate is the explicit `Apply` flag only — there is no "approved plan id" path
  (plans are ephemeral; no durable plan store, per ADR-0027). No
  `contracts/<type>/` artifact — the contract is the Rust trait surfaced through
  N-API + MCP.
- Re-exported at the crate root (`core/knowledge/src/lib.rs`) — whitelist `pub use`
  per AGENTS.md. *Traces to: AC list/filter, preview, apply, all ops · RFC-0014.*

### Component / module decomposition

- `core/domain/src/maintenance.rs` — plan/mutation/result/candidate types (new).
- `core/knowledge/src/maintenance.rs` — the port trait + candidate-policy types
  (new); `core/knowledge/src/lib.rs` re-exports.
- `adapters/sqlite/src/knowledge/maintenance.rs` — the SQLite impl (new): archive
  state, active-filter reads, list/paginate, transactional apply, rewrite/alias,
  a tx-aware merge core shared with `consolidate_entities` (archive-disposition
  for maintenance), health aggregates.
- `core/integration/src/{provider.rs,capability.rs}` — facade field/accessor/
  `require_graph_maintenance()` + a new `graph_maintenance` `CapabilityReport`
  field (distinct from the existing backend-neutral `maintenance`).
- `bindings/node/src/provider.rs` — `NativeGraphMaintenanceApi` + proxy.
- `packages/node/src/{binding.ts,provider.ts,index.ts}` — TS binding + transport.
- `mcp/engram-mcp/src/{main.rs,maintenance.rs}` + `packages/runtime/src/mcp/tools.ts`
  — MCP tool registration (Rust stdio + TS HTTP).

### State & control flow

Plan lifecycle (the spec's safety contract, realized):
`detect_candidates → build_plan (dry-run, Preview) → caller reviews → apply_plan
(Apply)`; inside apply: `begin tx → stage mutations → referential-integrity
verify → commit | rollback`. Archive/restore is a state transition
`Active ⇄ Archived` on the `archived_at` field; archiving a node transitions its
incident edges with it. Active reads (`list_*`, retrieval, convergence) filter
`archived_at IS NULL`.

### Behavior & rules

- Default non-mutating: `ApplyMode::Preview` is the default; only `Apply`
  mutates. There is no "approved plan id" path — plans are ephemeral; an apply is
  authorized by the explicit `Apply` flag alone (consistent with ADR-0027's
  no-durable-audit decision).
- Candidate policy (deterministic): orphan = no incident edges **or** no
  `source_refs`; low-confidence = `confidence < threshold`; unsupported =
  `validate_graph` finding; duplicate = `discover_collisions` group. Threshold and
  toggles come from the caller's policy — no LLM.
- Merge reuses `consolidate_entities`'s rewire/coalesce logic via a tx-aware core
  extracted for the apply transaction, and **archives** absorbed entities AND
  coalesced duplicate relationships (recoverable via restore) rather than
  hard-deleting them; `consolidate_entities` itself keeps its hard-delete behavior.
  `Provenance` is preserved on the survivor.
- **Post-apply integrity verify (enforceable, not advisory):** before commit the
  adapter runs a referential-integrity check — every relationship's subject and
  object resolves to an *active* (non-archived, non-absorbed) entity, and no
  reference targets an id that a merge in the same plan absorbed — plus the scope
  invariant. `OntologyRepository::validate_graph` is *not* the gate (advisory
  only per ADR-0008); it may be surfaced as a warning. A failed referential check
  trips `ROLLBACK`.
- Attribution: every mutation stamps/extends `Provenance` (actor/method/
  observed_at); no separate audit row.
- **Idempotency — per-variant target-state key** (a re-apply yields `unchanged`,
  never a duplicate write):

  | Mutation | Already-applied iff |
  |---|---|
  | Archive | `archived_at` already set on target |
  | Restore | `archived_at` already null on target |
  | Delete | target id already absent |
  | Merge | survivor already canonical; absorbed ids already archived/absent; no duplicate edges remain |
  | AddAlias | alias already in survivor's `aliases` |
  | RemoveAlias | alias already absent |
  | RewriteRelationship | (subject, predicate, object) already equals the new value |

### Failure, edge cases & resilience

- Verify-fail inside the SQLite transaction → `ROLLBACK` → graph unchanged; result
  carries verify findings and `Atomicity::SingleTransaction` (applied = 0).
- Idempotent reapply → `unchanged` counts, no error.
- Archiving an entity with already-archived edges, or restoring a node whose
  edges were independently archived → restore restores the consistent set; a
  partial restore is rejected with a clear finding.
- Cross-store: the port never claims all-or-nothing across engines; a future
  non-transactional backend reports `Atomicity::BestEffort` and the caller sees it.

### Quality attributes (NFRs)

- **Engine neutrality (ADR-0022):** no `Sql*` or SQL in `core/*`, the facade, or
  the binding. The deleted `.codex/hooks/check-surface-parity.sh` is a reinstated
  follow-up; reviewer-checked until then.
- **Operability:** `CapabilityReport` exposes the `graph_maintenance`
  capability; the apply result exposes its atomicity level; health metrics give
  point-in-time candidate/archived volumes per graph and source.
- **Safety:** irreversible delete is never the default; archive is always
  reversible.

## Tasks

### T1: Domain — archive field + maintenance plan/mutation/result types

**Depends on:** none
**Mode:** TDD.

**Tests:**
- serde round-trips `KnowledgeEntity`/`KnowledgeRelationship` with `archived_at`
  set and unset; `None` stays the default (no value migration). (AC: archive state)
- `MaintenanceMutation` variants carry their target ids + payload; `MaintenancePlan`
  carries a stable fingerprint over its sorted mutation list. (AC: preview, apply)
- `MaintenanceApplyResult` distinguishes applied / unchanged / failed counts,
  carries an `Atomicity` value, and echoes the plan fingerprint.
- `MaintenanceCandidate` round-trips with and without `review_status` and a
  reviewer actor.

**Approach:**
- Add `archived_at: Option<Timestamp>` to `KnowledgeEntity` and
  `KnowledgeRelationship` in `core/domain/src/knowledge.rs`.
- Add `core/domain/src/maintenance.rs` with the maintenance data contracts —
  `MaintenanceTarget`, `MutationKind`, `MaintenanceMutation` (+ `kind()`),
  `MutationSnapshot`, `MaintenanceMutationPreview`, `MaintenancePolicy`
  (+ `Default`), `CandidateKind`, `ReviewStatus`, `MaintenanceCandidate`,
  `Atomicity`, `VerifySeverity`, `MaintenanceVerifyFinding`, `ApplyKindCount`,
  `MaintenanceApplyResult`, `MaintenancePlan` (+ `::new`), and a dependency-free
  `plan_fingerprint` (FNV-1a over sorted canonical JSON). Structs and the enum's
  variant fields serialize camelCase (matching the domain convention; enums
  snake_case). Re-export from `core/domain/src/lib.rs`.
- Add a deterministic `plan_fingerprint` (hash over sorted, serialized mutations).
- Update `docs/domain-data-model.md` (entity/relationship tables + a maintenance
  section); regenerate contracts (`pnpm run contracts:generate`).

**Done when:** `cargo test -p engram-domain` green; `docs/domain-data-model.md`
updated; `contracts:generate` output committed and a re-run produces no further
diff (idempotent).

### T2: Maintenance port in core/knowledge

**Depends on:** T1
**Mode:** TDD.

**Tests:**
- A stub `GraphMaintenanceRepository` implements the trait; `build_plan` with
  `ApplyMode::Preview` returns a plan whose each `MaintenanceMutationPreview`
  carries before and after snapshots, and calls no mutating method (mock asserts
  zero writes). (AC: default non-mutating, preview)
- `detect_candidates` returns the typed candidate set from a stubbed graph.
- The trait is re-exported from `engram-knowledge` root (cross-crate resolve
  succeeds — AGENTS.md whitelist). (AC: surface reach)

**Approach:**
- Add `core/knowledge/src/maintenance.rs` defining `GraphMaintenanceRepository`
  plus `ApplyMode` and the list/filter
  request types (`EntityFilter`, `RelationshipFilter`, reusing `Cursor`/`Page<T>`
  from `core/domain/src/paging.rs`).
- Re-export at `core/knowledge/src/lib.rs`.

**Done when:** `cargo test -p engram-knowledge` green; trait resolvable from
outside the crate.

### T3: Deterministic candidate detectors

**Depends on:** T2
**Mode:** TDD.

**Tests:**
- Fixture graph: an entity with no edges and no `source_refs` → orphan; an entity
  with `confidence < threshold` → low-confidence; an ontology-violating edge →
  unsupported (via `validate_graph`); two name-colliding entities → duplicate (via
  `discover_collisions`). (AC: candidate detection)
- Detection is pure given (graph, policy) — same inputs ⇒ same candidate set.

**Approach:**
- Implement detectors as focused functions in
  `core/knowledge/src/maintenance/candidates.rs` (or behind the port), composing
  `EntityIdentityRepository::discover_collisions` and
  `OntologyRepository::validate_graph` plus graph-neighborship reads; no LLM, no
  embeddings. Ensure those primitives honor `archived_at IS NULL` so archived
  absorbed entities don't resurface as duplicate/unsupported candidates (AC10).

**Done when:** candidate-fixture tests green; no detector depends on an LLM or
embedding provider.

### T4: SQLite adapter — archive/restore + list/filter/paginate + active reads

**Depends on:** T1, T2
**Mode:** TDD + integration.

**Tests:**
- Archive sets `archived_at`; the archived row is excluded from `list_entities`/
  `list_relationships`, from retrieval graph lanes, and from ADR-0018 declared-set
  reconciliation (assert via a fixture that retracts a source whose entity was
  archived — the archived row is not resurrected); restore clears `archived_at`
  and the row reappears. (AC: archive/restore, active exclusion)
- Archiving a node archives its incident edges; restore returns both; the active
  graph shows no dangling edge. (AC: referential integrity)
- `list_*` honors graph/scope/source/kind/confidence filters and paginates via
  `Cursor`. (AC: list/filter/paginate)
- Archiving/restoring an entity stamps `Provenance` with the maintenance actor
  and method. (AC: attribution)

**Approach:**
- `adapters/sqlite/src/knowledge/maintenance.rs`: add `archived_at` column +
  active-filter index (`schema.rs`); implement `archive`/`restore` (with edge
  cascade) and `list_entities`/`list_relationships` with filters + cursor paging.
- Apply the `archived_at IS NULL` filter at the read-seam so every active path
  honors it: maintenance list/filter, retrieval/recall graph lanes,
  `graph_neighbors`/`graph_subgraph`, and ADR-0018 reconciliation — not only the
  new maintenance-port methods.

**Done when:** `cargo test -p engram-store-sqlite` (or the SQLite knowledge
adapter crate) green; a migration adds the column with `archived_at IS NULL`
default.

### T5a: SQLite adapter — build_plan + transactional apply + verify/rollback + idempotency

**Depends on:** T2, T3, T4
**Mode:** TDD + integration.

**Tests:**
- `build_plan(ApplyMode::Preview)` returns a `MaintenancePlan` whose every
  `MaintenanceMutationPreview` carries both before and after snapshots; it
  performs no write. (AC: preview)
- `apply_plan(ApplyMode::Preview)` stages the plan but does NOT commit — no row
  changes; only `Apply` commits. (AC: default non-mutating)
- `apply_plan(Apply)` stages all mutations in one `conn.transaction()`; on success
  the result reports `Atomicity::SingleTransaction`. (AC: atomic apply)
- Forcing the referential-integrity verify to fail trips `ROLLBACK` — induce it
  with a plan whose staged mutations leave a dangling reference (e.g. a
  `RewriteRelationship` pointing an edge at an id the plan absorbs without
  rewiring) — and assert the graph is byte-for-byte unchanged (row counts + a
  checksum), with the result reporting applied = 0 and the verify finding. (AC:
  rollback; the verify is the enforceable referential check, not advisory
  `validate_graph`)
- Reapplying a completed plan yields `unchanged` for each variant T5a implements
  (Archive / Restore / Delete), with no new writes. (AC: idempotency)

**Approach:**
- `adapters/sqlite/src/knowledge/maintenance.rs`: `build_plan` (preview from the
  mutation list) and `apply_plan` opening `conn.transaction()` and staging
  Archive/Restore/Delete via tx-aware store operations, with the
  referential-integrity verify before commit. Delete is staged SQL inside the open
  transaction (a scope-checked hard delete on the target id), NOT a call into
  `delete_*`/`delete_graph` — those re-enter the shared `Mutex`/open their own
  transaction and would deadlock or break single-tx atomicity.
- **Tx-aware discipline:** every mutation the apply stages runs against the open
  `Transaction`, never re-entering the store's shared `Mutex`/connection. Existing
  shared-mutex methods that would nest (merge, delete) get tx-aware cores — merge
  in T5b; delete via a tx-aware delete core or staged SQL here.

**Done when:** the three vertical integration tests (apply-succeeds, verify-fail
rollback, idempotent reapply of Archive/Restore/Delete) green on SQLite.

### T5b: SQLite adapter — rewrite_relationship + manage_alias + merge (tx-aware)

**Depends on:** T5a
**Mode:** TDD.

**Tests:**
- `rewrite_relationship` edits a predicate or endpoint in place without a full
  re-put and stamps `Provenance`; re-apply is `unchanged` when the value already
  matches. (AC: rewrite, attribution, idempotency)
- `manage_alias` adds/removes aliases; add is idempotent (alias present ⇒
  `unchanged`) and remove is idempotent (alias absent ⇒ `unchanged`). (AC: alias)
- `merge` runs inside the apply transaction via a tx-aware merge core, preserves
  `Provenance` on the survivor, **archives** (not hard-deletes) absorbed entities
  AND coalesced duplicate relationships, and re-applies as `unchanged` when the
  duplicates are already absorbed. (AC: merge, attribution; AC11 reversibility)
- Normalized-exact-identity resolution is NOT duplicated on the maintenance
  port — the facade (T6) composes `EntityIdentityRepository::resolve_or_put`
  for it (re-resolving an already-canonical name returns the existing entity,
  no duplicate). (AC: alias / normalized identity)

**Approach:**
- Extract a tx-aware merge core from `SqlIdentityStore::consolidate_entities`
  (`adapters/sqlite/src/knowledge/identity.rs`) that takes an absorb disposition
  (`Archive` vs `Delete`) and runs against an open `&mut Transaction`. The
  disposition governs BOTH absorbed entities AND the relationship-coalesce step:
  under `Archive`, the maintenance merge does `UPDATE ... SET archived_at` on
  duplicate-keyed relationships scoped to the merged entity ids (not the global
  hard-`DELETE` the current coalesce issues), so the merge is fully reversible;
  under `Delete`, `consolidate_entities` keeps its existing hard-`DELETE` behavior
  unchanged (regression net, `sqlite-consolidation` invariant). The existing
  `consolidate_entities` becomes a thin open-tx → core(`Delete`) → commit wrapper.
- Implement `rewrite_relationship` and `manage_alias` (tx-aware). Normalized-
  identity resolution is left to the facade (T6), which composes
  `EntityIdentityRepository::resolve_or_put` — it is not a maintenance-port
  method.

**Done when:** per-op tests green; rewrite/alias/merge each idempotent on re-apply;
existing `consolidate_entities` tests still green (regression).

### T5c: SQLite adapter — graph_health aggregates

**Depends on:** T3, T4
**Mode:** TDD.

**Tests:**
- `graph_health(scope)` returns per-graph and per-source counts: orphan,
  low-confidence, unsupported, duplicate candidate volumes, and archived-row
  count, as point-in-time read-only aggregates. (AC: health)

**Approach:**
- Implement `graph_health` in `adapters/sqlite/src/knowledge/maintenance.rs` as
  aggregate reads over the detector outputs and the `archived_at` column.

**Done when:** health-fixture test green; aggregates match a hand-computed expected set.

### T6: Integration facade + CapabilityReport + backend wiring

**Depends on:** T5a, T5b, T5c
**Mode:** goal-based check.

**Tests:**
- `provider.graph_maintenance()` accessor and `require_graph_maintenance()` return
  the capability or `CapabilityUnsupported`; `CapabilityReport` has a new
  `graph_maintenance` field (distinct from the existing backend-neutral
  `maintenance` field) defaulting to `FeatureDisabled`; `all_supported()` includes
  it. (AC: surface reach)
- The SQLite backend recipe wires the adapter into the provider; the capability
  flips from `FeatureDisabled` to `Supported`.

**Approach:**
- `core/integration/src/provider.rs`: field, accessor, `require_graph_maintenance()`,
  builder setter, builder-forward field, test entry. `core/integration/src/capability.rs`:
  a new `graph_maintenance` field (not the existing `maintenance`) + builder
  default + `all_supported()` line + `graph_maintenance_supported()` + setter;
  update the capability-count test to include `graph_maintenance` (the only key
  this change adds); the stale "nineteen" name and the pre-existing missing keys
  (`identity`, `procedures`) are out-of-scope drift — note them in the PR but do
  not expand scope. Wire into the SQLite recipe in `adapters/integration` (or
  `backends/sqlite`).

**Done when:** `cargo check --workspace` green; the capability appears in a real
`CapabilityReport` from the SQLite backend as `graph_maintenance: Supported`.

### T7: N-API binding + @engram/node TS transport

**Depends on:** T6
**Mode:** goal-based check.

**Tests:**
- `requireGraphMaintenanceApi()` returns a `NativeGraphMaintenanceApi` whose
  methods round-trip plan/apply through JSON encode/decode. (AC: surface reach)
- `@engram/node` exports the binding type and transport methods; `pnpm typecheck`
  green.

**Approach:**
- `bindings/node/src/provider.rs`: `requireGraphMaintenanceApi` proxy +
  `NativeGraphMaintenanceApi` struct + `#[napi]` impl (JSON-in/JSON-out).
- `packages/node/src/binding.ts`: `NativeGraphMaintenanceApiBinding` interface +
  method on `NativeProviderBinding`; `packages/node/src/provider.ts`: transport
  methods; re-export from `index.ts`.

**Done when:** `pnpm typecheck` + the N-API build green; a TS caller can build
and apply a plan through `@engram/node`.

### T8: engram-mcp tools (Rust stdio + TS HTTP)

**Depends on:** T7
**Mode:** goal-based check.

**Tests:**
- The full maintenance surface registers and dispatches over MCP:
  `list_maintenance_candidates`, `list_maintenance_entities`,
  `list_maintenance_relationships`, `build_maintenance_plan`,
  `apply_maintenance_plan`, `archive`, `restore`, `delete`, `merge`,
  `rewrite_relationship`, `manage_alias`, `graph_health`. (AC: surface reach)
- A dry-run (`build_maintenance_plan`, or any mutating op without the apply flag)
  returns a preview with no mutation; an apply without the apply flag is rejected
  as non-mutating. (AC: default non-mutating)

**Approach:**
- `mcp/engram-mcp/src/main.rs`: `registry.register(ToolRecord {...})` for each
  tool; handlers in `mcp/engram-mcp/src/maintenance.rs` calling
  `app.provider.require_graph_maintenance()`.
- `packages/runtime/src/mcp/tools.ts`: mirror tools dispatching through the
  transport.

**Done when:** both MCP servers expose the full tool set; a manual tool call
returns a dry-run preview and (with the apply flag) applies transactionally.

## Rollout

- **Delivery:** the capability ships behind its `CapabilityReport` entry
  (`graph_maintenance`), default `FeatureDisabled` until T6 wires the SQLite
  backend, then `Supported`. No flag, no big-bang — additive port + adapter +
  tools. Reversible by design: every mutation is archive-reversible; disabling the
  capability hides the tools without touching stored data.
- **Infrastructure:** none — SQLite column addition only (additive, nullable
  `archived_at`, no backfill needed).
- **External-system integration:** none.
- **Deployment sequencing:** T1 (domain+docs) before T2–T5 (port+adapter); the
  schema migration (T4) lands before any code path reads `archived_at`; T6
  (facade) before T7 (binding) before T8 (MCP).

## Risks

- **No public transaction port.** T5a stages the plan inside the adapter's own
  `conn.transaction()`. If a maintenance plan must mix knowledge-graph writes
  with memory/belief writes, single-transaction atomicity breaks — that mix is
  explicitly out of scope (spec *Never do*: backend-dependent, single-backend).
- **Active-read filtering sprawl.** Every knowledge read path must honor
  `archived_at IS NULL` — maintenance list/filter, retrieval/recall graph lanes,
  `graph_neighbors`/`graph_subgraph`, and ADR-0018 reconciliation; missing one
  leaks archived rows. Mitigated by applying the filter at the read-seam (T4) and
  a reconciliation fixture that asserts an archived row is not resurrected.
- **Archive retention.** Archived rows accumulate outside ADR-0018 convergence;
  a reaping policy is deferred (spec *Ask first*). Risk: unbounded growth until
  that lands.
- **Parity lint absent.** Surface parity is reviewer-checked until
  `.codex/hooks/check-surface-parity.sh` is reinstated; a missed surface could
  ship asymmetry. Mitigated by the T6/T7/T8 goal-based checks tracing the `graph`
  precedent file list and naming the full MCP tool set.

## Changelog

- 2026-08-07: initial plan, drafted against ADR-0027 (Proposed) and the
  assumption-surfacing checkpoint in `spec.md`.
- 2026-08-07: spec-mode adversarial review fixes — backend-neutral AC3 (no SQLite
  in the contract); atomicity level rides in `MaintenanceApplyResult`, not
  `CapabilityReport`; new `graph_maintenance` field distinct from `maintenance`;
  T5 split into T5a (apply/verify/idempotency), T5b (rewrite/alias/merge),
  T5c (health); named the enforceable referential-integrity verify (not advisory
  `validate_graph`); per-variant idempotency table; struck the orphaned "approved
  plan id"; full MCP tool surface; Provenance + before/after + normalized-identity
  test coverage; convergence read-path enumeration; verification-mode labels.
- 2026-08-07: spec-mode adversarial review pass-2 fixes — named the tx-aware merge
  refactor (Blocker: `consolidate_entities` re-acquires the shared Mutex, so merge
  cannot nest in the apply tx); maintenance merge **archives** absorbed entities
  (reversible) via a shared tx-aware core with an absorb disposition, keeping
  `consolidate_entities` behavior unchanged; AC11 carves out Merge (archives) and
  escalated Delete (permanent); narrowed T5a idempotency to its implemented
  variants with a T5b per-variant test; added the Delete producer (port + MCP +
  AC13) reusing `delete_*`; documented the `plan_fingerprint` consumer; added
  `review_status`/reviewer to `MaintenanceCandidate`; described the verify-fail
  test setup; aligned Objective/AC10 candidate vocabulary; de-engined the
  Always-do prose; made the T6 stale-test-name fix directive.
- 2026-08-07: spec-mode adversarial review pass-3 fixes — disposition now governs
  the merge relationship-coalesce step too (under `Archive`, scoped `UPDATE … SET
  archived_at` instead of the global hard-`DELETE`), so merge is fully reversible
  for entities AND relationships (AC11 "Delete is the one permanent mutation"
  holds); T5a Delete is named as staged SQL in the open tx (not a call into
  `delete_*`/`delete_graph`, which re-enter the shared Mutex); `Constrained by:`
  flags ADR-0027 as Proposed; T6 scoped to add only `graph_maintenance` (the
  stale name and missing `identity`/`procedures` are out-of-scope drift); T1
  contracts Done-when reworded to idempotent.
- 2026-08-07: T1 implemented + adversarial-review fixes — `MaintenancePolicy`
  (and the full T1 type set) landed in `core/domain` (not T2) because
  `MaintenancePlan.policy` requires it in-domain; structs + the enum's variant
  fields serialize camelCase (Blocker: was snake_case, mismatching embedded
  `KnowledgeEntity`); `plan_fingerprint` uses `expect` not `unwrap_or_default`
  (no silent collision); `RewriteRelationship` round-trip test added;
  fingerprint doc narrowed to "mutations digest"; pre-existing `cargo fmt` drift
  in 5 unrelated files reverted to keep T1 focused.
- 2026-08-08: T2 implemented + adversarial-review fixes — the `GraphMaintenanceRepository`
  port exposes only list/detect/build_plan/apply_plan/graph_health; the six granular
  ops collapse into `MaintenanceMutation` variants staged via single-mutation plans
  (focused port; granular UX is the MCP layer, T8). AC8 amended: normalized-exact-
  identity resolution is NOT duplicated on the maintenance port — the facade (T6)
  composes `EntityIdentityRepository::resolve_or_put`. Added domain port-supporting
  types (ApplyMode, EntityFilter, RelationshipFilter, MaintenancePlanRequest,
  MaintenanceHealth) + `previews` on `MaintenancePlan`; `tokio` dev-dep on
  `engram-knowledge` for async port tests.
