# Plan: Local code embeddings

- **Spec:** [spec.md](spec.md)
- **Status:** Done

## Approach

Extract MCP embedding backfill into a focused engine-neutral integration
operation. Explicit local execution enables policy-authorized semantic code;
model_eligible continues to govern external processing. All transports delegate.
Tempted to add a generic enrichment ticket framework; declining because this
is local retrieval inference. Tempted to add AST token-segment storage;
declining because that is a separate RFC slice.

## Constraints

ADR-0022 neutrality/parity; RFC-0021 snapshots excluded and external enrichment
separately authorized. User authorization covers local code and self-vector reset.

## Construction tests

Existing source-selection fixtures seed active units and a same-hash snapshot.
Recording model/vector tests verify selection, policy, races and reuse.
Real binary/addon plus code-only and self stores prove user-visible vector hits.

## Design (LLD)

### Interfaces & contracts

`EmbeddingBackfillRequest`: scope, requester, limit (default 256; 1..1024),
optional sourceFilter. `EmbeddingBackfillReport`: embedded, newEmbeddings,
reused, codeEmbedded, remaining. Async `EngramProvider::backfill_embeddings`
delegates to a focused module. N-API `backfillEmbeddingsJson` and TS
`backfillEmbeddings` use generated DTOs. HTTP resolves requester/scope from
trusted sourceContext, rejects caller overrides. MCP preserves progress text.

### Behavior & rules

`EmbeddingProvider::is_local()` defaults false; FastEmbed overrides true.
Code requires a non-snapshot semantic role, exact scope/generation/hash/
fingerprint and active manifest with source policy/requester authorization.
Check canonical chunks before model calls and current attempt/policy after
inference/reuse before insertion. Dedup twins independently pass eligibility.
Existing bounds remain 8,192 characters/input and 64 inputs/model batch; this
does not promise complete model-token coverage of long units. Empty hashes
are inferred directly, not silently skipped. Embedding-space mismatch fails.

### Failure, edge cases & resilience

Typed model/insertion errors propagate; completed inserts remain and stable-ID
retries converge. Stale code is skipped, never inserted as current. No
cross-store transaction or external model grant is implied.

## Tasks

### T1: Shared local backfill and deterministic fixtures pass

**Depends on:** none
**Verification mode:** TDD
**Tests:** AC1–AC3/AC5: local symbols/context/residual, snapshot twins, external
zero-code calls, source privacy/expiry/scope, generation transition during
inference, repeat zero calls, model failure/retry and readiness prerequisites.
**Approach:** Domain DTO/schema; focused integration module; locality trait;
provider method and additive code_embeddings capability computed from handles.
**Done when:** deterministic tests, generated contracts and neutrality pass.

### T2: Every supported transport invokes shared backfill

**Depends on:** T1 (published DTO contract permits transport preparation)
**Verification mode:** TDD and goal-based check
**Tests:** AC4/AC5: native TS forwarding and real addon operation; HTTP trusted
context/override rejection; MCP counts and source-filtered scan delta.
**Approach:** Thin N-API/native TS forwarding, HTTP maintenance reindex, MCP
delegation replacing duplicated embedding logic. No new model/provider.
**Done when:** native/runtime tests, TS build and surface parity pass.

### T3: Real code recall and self-vector rebuild pass

**Depends on:** T1, T2
**Verification mode:** Visual / manual QA
**Tests:** AC6: code-only vector-tagged chunk; source still exact; snapshot
exclusion; self reset canonical count/hash invariants; real self code hits;
unchanged repeat zero pending.
**Approach:** Release/native rebuild; existing Rust SqliteVectorIndex::clear
via a temporary operational helper on the self store, after an online backup.
Never delete co-located engram_data.db. Backfill and retain evidence.
**Done when:** actual artifact, required gates and independent reviews pass.

## Rollout

Existing vector enablement activates local code backfill. External processing
stays blocked. Rollback disables vectors or uses the prior binary; vectors
are rebuildable projections. Reset affects only self-store vector rows.

## Risks

Inference races require post-inference source checks. Canonical rows share the
SQLite file with vectors. Large code exceeds model input windows independently
of exact stored source coverage.

## Changelog

- 2026-10-14: authorized local code embedding; retain source and external guards.

## Verification outcome

T1–T3 passed. See [live evidence](../../research/local-code-embedding-verification.md)
and [independent review](reviews/2026-10-14.md). Full Rust1070/TS206 tests,
19 source fixtures, native/HTTP/stdio live code hits and self9316 code vectors
with zero snapshots/pending work were verified.
