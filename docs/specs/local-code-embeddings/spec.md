# Spec: Local code embeddings

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [plan.md](plan.md)
- **Constrained by:** ADR-0022, RFC-0021 (local retrieval embedding clarification)
- **Contract:** contracts/v1/schemas/engram-v1.schema.json
- **Shape:** integration

## Objective

With vectors enabled, an operator indexes semantic code chunks using the local
embedding provider and retrieves actual code through vector search. Exact
source snapshots remain source-read artifacts. Rust SDK, N-API/TypeScript,
stdio MCP and HTTP MCP share backfill behavior and report readiness/progress.

## Boundaries

### Always do

- Check exact source scope, retrieval policy, expiry, active generation and
  canonical chunk availability before local inference/reuse and insertion.
- Preserve bounded batching, stable IDs, eligible-only hash deduplication and
  source authorization in retrieval. Provider locality defaults external;
  the existing in-process adapter explicitly declares local execution.

### Ask first

- A new model, external source transmission or LLM summaries need separate
  authorization. Wiping canonical records or another project's index is
  outside the authorized self-store vector rebuild.

### Never do

- Embed/reuse snapshots, denied/inactive/stale source or legacy code without
  a validated generation. Do not infer locality from a provider-name string.
- Reclassify code as externally model-eligible, remove external extraction
  guards, add provider/storage dependencies or implement embedding logic in TS.

## Testing Strategy

TDD integration fixtures with recording local/external models and deterministic
vectors prove selection, source checks, generation changes, dedup and failures.
Goal-based gates prove additive generated DTOs, capability readiness and surface
parity. Manual QA exercises real local FastEmbed code-vector hits in a code-only
fixture and the rebuilt self store, with canonical data unchanged by reset.

## Acceptance Criteria

- [x] **AC1:** Local backfill embeds symbol/context/residual units and ordinary
  documents; snapshots and legacy-unknown code are never model/reuse candidates.
- [x] **AC2:** Private/expired/denied/wrong-scope and inactive code is excluded;
  a generation/policy transition during inference prevents vector insertion.
- [x] **AC3:** External/unknown providers receive no code. External LLM feeds
  remain unchanged. Local execution is an explicit provider contract.
- [x] **AC4:** Rust, native TS, stdio scan/reindex and HTTP maintenance share
  backfill, with generated DTOs and code_embeddings capability readiness based
  on actual local provider/vector/query/source prerequisites.
- [x] **AC5:** Repeats perform no inference; bounded deterministic selection
  reports new/reused/code/pending counts. Errors surface and retry converges.
- [x] **AC6:** Real vector search returns actual code in a code-only fixture
  and the rebuilt self store. Vector reset preserves canonical records and
  temporal baseline; repeat backfill drains to zero pending.

## Assumptions

- Technical: FastEmbed is already default and its local model is cached
  (MCP Cargo.toml and successful live vector verification).
- Technical: code uses the existing chunk/vector representation; blanket
  model_eligible gating blocks local embedding (knowledge_query.rs/SQLite/MCP).
- Product: local code embeddings and self-vector reset are explicitly
  authorized by the user's 2026-10-14 instructions; no new model is selected.
- Process: full work-loop and neutral/surface-parity gates apply (AGENTS.md).

## Verification evidence

[Live verification](../../research/local-code-embedding-verification.md) records
all six criteria, all transports, the vector-only reset and the retained self store.

## Compatibility

The operation DTOs and capability key are additive v1 wire changes. Direct
Rust CapabilityReport struct literals need the new field (builders handle it);
custom TypeScript NativeProviderTransport implementations need the new method.
Those implementation surfaces require source updates and the native addon must
be rebuilt. Existing provider trait implementations retain the nonlocal default.
