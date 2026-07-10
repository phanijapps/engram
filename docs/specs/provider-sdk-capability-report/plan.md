# Plan: Provider SDK + capability report (S1)

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog
> at the bottom.

## Approach

Complete the SDK facade's contract surface, in four independent moves that each
ship and test on their own:

1. **CapabilityReport → 18 keys.** Today the report has 10 builder families.
   Add the 8 missing keys (`hybrid_search`, `episodes_evidence`, `contradiction`,
   `atomic_batch`, `unified_recall`, `export_import`, `maintenance`,
   `observability`) and have the unwired/empty provider mark every not-yet-built
   area `Unsupported { FeatureDisabled }`. The `CapabilityState`/`CapabilityReason`
   vocabulary already exists in `engram-domain` — no new variant needed; this is
   report-completion, not model invention. The brief-area → key mapping is fixed
   in §Design (Data & schema).
2. **CoreError → 10 categories.** Add the ~5 missing structured variants to
   `engram-runtime::CoreError` (the runtime crate that owns the error surface
   per `reference.md`), mapping to the brief's 10 categories. Existing variants
   already cover five.
3. **Neutrality gate (ADR-0022 rule 1).** A `check-engine-neutrality` gate that
   fails if an engine symbol, engine-crate import, or raw-SQL literal appears in
   the gated surface (8 clean port crates + `core/integration/src/{provider,capability}.rs`).
   It is green today and must stay green; `engram-runtime`, `config.rs`,
   `core/eval`, and `bindings/node` are allow-listed deferred debt. Matches the
   repo's `check-contracts.sh` / `check-docs.sh` hook convention.
4. **SDK docs.** Rustdoc `EngramProvider` as the canonical entry and add a
   host-usage guide under `docs/guides/how-to/`.

T1/T2/T3 are independent (no inter-deps); T4 follows T1 so the docs describe the
completed report. Riskiest part is T1/T2 touching a **stability-governed**
surface (`CapabilityReason` strings are declared stable; `CoreError` is spoken
by every crate) — so both are purely additive (new keys/variants, no removal or
rename). SQLite is untouched throughout.

## Constraints

- **`rust-crate-integration` spec (Implementing)** — owns the provider facade,
  `CapabilityReport`, the `CoreError` extension already landed
  (`CapabilityUnsupported`, `EmbeddingSpaceMismatch`, `MigrationManifestStale`),
  `EmbeddingProvider`/`VectorIndex` traits, migration/import API, and the
  conformance harness. T1/T2 **extend** those surfaces; they do not rebuild them.
  S1 is the delta over that spec, driven by the newer brief + ADR-0022.
- **ADR-0022** — rule 1 (engine neutrality) is enforced by T3; rules 2–3 (grid
  layout, `backends/` crate) are deferred and explicitly not done here.
- **`docs/architecture/reference.md`** — `CoreError`/`CoreResult` live in
  `engram-runtime`; typed errors, no stringly public contracts; fail-closed on
  unsupported paths. Dependency direction: `domain ← runtime ← {memory,
  knowledge, retrieval} ← orchestration ← … ← adapters ← binding ← packages`.
- **Capability-report stability contract** (`core/domain/src/capability.rs`) —
  existing `CapabilityReason` string forms are stable; new reasons may be added,
  existing ones never removed/renamed. T1 adds keys, never renames.
- **engram-host-sdk brief** (S1) — the 18 capability areas and 10 error
  categories are the source list.

## Construction tests

**Integration tests:** a workspace test (or the neutrality hook invoked from
`cargo test` / the gate) asserting the port-trait crates + `core/integration`
provider/capability modules contain no engine symbols — the ADR-0022 rule-1
gate (T3). Per-task unit tests cover T1/T2 invariants.

**Manual verification:** `cargo doc -p engram-integration` builds clean;
`docs/guides/how-to/use-engram-provider.md` renders; capability report from a
no-backend bootstrap lists all 18 areas with explicit states.

## Design (LLD)

Conforms to `docs/architecture/reference.md` (normative). `Shape: data` →
`Data & schema` + `Interfaces & contracts` sub-sections only.

### Design decisions

- **Additive-only to stability-governed surfaces.** New `CapabilityReport` keys
  and new `CoreError` variants; no removal/rename. Traces to: AC1, AC2, AC3.
- **Neutrality gate as a `check-*` hook, not a runtime check.** Matches the
  repo convention (`check-contracts.sh`, `check-docs.sh`); enforced at the
  validation gate, not in shipped code. Traces to: AC4.
- **Deferred-debt layers excluded from the gate, documented not hidden.**
  `SqliteStorageLayout`, `core/eval`, `bindings/node` are named exceptions.
  Traces to: AC4.

### Data & schema

- `CapabilityReport` (in `core/integration/src/capability.rs`) gains 8 fields +
  builder methods: `hybrid_search`, `episodes_evidence`, `contradiction`,
  `atomic_batch`, `unified_recall`, `export_import`, `maintenance`,
  `observability`. (Existing 10: memory, knowledge, graph, ontology, taxonomy,
  beliefs, hierarchy, retrieval, vectors, migration → 18 total.)
  `EngramProvider::empty()` sets the not-yet-built ones to
  `Unsupported { FeatureDisabled }`. **`CapabilityReport::new()` and
  `all_supported()` are positionally hard-coded to 10 fields today — both must
  be extended to the new 18, or `all_supported()` silently returns `true` with a
  new family `Unsupported`.** Traces to: AC1, AC2.
- **`migration` vs `export_import` split (justified):** `migration` =
  schema-version migration (existing responsibility, ADR-0005); `export_import`
  = semantic-state export/import + backend-to-backend movement (brief #18). They
  answer different questions (is the schema current? vs can I move data?), so
  they are distinct keys — not a rename of `migration`.
- **Brief-area → key mapping (authoritative for the T1 enumeration test):**

  | Brief row | CapabilityReport key |
  |---|---|
  | #4 Memory facts | `memory` |
  | #5 Knowledge graph (CRUD) | `knowledge` |
  | #5 Knowledge graph (traversal) | `graph` |
  | #3 / #11 Vector search + embedding | `vectors` |
  | #3 Hybrid lexical/vector | `hybrid_search` (new) |
  | #6 Episode / evidence | `episodes_evidence` (new) |
  | #7 Ontology | `ontology` |
  | #8 Taxonomy | `taxonomy` |
  | #9 Belief storage | `beliefs` |
  | #3 / #9 Contradiction tracking | `contradiction` (new) |
  | #10 Atomic batch | `atomic_batch` (new) |
  | #12 Unified recall | `unified_recall` (new) |
  | #13 Maintenance | `maintenance` (new) |
  | #14 Observability | `observability` (new) |
  | #18 Migration (schema) | `migration` |
  | #18 Export / import (data) | `export_import` (new) |
  | (existing capability, not a brief row) | `hierarchy` |
  | (existing capability, not a brief row) | `retrieval` (lexical/keyword) |

  Brief meta rows #1, #2, #3 (discovery), #15, #16, #17 are infrastructure, not
  capability keys.
- `CoreError` (in `engram-runtime/src/error.rs`) gains variants
  `MigrationFailed { reason }`, `TransactionUnsupported { capability }`,
  `ValidationFailed { reason }`, `BackendTransient { backend, message }`,
  `BackendPermanent { backend, message }`. Brief-category mapping:
  unsupported capability→`CapabilityUnsupported`; backend unavailable→
  `ProviderUnavailable`; migration required→`MigrationPending`; migration
  failed→`MigrationFailed`; embedding dim mismatch→`EmbeddingSpaceMismatch`;
  validation failed→`ValidationFailed`; conflict→`Conflict`; transaction
  unsupported→`TransactionUnsupported`; transient→`BackendTransient`;
  permanent→`BackendPermanent`. Traces to: AC3.

### Interfaces & contracts

- No new `contracts/<type>/` interface — `CapabilityReport` and `CoreError` are
  Rust model surfaces, not REST/event contracts. The stable external surface is
  the serialized `CapabilityReport` JSON and the `CoreError` variant set; both
  stay backward-compatible (additive). Traces to: AC1, AC3.

## Tasks

### T1: CapabilityReport covers all 18 keys (10 existing + 8 new)

**Depends on:** none

**Tests:**
- A test enumerates the 18 `CapabilityReport` keys (per the §T1 mapping table)
  and asserts each has an explicit `CapabilityState` on a fully-built report
  (no field absent). (AC1)
- A test on `EngramProvider::empty()` (no backend wired) asserts every
  not-yet-built area is `Unsupported { FeatureDisabled }` — present, not
  absent. (AC2)
- A regression test asserts `CapabilityReport::all_supported()` returns `false`
  on an `empty()` provider — guards against `all_supported()` / `new()` silently
  staying at 10 fields after the new ones are added. (AC1)

**Approach:**
- Add the 8 new fields + builder methods + `empty()` defaults in
  `core/integration/src/capability.rs` and `core/integration/src/provider.rs`,
  per the §T1 mapping table.
- **Extend `CapabilityReport::new()` and `CapabilityReport::all_supported()`**
  to cover the new fields — both are positionally hard-coded to 10 today and
  will silently misreport if left unchanged.
- Keep it additive: no rename of existing families, no new `CapabilityState`
  variant.

**Done when:** both unit tests green; `EngramProvider::empty().capabilities()`
lists all 18 areas with explicit states.

### T2: CoreError covers the brief's 10 error categories

**Depends on:** none

**Tests:**
- A test constructs each new variant and asserts it carries structured fields
  (not a bare string). (AC3)
- A test asserts each of the brief's 10 categories maps to a distinct
  `CoreError` variant, and that discrimination is by variant, not string match.
  (AC3)
- A test asserts the new variants' `to_redacted()` strips a planted SQL snippet,
  absolute path, and raw vector from the free-form `message`/`reason` fields
  (esp. `BackendTransient` / `BackendPermanent` / `MigrationFailed` /
  `ValidationFailed`) — matching the rust-crate-integration redaction contract.
  (AC3)

**Approach:**
- Add `MigrationFailed { reason }`, `TransactionUnsupported { capability }`,
  `ValidationFailed { reason }`, `BackendTransient { backend, message }`,
  `BackendPermanent { backend, message }` to `core/runtime/src/error.rs`.
- Extend **`Display` + `to_redacted()`** (apply `redact_message` to the new
  free-form `message`/`reason` fields) + the `DiagnosticError` wrapper. Every
  existing variant has an explicit `to_redacted()` arm, so missing arms are a
  compile error — extend all three match sites. Additive only.

**Done when:** error-category tests green; `cargo check --workspace` clean.

### T3: Neutrality gate (ADR-0022 rule 1)

**Depends on:** none · **Mode:** TDD (inject-then-revert invariant) with a goal-based baseline (gate green on the current tree)

**Tests:**
- Baseline: the gate exits 0 on the current tree — the gated surface (8 clean
  port crates + `core/integration/src/provider.rs` + `core/integration/src/capability.rs`)
  is clean today and the deferred-debt layers are allow-listed. (AC4)
- A **parametric** injection test covers all three forbidden pattern classes —
  (a) a `Sql*` type name, (b) an engine-crate import (`rusqlite::…` /
  `engram_store_*`), (c) a raw-SQL literal — each temporarily injected into a
  gated file, asserting the gate exits non-zero, then reverted. (AC4)

**Approach:**
- Add `.codex/hooks/check-engine-neutrality.sh`: grep the gated surface for the
  three forbidden classes; allow-list the deferred-debt layers (`engram-runtime`,
  `core/integration/src/config.rs`, `core/eval`, `bindings/node`). Wire into
  `AGENTS.md` § Validation and the work-loop preflight alongside
  `check-contracts.sh` / `check-docs.sh`.

**Done when:** gate passes on the current tree; the parametric injection test
fires on all three pattern classes; listed in `AGENTS.md` § Validation.

### T4: EngramProvider documented as the canonical SDK entry

**Depends on:** T1

**Tests:**
- `cargo doc -p engram-integration --no-deps` builds clean. (AC5)
- `docs/guides/how-to/use-engram-provider.md` exists and shows open-from-config
  → read capability report → call a supported family end to end. (AC5)

**Approach:**
- Expand rustdoc on `EngramProvider` / `bootstrap` (point at the 18-area
  capability report from T1). Write the host-usage guide under
  `docs/guides/how-to/`.

**Done when:** `cargo doc` clean and the guide present.

## Rollout

- **Delivery:** additive Rust API change (new report keys + error variants) +
  a new dev-time gate hook. No flag, no migration, fully reversible (revert the
  commit). Nothing irreversible: no data migration, no published event, no
  schema change.
- **Infrastructure:** none — the neutrality hook runs in the existing
  validation gate; no new compute/storage/secrets.
- **External-system integration:** none.
- **Deployment sequencing:** T1 and T4 (docs) should land together or T1 before
  T4 so the guide describes the completed report. T2 and T3 are independent.

## Risks

- **Stability surface churn.** `CapabilityReport` and `CoreError` are spoken
  across the workspace; an accidental rename/remove would break dependents.
  Mitigation: additive-only; existing tests + `cargo check --workspace` guard.
- **Neutrality gate false positives.** A too-broad forbidden pattern could make
  the gate noisy. Mitigation: scope to type names (`Sql[A-Z]…`) + engine-crate
  imports + raw-SQL literals; allow-list the deferred-debt layers (`engram-runtime`,
  incl. its SQL-redaction regex, `core/integration/src/config.rs`, `core/eval`,
  `bindings/node`).
- **Brief-area → key mapping ambiguity.** *(Resolved by design)* the mapping is
  fixed authoritatively in §Design (Data & schema), so the T1 enumeration test is
  unambiguous; the host-usage guide (T4) re-states it for users.

## Changelog

- 2026-07-09: initial plan (S1 of engram-host-sdk brief; conforms to ADR-0022 + reference.md).
- 2026-07-09: reconciled against the pre-existing **Implementing** `rust-crate-integration` spec, which already owns the provider facade + `CapabilityReport` + the `CoreError` extension + conformance harness. S1 is reframed as the *delta* over it (18-area report completion, the 5 missing error categories, the ADR-0022 neutrality gate, SDK docs) — not a re-establishment of the facade. Added to Constraints + Assumptions.
- 2026-07-09: addressed spec-mode adversarial-review findings (single pass). Critical: (1) `engram-runtime` re-exports home-grown `Sql*` types → descope it from the gate as deferred debt (logged in `docs/backlog.md`); gate surface is now the 8 clean port crates + `core/integration/src/{provider,capability}.rs` (file list). (2) "18 brief areas" was incoherent → committed to an explicit 18-key set (10 existing + 8 new) with an authoritative brief→key mapping table in §Design. Major: T1 extends `new()`+`all_supported()` (regression test added); T2 extends `to_redacted()` with `redact_message` (redaction test added); `migration` vs `export_import` split justified; T3 injection made parametric over all 3 forbidden pattern classes. Minor: T3 mode labeled TDD; `orchestration` added to the dep chain; ADR-0022 scope-narrowing recorded via backlog anchor; mapping table moved into T1 Design; gate carve-out stated as a file list.
