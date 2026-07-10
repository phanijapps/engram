# Plan: Atomic (best-effort) batch ingest API (S3)

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn.

## Approach

S3 adds a backend-neutral `BatchIngest` port + provider handle that writes a
semantic batch across the separate SQLite stores **best-effort** (no cross-store
ACID — infeasible without a forbidden storage restructure), reporting a per-step
outcome. Four sequential moves:

1. **Port + DTOs.** `BatchIngest` + `BatchIngestRequest`/`BatchOutcome`/`StepOutcome`/
   `TransactionGuarantee` in `core/integration/src/batch.rs`. `transaction_guarantee()`
   returns `BestEffort`.
2. **SQLite impl.** `SqlBatchIngest` in `adapters/integration/src/batch.rs`, composing
   the memory + knowledge store handles. Writes the four writable steps in fixed order
   via existing per-store puts; Evidence + Embeddings steps report `Skipped` in v1
   (evidence is embedded in record provenance; vector storage is a VectorIndex
   follow-up). Continues past failures; aggregates the per-step outcome.
3. **Provider + capability.** `EngramProvider` gains a `batch: Option<Arc<dyn BatchIngest>>`
   handle + `batch()` accessor; `bootstrap_provider` constructs `SqlBatchIngest`, runs
   the fixture, flips `atomic_batch` to `Supported` on pass.
4. **Conformance fixture + gate.** Ingests a full batch, re-ingests for dedup, forces a
   partial failure; adds `core/integration/src/batch.rs` to the neutrality gate.

Strictly sequential (T1→T2→T3→T4). Riskiest part is the partial-failure aggregation
+ keeping the port engine-neutral while the impl composes `Sql*` stores — mitigated by
the gate (covering `batch.rs` after T1).

## Constraints

- **ADR-0022** — the `BatchIngest` port (`core/integration/src/batch.rs`) must not name
  an engine type or hold SQL; the SQLite impl lives in `adapters/integration`.
- **`rust-crate-integration` (Implementing)** — owns the facade; S3 extends it with one
  new handle + capability flip, additive only.
- **S1** — owns the `atomic_batch` capability key (`Unsupported { FeatureDisabled }`); S3 flips it.
- **`docs/architecture/reference.md`** — typed errors (`CoreResult`); the read path is
  backend-agnostic behind ports (ADR-0009). Per-store writes already carry their own
  transactions (memory `transactional_write`; knowledge graph cascade).
- **No storage restructure** — separate SQLite files stay separate; no ATTACH, no merge.
- **Honesty** — never claim ACID; `TransactionGuarantee::BestEffort` is surfaced.

## Construction tests

**Integration tests:** the conformance fixture (T4) is the cross-cutting integration
test — full batch lands + Complete; re-ingest → Deduplicated; forced partial failure →
Partial + per-step error; `transaction_guarantee() == BestEffort`. Per-task unit tests
cover the port contract (T1) and the per-step aggregation (T2).

**Manual verification:** from a bootstrapped provider, ingest a batch and confirm every
writable record landed and the outcome is Complete with `guarantee == BestEffort`.

## Design (LLD)

Conforms to `docs/architecture/reference.md`. `Shape: service` →
`Interfaces & contracts`, `Data & schema`, `Failure, edge cases & resilience`.

### Design decisions

- **Best-effort, not ACID.** Separate SQLite files cannot share a transaction; the brief
  permits partial-failure reporting. `TransactionGuarantee::BestEffort` is surfaced, never
  implied atomic. Traces to: AC1, AC3.
- **Fixed step order, continue-on-failure.** Episode → Facts → Entities → Relationships →
  Evidence → Embeddings. A failed step is recorded and the batch proceeds; no rollback of
  succeeded steps (none exists cross-store). Traces to: AC2, AC3.
- **Four writable steps + two Skipped in v1.** Episode/Facts/Entities/Relationships write
  via existing per-store puts. Evidence is embedded in record `Provenance.evidence`
  (callers put it on the records); Embeddings (`EmbeddingRef`) is metadata whose vector
  storage is a VectorIndex follow-up. Both report `Skipped` in v1 — honest, not silent.
  Traces to: AC2, AC3.
- **Idempotency via per-store semantics.** The batch `idempotency_key` routes to memory's
  key-based idempotency; knowledge upserts by record id; re-ingest reports `Deduplicated`
  where the store dedupes. No cross-store batch-key ledger (an `Ask first` follow-up).
  Traces to: AC1, AC3.
- **Port in `core/integration`, impl in `adapters/integration`.** Facade-level trait; the
  `Sql*`-composing impl is engine-specific, gated by ADR-0022. Traces to: AC1, AC5.

### Interfaces & contracts

`core/integration/src/batch.rs`:

- `TransactionGuarantee { BestEffort, Atomic }` (Atomic reserved for a future single-connection backend).
- `BatchStep { Episode, Facts, Entities, Relationships, Evidence, Embeddings }`.
- `StepStatus { Succeeded, Deduplicated, Skipped, Failed }`.
- `StepOutcome { step: BatchStep, status: StepStatus, error: Option<CoreError> }` (typed, not stringly).
- `BatchStatus { Complete, Partial }`.
- `BatchOutcome { guarantee: TransactionGuarantee, status: BatchStatus, steps: Vec<StepOutcome> }`.
- `BatchIngestRequest { idempotency_key: String, scope: Scope, source: Option<KnowledgeSource>, documents: Vec<SourceDocument>, chunks: Vec<KnowledgeChunk>, facts: Vec<MemoryRecord>, entities: Vec<KnowledgeEntity>, relationships: Vec<KnowledgeRelationship>, evidence: Vec<EvidenceRef>, embeddings: Vec<EmbeddingRef> }`.
- `BatchIngest` trait: `fn transaction_guarantee(&self) -> TransactionGuarantee;` + `async fn ingest(&self, request: BatchIngestRequest) -> CoreResult<BatchOutcome>;`.

Traces to: AC1, AC2.

### Data & schema

No new table, no migration. The impl calls existing per-store writes:
- Episode → `put_source` / `put_document` / `put_chunk` (knowledge).
- Facts → memory write per `MemoryRecord` (memory's `transactional_write`). **Derive a per-record idempotency key `{batch_key}#{index}`** for each memory record — memory's idempotency lookup is `(tenant, subject, workspace, idempotency_key)` with no per-record disambiguation, so reusing the batch key would dedupe records 2..N against record 1 (data loss). The derived keys are deterministic, so re-ingest reproduces them and dedupes correctly.
- Entities → `put_entity`; Relationships → `put_relationship` (knowledge). These upsert by record id (no key-based dedup) — re-ingest overwrites and reports `Succeeded`, NOT `Deduplicated`.
- Evidence / Embeddings → no v1 write (`Skipped`). Traces to: AC3, AC7.

### Failure, edge cases & resilience

- Best-effort: a step failure does NOT abort the batch; it is recorded and the batch continues. Overall status is `Partial` if any step `Failed`, else `Complete`.
- `Skipped` steps (Evidence, Embeddings in v1) do not make the batch `Partial`.
- Empty slices are fine — a step with an empty payload reports `Succeeded` (nothing to write).
- The outcome's `guarantee` is always `BestEffort` for the SQLite impl, so a caller never mistakes the result for atomic.

## Tasks

### T1: BatchIngest port + DTOs

**Depends on:** none · **Mode:** TDD

**Tests:**
- An in-memory stub `BatchIngest` returns `transaction_guarantee() == BestEffort` and, given a request, produces a `BatchOutcome` with one `StepOutcome` per step in fixed order; all-succeed → `Complete`; one failed → `Partial`. (AC1, AC2)

**Approach:**
- Add `core/integration/src/batch.rs` with the DTOs + `BatchIngest` trait; re-export from `lib.rs`.
- Add `core/integration/src/batch.rs` to `.codex/hooks/check-engine-neutrality.sh` `GATED_PATHS`.

**Done when:** port compiles, re-exported, stub test green, gate green (+ AC7).

### T2: SqlBatchIngest impl

**Depends on:** T1 · **Mode:** TDD

**Tests:**
- A well-formed batch against in-memory stores lands source/document/chunk + memory + entities + relationships and reports `Complete` with Evidence/Embeddings `Skipped`.
- A batch with **N=3 distinct memory records under one batch key** lands all three (none deduplicated away) — guards the `{batch_key}#{index}` per-record-key derivation against memory's no-per-record-disambiguation lookup.
- Re-ingesting the same batch reports the **Facts step `Deduplicated`** (memory key dedup) and the **knowledge steps `Succeeded`** (upsert overwrote, not Deduplicated); overall `Complete`.
- A batch with a failing step (e.g. a relationship put that errors) reports that step `Failed` with its typed error, the other steps still land, and overall is `Partial`. (AC2, AC3)

**Approach:**
- Add `adapters/integration/src/batch.rs` with `SqlBatchIngest { memory: Arc<SqlMemoryService>, knowledge: Arc<SqlKnowledgeStore> }`. `ingest` runs the six steps in order, maps each per-store result to a `StepOutcome` (Ok→Succeeded/Deduplicated, Err→Failed), continues on error, aggregates `BatchOutcome { guarantee: BestEffort, status, steps }`.

**Done when:** impl tests green; `core/integration` still passes the neutrality gate (+ AC7).

### T3: Provider handle + capability flip

**Depends on:** T2 · **Mode:** TDD

**Tests:**
- `EngramProvider` exposes `batch() -> Option<&Arc<dyn BatchIngest>>`. (AC4)
- A provider with `SqlBatchIngest` attached reports `atomic_batch` `Supported` and the handle's `transaction_guarantee()` is `BestEffort`; an unwired provider reports it `Unsupported { FeatureDisabled }` with no handle. (AC4)

**Approach:**
- Add the `batch` field + builder method + accessor to `EngramProvider`/`EngramProviderBuilder` (`core/integration/src/provider.rs`), mirroring existing handles.
- In `bootstrap_provider`, construct `SqlBatchIngest`, gate the handle + capability flip on `fixtures::batch::run_batch_fixture().is_ok()`, and mark `atomic_batch` `Supported` only on pass (start at `failed()`/ConformanceFailed like other families). T4 supplies the `run_batch_fixture` body; T3 and T4 land together in one PR (the call site in T3 resolves once T4's function exists — no transient non-conformance flip ships).

**Done when:** handle + flip tests green (+ AC7).

### T4: Conformance fixture + gate

**Depends on:** T3 · **Mode:** goal-based check

**Tests:**
- A fixture ingests a full batch (all writable slices) → recovers every record → `Complete`; re-ingests → `Deduplicated`; forces a partial failure → `Partial` + per-step error; asserts `transaction_guarantee() == BestEffort`. (AC5)
- `core/integration/src/{provider,capability,provenance,batch}.rs` pass `.codex/hooks/check-engine-neutrality.sh`. (AC6)

**Approach:**
- Add the fixture alongside existing conformance fixtures in `adapters/integration`.

**Done when:** fixture green; neutrality gate green (+ AC7).

## Rollout

- **Delivery:** additive Rust API (one new port trait + DTOs + handle + capability flip) + one conformance fixture. No flag, no migration, fully reversible.
- **Deployment sequencing:** T1→T2→T3→T4 strictly. Per-task regression bar (AC7).

## Risks

- **Port/impl boundary leakage.** The impl composes `Sql*` stores; an accidental engine reference in the port trips the gate (covering `batch.rs` after T1).
- **Overclaimed atomicity.** The greatest honesty risk; mitigated by `TransactionGuarantee::BestEffort` on the port + every outcome, and `Skipped` (not silent) for unwired steps.
- **Idempotency nuance.** v1 relies on per-store idempotency (memory key, knowledge upsert), not a cross-store ledger; re-ingest may report `Succeeded` (upsert overwrote) rather than `Deduplicated` on stores without key-based idempotency — acceptable + documented; a batch-key ledger is an `Ask first` follow-up.

## Changelog

- 2026-07-10: initial plan (S3 of engram-host-sdk brief; best-effort per user confirmation; conforms to ADR-0022 + reference.md).
- 2026-07-10: spec-mode review fixes — derive a per-record idempotency key `{batch_key}#{index}` for the Facts step (Critical: reusing one batch key as the per-write memory key silently dropped records 2..N — memory's lookup has no per-record disambiguation); declare Evidence/Embeddings `Skipped` in v1 in the Objective + an AC (honesty: spec no longer overclaims those writes); `StepStatus` made four-valued (`Skipped` added) and `StepOutcome.error` typed to `Option<CoreError>` (no stringly contract); `Deduplicated` narrowed to the Facts step (knowledge steps report `Succeeded` on re-ingest — upsert overwrote); added the N=3-distinct-memories test; removed the T3→T4 fixture forward-reference (co-resident in one PR); added deferred AC `atomic-batch-evidence-embeddings` (implementing PR must create that `docs/backlog.md` anchor).
