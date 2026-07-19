# Spec: Atomic (best-effort) batch ingest API (S3)

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0022 (engine neutrality), [`rust-crate-integration`](../rust-crate-integration/spec.md) (the Implementing facade S3 extends), [`provider-sdk-capability-report`](../provider-sdk-capability-report/spec.md) (S1 — owns the `atomic_batch` capability key S3 flips)
- **Brief:** [`docs/product/briefs/engram-host-sdk.md`](../../product/briefs/engram-host-sdk.md) (slice S3, capability #10)
- **Contract:** none — a Rust port trait (`BatchIngest`) + DTOs + provider handle.
- **Shape:** service

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

A host writes a **semantic batch** to the relevant stores through one backend-neutral
operation on `EngramProvider`, carrying one batch idempotency key. The batch carries an
episode (optional `source` + `documents` + `chunks`), facts (memory records), graph
entities, graph relationships, evidence links (`EvidenceRef`), and embedding references
(`EmbeddingRef`). Because the SQLite stores live in **separate files** with no
cross-store transaction, the operation is **best-effort, not ACID**: each step writes in
its own per-store transaction, in a fixed order, and on partial failure the host receives
a per-step `BatchOutcome` naming exactly which steps `Succeeded`, were `Deduplicated`,
were `Skipped`, or `Failed` (with a typed error).

**v1 writes four steps** — Episode, Facts, Entities, Relationships — via the existing
per-store repository writes. The Evidence and Embeddings steps are **`Skipped` in v1**:
evidence is embedded in the records' `Provenance.evidence` (callers put it on the records
they pass), and `EmbeddingRef` is metadata whose vector storage is a VectorIndex
follow-up. Both are reported `Skipped` — honest, not silent.

The operation's guarantee is surfaced explicitly as `TransactionGuarantee::BestEffort` —
never overclaimed as atomic. The `atomic_batch` capability, reported
`Unsupported { FeatureDisabled }` today, becomes `Supported` behind a `BatchIngest`
provider handle wired through the SQLite backend.

## Boundaries

The three-tier guard that keeps an implementing agent inside the lines.
*Always do* applies without asking; *Ask first* requires human sign-off
before proceeding; *Never do* is a hard rule, even under time pressure.

### Always do

- Write the batch step-by-step in a fixed order (Episode → Facts → Entities → Relationships → Evidence → Embeddings), each writable step via the existing per-store repository writes.
- Derive a **per-record idempotency key** from the batch key for the Facts step (`{batch_key}#{index}`) so N distinct memory records all land on first ingest and re-ingest reproduces the same N keys (memory's idempotency lookup is `(tenant, subject, workspace, idempotency_key)` with no per-record disambiguation, so a single reused key would silently drop records 2..N).
- Report `Deduplicated` only where a store key-dedupes (the Facts step, memory); knowledge steps report `Succeeded` on re-ingest because upsert overwrote — do not promise `Deduplicated` for upserts.
- Continue on a step failure (best-effort): record the failed step + its typed error and proceed to the remaining steps; never abort the whole batch silently.
- Report Evidence and Embeddings steps as `Skipped` in v1 (not silently absent).
- Surface `TransactionGuarantee::BestEffort` on the port and on every outcome.
- Keep the `BatchIngest` port in `core/integration` (engine-neutral); put the SQLite impl in `adapters/integration`.
- Flip `atomic_batch` to `Supported` only when the conformance fixture passes; attach the handle only then.

### Ask first

- Promote the guarantee toward true atomicity (requires a single ATTACH'd connection — a storage restructure ADR-0022 forbids).
- Wire the Evidence / Embeddings steps to real writes (evidence is embedded today; embeddings need VectorIndex wiring).
- Add cross-store idempotency tracking (a batch-key → landed-records ledger) beyond per-store semantics.
- Promote `BatchIngest` out of `core/integration` into its own behavior crate.

### Never do

- Claim or imply cross-store ACID — the guarantee is `BestEffort`, surfaced. *(honesty)*
- Reuse one batch key as the per-write memory key (drops records 2..N). *(correctness)*
- Name an engine type (`Sql*`, …) or hold SQL in the `BatchIngest` port or `core/integration`. *(structural, ADR-0022)*
- Bypass the existing per-store repository writes (re-implementing SQL inserts) — the batch composes the ports. *(structural)*
- Restructure storage (merge files / ATTACH) to fake atomicity.
- Roll back already-succeeded steps on a later step's failure (no cross-store rollback exists; that would be a false claim of atomicity).

## Testing Strategy

- **BatchIngest port + DTOs — TDD.** `transaction_guarantee()` returns `BestEffort`; an in-memory stub maps each payload slice to a `StepOutcome` and aggregates a `BatchOutcome` (Complete when no step Failed; Partial when any Failed; Skipped steps do not make it Partial). The invariant is compressible: "the outcome reports one typed status per step, in order, and an honest overall status."
- **SqlBatchIngest impl — TDD.** Against in-memory SQLite stores: a well-formed batch lands every writable record (Episode/Facts/Entities/Relationships `Succeeded`, Evidence/Embeddings `Skipped`) and reports Complete; a batch with N=3 distinct memory records under one batch key lands all three (none deduplicated away — guards the per-record-key derivation); re-ingest reports the Facts step `Deduplicated` and knowledge steps `Succeeded`; a batch with a failing step reports that step `Failed` with its typed error while the other steps still land (Partial).
- **Provider handle + capability flip — TDD.** A bootstrapped provider exposes `batch()` and reports `atomic_batch` `Supported`; an unwired provider reports it `Unsupported` with no handle.
- **Conformance fixture — goal-based.** Full batch → Complete (Evidence/Embeddings Skipped); re-ingest → Complete with the Facts step Deduplicated; forced partial failure → Partial + per-step typed error; `transaction_guarantee() == BestEffort`.
- **Engine neutrality — goal-based check.** `core/integration/src/batch.rs` stays green under `.codex/hooks/check-engine-neutrality.sh` (added to `GATED_PATHS`).
- **No regression — goal-based check.** Existing workspace tests stay green; no schema change.

## Acceptance Criteria

- [x] The `BatchIngest` port in `core/integration/src/batch.rs` exposes `transaction_guarantee() -> TransactionGuarantee` (returning `BestEffort` for the SQLite impl) and `async fn ingest(BatchIngestRequest) -> CoreResult<BatchOutcome>`. The request carries one `idempotency_key` + `Scope` + optional `source: Option<KnowledgeSource>`, `documents: Vec<SourceDocument>`, `chunks: Vec<KnowledgeChunk>`, `facts: Vec<MemoryRecord>`, `entities: Vec<KnowledgeEntity>`, `relationships: Vec<KnowledgeRelationship>`, `evidence: Vec<EvidenceRef>`, `embeddings: Vec<EmbeddingRef>` — each optional/empty-allowed.
- [x] `BatchOutcome` reports a per-step `StepOutcome { step: BatchStep, status: StepStatus, error: Option<CoreError> }` for every step, in the fixed step order, plus an overall `status: BatchStatus`. `StepStatus = Succeeded | Deduplicated | Skipped | Failed` (`Skipped` for Evidence/Embeddings in v1 and for empty writable payloads that trivially succeed; `Deduplicated` only for key-deduped stores). `BatchStatus = Complete | Partial`, where **Partial iff any step is Failed** (Skipped/Deduplicated do not make it Partial). The error is typed (`CoreError`), not a string.
- [x] A SQLite `BatchIngest` implementation writes Episode/Facts/Entities/Relationships via the existing per-store repository writes (memory `transactional_write` with a **per-record key `{batch_key}#{index}`**; knowledge `put_*`), continues past failures, and reports Evidence/Embeddings as `Skipped`; no schema change, no storage restructure, no rollback of succeeded steps. A batch with N distinct memory records under one batch key lands all N (none dropped).
- [x] `EngramProvider` exposes a `batch()` handle; the `atomic_batch` capability flips to `Supported` only when the conformance fixture passes, with `transaction_guarantee() == BestEffort` visible through the handle.
- [x] A conformance fixture ingests a full batch (all six steps, Evidence/Embeddings `Skipped`) → recovers the writable records → `Complete`; re-ingests → `Complete` with the Facts step `Deduplicated`; forces a partial failure → `Partial` + per-step typed error; asserts `transaction_guarantee() == BestEffort`.
- [x] `.codex/hooks/check-engine-neutrality.sh` covers `core/integration/src/batch.rs` (added to `GATED_PATHS`); the port layer is engine-symbol-free.
- [x] v1 reports `Skipped` for the Evidence and Embeddings steps. Evidence is embedded in record provenance (lands via the existing per-store writes; attaching evidence to records *not* in the batch is the ADR-0023 `ProvenanceQuery::attach_evidence` write op, not a batch step). Embeddings stay `Skipped` per ADR-0024 — the deferred-reindex model: the batch records `EmbeddingRef` metadata via `Provenance` and actual vectors are generated by a separate reindex job via `VectorIndex`. (was deferred: `atomic-batch-evidence-embeddings`)
- [x] SQLite behavior for existing operations is unchanged; existing workspace tests green.

## Assumptions

- Technical: within-store transactions exist (memory `transactional_write`; knowledge graph cascade `graph.rs:122`) but no cross-store transaction (source: `adapters/{memory,knowledge}/sqlite/src`).
- Technical: storage is separate SQLite files (`SqliteLayoutPaths { memory, knowledge, belief, hierarchy }`) → true cross-store ACID impossible without ATTACH/restructuring, which ADR-0022 + "SQLite untouched" forbid (source: `adapters/integration/src/wiring.rs:43`).
- Technical: per-write idempotency exists — memory's lookup is `(tenant, subject, workspace, idempotency_key)` with NO per-record disambiguation (`adapters/memory/sqlite/src/{write,transactional_write}.rs`), so a batch must derive a per-record key; belief upsert is idempotent by scope/subject/valid_from; knowledge upserts by record id (no key-based dedup) (source: `core/domain/src/operations.rs`, `adapters/{memory,orchestration}/sqlite`).
- Technical: every batch-payload domain type exists — `MemoryRecord`, `KnowledgeSource`/`SourceDocument`/`KnowledgeChunk`, `KnowledgeEntity`/`KnowledgeRelationship`, `EvidenceRef`, `EmbeddingRef` (source: `core/domain/src/{memory,knowledge}.rs`).
- Technical: no batch/multi-record composition exists today; `EngramProvider` has the `atomic_batch` capability key (`Unsupported { FeatureDisabled }`) but no handle (source: `core/integration/src/provider.rs`).
- Product: S3 is a **best-effort batch** with per-step partial-failure reporting — explicitly NOT cross-store ACID; Evidence/Embeddings are `Skipped` in v1 (evidence embedded in records; embeddings a VectorIndex follow-up). (source: user confirmation 2026-07-10)
- Design: payload {source/documents/chunks, facts, entities, relationships, evidence, embeddings} each optional + one batch `idempotency_key`; per-record key `{batch_key}#{index}` for Facts; `BatchOutcome` per-step {Succeeded|Deduplicated|Skipped|Failed} with typed error + overall {Complete|Partial}; `atomic_batch` Supported with `TransactionGuarantee::BestEffort` surfaced. (source: user confirmation 2026-07-10 + spec review 2026-07-10)
- Process: SQLite only; the port stays engine-neutral (ADR-0022); additive only; typed errors (no stringly public contracts); reuse existing per-store repository writes.
