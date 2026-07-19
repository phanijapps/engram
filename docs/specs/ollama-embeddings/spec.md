# Spec: Ollama Embeddings Integration

- **Status:** Draft
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** none
- **Brief:** none
- **Contract:** none
- **Shape:** integration

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

<!-- **Light-mode lean fill.** For low-risk work running the `work-loop`
skill's light mode, only Objective + Acceptance Criteria + a short task list
(in `plan.md`) are required. **Boundaries**, **Testing Strategy**, and
**Assumptions** are optional — keep them only if they earn their place. Any
risk trigger (see the `work-loop` skill) escalates to full mode, where every
section is filled. -->

<!-- **Present tense, as-built.** Write every body section below as if the
feature already exists and always worked this way — no "will be", no
"previously X, now Y", no deprecation timelines, no version-stamped history.
The body describes the current contract; decision history lives in ADRs and the
changelog. This applies to the spec body only — `plan.md` keeps its own
changelog of how the approach evolved. -->

## Objective

Engram accepts pre-computed embeddings, model metadata, and dimensions from the caller via Ollama and uses them for vector search without managing the embedding provider. Callers provide their Ollama endpoint URL, embedding vectors, and model dimensions; Engram stores embeddings with metadata and retrieves them via sqlite-vec similarity search. Errors from embedding operations propagate back to the caller without fallback logic.

## Boundaries

The three-tier guard that keeps an implementing agent inside the lines.
*Always do* applies without asking; *Ask first* requires human sign-off
before proceeding; *Never do* is a hard rule, even under time pressure.

### Always do

- Propagate embedding operation errors back to the caller without silent fallback
- Store model metadata (model name, dimensions) alongside embeddings for retrieval
- Validate embedding dimensions match the declared dimensions before storage
- Accept embeddings as raw float arrays from the caller
- Use existing VectorQueryProvider trait for query embeddings interface

### Ask first

- Adding batch embedding support beyond single-query operations
- Changing sqlite-vec fixed-dimension constraint per table
- Adding automatic embedding dimension detection from Ollama API

### Never do

- Implement Ollama HTTP client or provider management inside Engram
- Store Ollama API credentials or manage provider lifecycle
- Add fallback embedding providers or automatic retry logic
- Mix Ollama-specific logic with existing FastEmbed implementation
- Create embedding provider selection or routing logic in core

## Testing Strategy

Name the verification mode(s) this spec uses. The
`work-loop` skill defines three:

- **TDD** — for logic with a compressible invariant.
- **Goal-based check** — a one-liner verifies the outcome (a build
  command, a `grep`, a typecheck).
- **Visual / manual QA** — a recorded gesture and an observable
  outcome, for UX flows.

A spec may pick one or mix them. State which mode each behavior falls
under, and why. These three modes are the *altitude* of a check, not its
*surface*: a goal-based or manual-QA behavior may be verified by an
**integration** test (two components together) or an **end-to-end (E2E)**
test (the whole journey, as the user drives it) rather than a unit test —
name that surface when a behavior only proves out across a boundary or a
full flow.

Validation rules for embedding dimension matching: TDD (compressible invariant).
Error propagation from embedding operations: TDD (error paths are invariant).
Metadata storage and retrieval: goal-based check, exercised by integration test.
Vector search with stored embeddings: goal-based check, exercised by integration test.
HTTP client dependency injection: goal-based check (typecheck).

## Acceptance Criteria

- [ ] Given a caller-provided embedding vector with model metadata, when the embedding is stored, the vector includes model name and dimensions in sqlite-vec metadata field
- [ ] Given an embedding with mismatched dimensions (vector length ≠ declared dimensions), when storage is attempted, the operation returns a dimension mismatch error without storing the vector
- [ ] Given a query embedding from the caller, when vector search is executed, sqlite-vec returns results ordered by cosine similarity using the stored vectors
- [ ] Given an HTTP client injection point for Ollama calls, when the caller provides a client instance, the framework uses it for embedding operations without creating its own client
- [ ] Given any embedding operation failure (HTTP error, timeout, invalid response), when the error occurs, the framework propagates the error back to the caller without retry or fallback

## Assumptions

- Technical: Current embedding architecture uses FastEmbed BGE-small (384 dims) feature-gated in adapters/retrieval/sqlite-vec (source: adapters/retrieval/sqlite-vec/Cargo.toml lines 8-10)
- Technical: VectorQueryProvider trait defines query_vector() interface for query embeddings (source: core/retrieval/src/ports.rs lines 22-28)
- Technical: sqlite-vec requires fixed dimensions per vector table at creation time (source: adapters/retrieval/sqlite-vec/src/index.rs lines 21, 78)
- Technical: No HTTP client infrastructure exists in the workspace (source: user confirmation 2026-07-05)
- Technical: No batch embedding support exists currently (source: user confirmation 2026-07-05)
- Product: Ollama integration is framework-level where caller provides URL, embeddings, and dimensions (source: user confirmation 2026-07-05)
- Product: No fallback provider needed - caller manages all provider interactions (source: user confirmation 2026-07-05)

If an assumption later turns out wrong, fix the spec body in the same
PR and add a one-line note here recording what changed and why.
-->
