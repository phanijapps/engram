# Plan: Episode / evidence (provenance) query API (S2)

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog
> at the bottom.

## Approach

S2 adds the **read half** of the `episodes_evidence` capability: a backend-neutral
`ProvenanceQuery` port + provider handle, backed by a SQLite impl that reads the
`Provenance`/`EvidenceRef` already embedded in records. Four sequential moves:

1. **Port + enum extension.** Define `ProvenanceQuery` in `core/integration/src/provenance.rs`;
   add `Relationship` and `Belief` to `EvidenceTargetType` (`core/domain`) so the port's typed
   target input can name every record kind that carries provenance.
2. **SQLite impl.** `SqlProvenanceQuery` in `adapters/integration/src/provenance.rs`, composing
   `Arc<SqlKnowledgeStore>`. v1 backs entity/relationship/source (scope-column tables) by
   listing + Rust-side filtering over each record's `Provenance`/`evidence`; the other target
   kinds return `CoreError::CapabilityUnsupported`. No schema change.
3. **Provider + capability.** `EngramProvider` gains a `provenance: Option<Arc<dyn ProvenanceQuery>>`
   handle + `provenance()` accessor; `bootstrap_provider` constructs `SqlProvenanceQuery`, runs
   the fixture, and flips `episodes_evidence` to `Supported` on pass.
4. **Conformance fixture + gate.** Writes entity/relationship/source records carrying
   `Provenance`/`EvidenceRef` and recovers them through the handle; adds `core/integration/src/provenance.rs`
   to the neutrality gate.

Strictly sequential (T1→T2→T3→T4). Riskiest part is keeping the port engine-neutral while the
impl composes `Sql*` stores — mitigated by the ADR-0022 gate, now covering `provenance.rs`.

## Constraints

- **ADR-0022** — the `ProvenanceQuery` port (`core/integration/src/provenance.rs`) must not name an
  engine type or hold SQL; the SQLite impl lives in `adapters/integration`.
- **`rust-crate-integration` spec (Implementing)** — owns the facade; S2 extends it with one new
  handle + capability flip, additive only.
- **S1 (`provider-sdk-capability-report`)** — owns the `episodes_evidence` capability key
  (currently `Unsupported { FeatureDisabled }`); S2 flips it.
- **`docs/architecture/reference.md`** — typed errors (`CoreResult`), fail-closed on unsupported;
  dependency direction `domain ← runtime ← {memory, knowledge, retrieval} ← orchestration ← … ←
  adapters ← binding`. Read path backend-agnostic behind ports (ADR-0009).
- **No new storage** — read `Provenance`/`EvidenceRef` from existing `record_json`; no migration.

## Construction tests

**Integration tests:** the conformance fixture (T4) is the cross-cutting integration test —
writes records with evidence, queries through the wired provider handle, asserts recovery + the
capability flip. Per-task unit tests cover the port contract (T1) and the SQLite filtering (T2).

**Manual verification:** from a bootstrapped SQLite provider, query evidence for a known
entity/relationship and confirm the `Provenance`/`EvidenceRef` round-trip; confirm an unwired
provider reports `episodes_evidence` `Unsupported`.

## Design (LLD)

Conforms to `docs/architecture/reference.md`. `Shape: service` →
`Interfaces & contracts`, `Data & schema`, `Failure, edge cases & resilience`.

### Design decisions

- **Read-only for v1; write-side deferred.** The query half ships now; a dedicated
  *attach-evidence-to-existing-record* write op is documented (spec + backlog), not built.
  Traces to: AC1, AC6.
- **Extend `EvidenceTargetType` with `Relationship` + `Belief`** (additive domain extension) so the
  port's typed input can name every provenance-bearing record kind. Traces to: AC1.
- **v1 supports entity/relationship/source; the rest return `CapabilityUnsupported`.** Committed
  (not conditional): memory/belief/document/chunk/concept/event/url are unsupported in v1 because
  their scope-safe listing is not wired (documents/chunks lack scope columns; memory listing has no
  scope param; no queryable belief path). Wiring them is an `Ask first` follow-up. Traces to: AC1.
- **Port in `core/integration`, impl in `adapters/integration`.** Facade-level trait composing
  existing handles (no new crate); the `Sql*`-composing impl is engine-specific, gated by ADR-0022.
  Traces to: AC1, AC2, AC5.
- **No schema change — filter in Rust over `record_json`.** Acceptable at demo scale; an
  indexed/materialized evidence table is an `Ask first` follow-up. Traces to: AC2.

### Interfaces & contracts

`ProvenanceQuery` trait (`core/integration/src/provenance.rs`), all ops take `&Scope` + an optional
`TimeWindow { from: Option<Timestamp>, to: Option<Timestamp> }` filtering on `Provenance.observed_at`:

- `provenance_for(target: EvidenceTargetType, id: &str, scope) -> CoreResult<Option<Provenance>>`
- `evidence_for(target: EvidenceTargetType, id: &str, scope) -> CoreResult<Vec<EvidenceRef>>`
- `provenance_by_source(source_id: &str, scope, tw) -> CoreResult<Vec<ProvenanceEntry>>`
- `evidence_by_scope(scope, tw, limit) -> CoreResult<Vec<ProvenanceEntry>>`

`ProvenanceEntry { target: EvidenceTargetType, target_id: String, provenance: Provenance }` (defined
in `core/integration/src/provenance.rs`). `EvidenceTargetType` (extended) is the typed target
discriminator for every op. v1 returns data for Entity/Relationship/Source; the other variants return
`CoreError::CapabilityUnsupported { capability: "episodes_evidence", .. }`. Traces to: AC1.

### Data & schema

No new table, no migration. Per v1 target kind, the impl uses the existing scope-column listing and
deserializes each `record_json`:

- **Entity** → `list_entities_by_source` / entity lookup; read `KnowledgeEntity.provenance`.
- **Relationship** → `list_relationships_by_source`; read `KnowledgeRelationship.provenance` + `KnowledgeRelationship.evidence`.
- **Source** → source lookup; read `KnowledgeSource.provenance`.

Documents/chunks/concept/event/url/memory/belief short-circuit to `CapabilityUnsupported` in v1
(no scope-safe listing wired). Traces to: AC2.

### Failure, edge cases & resilience

- Read-only — no mutation path.
- No matches → empty `Vec` / `None`, not an error (distinct from `CoreError::NotFound`, which is for a
  missing *record*).
- Unsupported target kind → `CoreError::CapabilityUnsupported`, typed, not a silent empty.
- `MemoryContent.provenance` is `Option<Provenance>` — when memory targets are wired later, absent
  provenance returns `None`/empty, not an error. (Not exercised in v1; noted for the follow-up.)

## Tasks

### T1: ProvenanceQuery port + EvidenceTargetType extension

**Depends on:** none · **Mode:** TDD

**Tests:**
- An in-memory stub impl round-trips: given a record's `Provenance`, the four ops return it; a
  disjoint `observed_at` window yields empty/None. (AC1)
- The stub returns `CoreError::CapabilityUnsupported` for the v1-unsupported target kinds
  (memory/belief/document/chunk). (AC1)

**Approach:**
- Add `Relationship` and `Belief` variants to `EvidenceTargetType` in `core/domain/src/provenance.rs`
  (additive; update its round-trip test).
- Add `core/integration/src/provenance.rs` with the `ProvenanceQuery` trait + `TimeWindow` + `ProvenanceEntry`;
  re-export from `lib.rs`.
- Add `core/integration/src/provenance.rs` to `.codex/hooks/check-engine-neutrality.sh` `GATED_PATHS`.

**Done when:** port compiles, re-exported, enum extension round-trips, stub tests green, gate green (+ AC7: workspace tests green).

### T2: SQLite ProvenanceQuery impl (adapters/integration)

**Depends on:** T1 · **Mode:** TDD

**Tests:**
- Given `SqlKnowledgeStore` rows (entity + relationship) carrying `Provenance`/`EvidenceRef`,
  `provenance_for`/`evidence_for` recover them; `provenance_by_source` filters by source; a disjoint
  `observed_at` window returns empty. (AC1, AC2)
- Scope isolation: a query in tenant A does not return tenant B's evidence. (AC1)
- Unsupported target kinds return `CapabilityUnsupported`. (AC1)

**Approach:**
- Add `adapters/integration/src/provenance.rs` with `SqlProvenanceQuery { knowledge: Arc<SqlKnowledgeStore> }`.
  Implement the four ops over entity/relationship/source via the existing scope-column listings +
  Rust-side filtering of deserialized `record_json`; short-circuit other target kinds to `CapabilityUnsupported`.

**Done when:** impl tests green; `core/integration` still passes the neutrality gate (+ AC7).

### T3: Provider handle + capability flip

**Depends on:** T2 · **Mode:** TDD

**Tests:**
- `EngramProvider` exposes `provenance() -> Option<&Arc<dyn ProvenanceQuery>>`. (AC3)
- A provider with `SqlProvenanceQuery` attached reports `episodes_evidence` `Supported`; an unwired
  provider reports it `Unsupported { FeatureDisabled }` with no handle. (AC3)

**Approach:**
- Add the `provenance` field + builder method + accessor to `EngramProvider`/`EngramProviderBuilder`
  (`core/integration/src/provider.rs`), mirroring existing handles.
- In `adapters/integration/src/wiring.rs::bootstrap_provider`, construct `SqlProvenanceQuery`, run the
  T4 fixture, attach the handle, and mark `episodes_evidence` `Supported` only on pass.

**Done when:** handle + flip tests green (+ AC7).

### T4: Conformance fixture + deferred-write documentation

**Depends on:** T3 · **Mode:** goal-based check

**Tests:**
- A conformance fixture writes entity/relationship/source records carrying `Provenance`/`EvidenceRef`,
  queries them through the wired provider handle, and asserts recovery; the capability is `Supported`
  only when it passes. (AC4)
- `core/integration/src/{provider,capability,provenance}.rs` pass `.codex/hooks/check-engine-neutrality.sh`. (AC5)

**Approach:**
- Add the fixture alongside existing conformance fixtures in `adapters/integration`.
- Add the deferred write-side entry to `docs/backlog.md` (`episode-evidence-write-side`).

**Done when:** fixture green; backlog entry present; neutrality gate green (+ AC7).

## Rollout

- **Delivery:** additive Rust API (one new port trait + one domain-enum extension + handle + capability
  flip) + one conformance fixture. No flag, no migration, fully reversible. Nothing irreversible.
- **Infrastructure:** none.
- **External-system integration:** none.
- **Deployment sequencing:** T1→T2→T3→T4 strictly (each consumes the prior). Per-task regression bar
  (AC7): existing workspace tests stay green at every task.

## Risks

- **Port/impl boundary leakage.** The impl composes `Sql*` stores; an accidental engine reference in
  the port trips the neutrality gate (now covering `provenance.rs`). Mitigation: gate green per task.
- **v1 target coverage.** v1 is entity/relationship/source only; the rest return `CapabilityUnsupported`
  (honest, not silent). Wiring memory/belief/document/chunk is an `Ask first` follow-up.
- **Query cost at scale.** Rust-side filtering over `record_json` is fine at demo scale; an indexed
  evidence table is an `Ask first` follow-up, not silently absorbed.

## Changelog

- 2026-07-10: initial plan (S2 of engram-host-sdk brief; conforms to ADR-0022 + reference.md; read-only per user, write-side deferred).
- 2026-07-10: spec-mode review fixes — extend `EvidenceTargetType` with Relationship/Belief (Major 1); commit v1 = entity/relationship/source, rest `CapabilityUnsupported` (Major 2/4); add `core/integration/src/provenance.rs` to the neutrality gate (Major 3); name `ProvenanceEntry` DTO + `observed_at` window (Minor 5/6); drop the compiler-mirroring shape test (Minor 8); note `MemoryContent.provenance` Option (Minor 9); AC7 per-task regression bar (Minor 11).
- 2026-07-10: implementation review fixes — renamed `provenance_by_source`'s param `source_id` → `stable_source_key` (Blocker: it filtered by the graph's stableSourceKey, not the KnowledgeSource.id, so real callers got silent empties; the fixture had masked it); `evidence_by_scope` now includes Source records (a v1-supported target) so the scope-wide query is not silently incomplete; `episodes_evidence` now reports `ConformanceFailed` (not `FeatureDisabled`) on fixture failure, matching the other implemented families; spec flipped to Shipped with ACs ticked. Deferred (nit): the fixture's assertion-failure carrier uses `CoreError::Conflict` internally (it is wrapped to `Adapter` before return, so the external behavior is correct) — cosmetic follow-up.
