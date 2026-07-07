# Plan: Ollama Embeddings Integration

- **Spec:** [`spec.md`](spec.md)
- **Status:** Drafting

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog
> at the bottom.

<!-- **Light-mode lean fill.** For low-risk work running the `work-loop`
skill's light mode, only Approach + a short Tasks list are required.
**Constraints**, **Risks**, **Changelog**, and the whole `## Design (LLD)`
section are optional — keep them only if they earn their place. Any risk
trigger (see the `work-loop` skill) escalates to full mode, where every
section is filled. -->

## Approach

This is an integration feature that adds HTTP client injection points and embedding metadata handling to the existing retrieval infrastructure. The work proceeds in three phases: (1) Add HTTP client dependency and injection interface, (2) Extend sqlite-vec schema to store model metadata with embeddings, (3) Implement dimension validation and error propagation paths. The riskiest part is the sqlite-vec schema extension since it requires understanding the fixed-dimension constraint and ensuring backward compatibility with existing FastEmbed embeddings. Testing strategy uses TDD for validation logic and integration tests for metadata storage/retrieval.

## Constraints

- AGENTS.md boundary rules: engram-domain must not depend on HTTP clients or async runtimes
- AGENTS.md boundary rules: engram-knowledge owns source ingestion ports, not memory writes
- AGENTS.md boundary rules: Store, vector, embedding, model integrations belong in adapter crates
- sqlite-vec fixed-dimension constraint: requires fixed dimensions per table at creation time
- No existing HTTP client infrastructure in workspace (adds new dependency)

## Construction tests

Most construction tests live under **Tasks** below (per-task `Tests:`
subsections). This top-level section is only for cross-cutting tests that
span tasks.

**Integration tests:**
- Store embeddings with model metadata and verify retrieval via sqlite-vec
- Execute vector search with stored embeddings and validate cosine similarity ordering
- Inject HTTP client mock and verify it's used for embedding operations without creating internal client

**Manual verification:** none beyond per-task tests

## Design (LLD)

The low-level design — the *how*, below the Approach and above the per-task
steps. **Optional and shape-pruned:** scaffold only the sub-sections the spec's
`Shape:` selects, and delete the rest. A one-file change keeps this section thin
or empty; a heavyweight feature fills most of it. The spec stays the contract —
**no acceptance criterion lives here**; each sub-section instead **traces to the
AC(s) it satisfies and the `contracts/` it implements**, so the design is always
anchored to something verifiable.

Stack-neutral by construction: these are the *kinds* of design decision every
build makes, never a framework. Name your actual stack *inside* each sub-section
— derived from `docs/architecture/reference.md` when that file is present (use
its components, stereotypes, and standards by name), otherwise from the
established repo (lockfiles, build files, imports) or elicited when unclear. The
headings themselves stay universal.

<!-- Shape → sub-sections (a guide, not a gate):
  ui          → decomposition, state & control flow, behavior & rules, quality attributes
  service     → interfaces & contracts, data & schema, failure & resilience, quality attributes
  data        → data & schema, interfaces & contracts
  integration → dependencies & integration, interfaces & contracts, failure & resilience
  mixed/unsure→ scaffold all, then prune.
Delete every sub-heading the shape doesn't select. -->

### Design decisions
- HTTP client injection: Pass client as dependency rather than constructing internally (framework pattern) · Traces to: AC-4 · Satisfies: caller-provided URL requirement
- Model metadata in sqlite-vec: Store model name and dimensions in vectors table metadata field · Traces to: AC-1 · Satisfies: retrieval with provenance
- Dimension validation at storage: Validate vector length matches declared dimensions before write · Traces to: AC-2 · Satisfies: data integrity
- Error propagation without fallback: Propagate embedding errors directly to caller · Traces to: AC-5 · Satisfies: framework boundary

### Interfaces & contracts
- VectorQueryProvider trait: Extend or implement trait to accept caller-provided embeddings with metadata · Traces to: AC-1, AC-4 · Existing: core/retrieval/src/ports.rs
- Dimension validation interface: Function signature validates Vec<f32> length against declared dimensions · Traces to: AC-2 · New in adapters/retrieval/sqlite-vec/src/validation.rs
- HTTP client injection point: Generic client trait or interface for Ollama HTTP operations · Traces to: AC-4 · New in core/retrieval/src/clients.rs

### Failure, edge cases & resilience
- Dimension mismatch: Return error before storage, no partial write · Traces to: AC-2
- HTTP client errors: Propagate timeout, connection, and HTTP errors without retry · Traces to: AC-5
- Invalid embedding format: Return validation error without storage attempt · Traces to: AC-2, AC-5
- Missing model metadata: Reject embedding storage requiring model name and dimensions · Traces to: AC-1
- sqlite-vec table dimension mismatch: Error when existing table dimensions don't match embedding dimensions · Traces to: AC-2

### Dependencies & integration
- HTTP client library: Add reqwest or similar Rust HTTP client as dependency · Traces to: AC-4 · New dependency in adapters/retrieval/sqlite-vec/Cargo.toml
- Ollama service: Caller-provided URL, no Engram-managed service discovery · Traces to: AC-4 · External dependency managed by caller
- sqlite-vec: Existing vector storage with fixed-dimension constraint · Traces to: AC-1, AC-2, AC-3 · Existing dependency in adapters/retrieval/sqlite-vec
- VectorQueryProvider trait: Existing interface for query embeddings · Traces to: AC-3 · Existing in core/retrieval/src/ports.rs

> **Rollout & deployment** — the tenth design dimension — is **not** a
> sub-heading here. It is realized by [`## Rollout`](#rollout) below (infra,
> external-system integration, deployment sequencing). Cross-link it from the
> relevant sub-sections; never duplicate it.

## Tasks

The work-breakdown. Tasks are sized so each one is a coherent commit or PR.
**Phrase each task as a verifiable goal, not a procedure.** The task name
*is* the success criterion: *"Add validation"* → *"All invalid-input tests
pass"*; *"Refactor X"* → *"Tests for X green before and after; public
surface unchanged"*. **Within each task, `Tests:` comes before `Approach:`** —
tests drive implementation, not the other way around. Use red-green-refactor
with separate commits when the change is non-trivial.

**Every task must declare `Depends on:` explicitly** — list prior task IDs
or `none`. Don't omit the field; "obvious from order" is the failure mode
that hides serial-by-default thinking. `none` is a valid and common answer.

**`Depends on:` grammar** (so the supervisor-mode scheduler —
`loop-cohort schedule` — can read it). The field is a comma-separated list of:
local task IDs (`T1`, `T1a`), ranges (`T1-T6`), or a **cross-spec marker**
`spec:<name>/TN` for a dependency on another spec's task (e.g.
`spec:auth-tokens/T7`). Parenthetical prose after the IDs is
ignored, so `T11 (lands after the shim)` is fine. Cross-spec deps are
*spec-sequencing*, not intra-plan waves, and are excluded from this plan's
DAG. The scheduler **fails on a dependency cycle** and **warns on a
forward-reference** (a dep authored later — it still schedules correctly by
running the dep first).

**Optional `Touches:` grammar** (read by `loop-cohort schedule`).
A task *may* add a `**Touches:**` line listing the file globs it expects to
touch — a comma-separated list of paths/globs (`src/api/*.py, docs/api.md`),
trailing prose ignored. `loop-cohort schedule` uses it to predict, per wave,
`predicted-disjoint: yes|no|unknown` **before** dispatch — a cheap
*serialize-only* screen. It **never greenlights** parallel: a predicted overlap
serializes early, but `yes`/`unknown` still require the authoritative post-write
`git merge-tree` check to actually parallelize (under-declaration is unsafe).
The field is **optional** — omit it freely; a task with no `Touches:` makes its
wave `unknown`, never an error.


### T1: Add HTTP client dependency and injection interface

**Depends on:** none

**Tests:**
- Test that HTTP client can be injected via dependency parameter
- Test that injected client is used for operations without creating internal instance

**Approach:**
- Add reqwest dependency to adapters/retrieval/sqlite-vec/Cargo.toml
- Create HttpClient trait in core/retrieval/src/clients.rs with methods for embedding operations
- Implement concrete reqwest-based client in adapters/retrieval/sqlite-vec/src/reqwest_client.rs

**Done when:** Typecheck passes with new dependency and trait compilation succeeds

### T2: Extend VectorQueryProvider trait for caller-provided embeddings

**Depends on:** T1

**Tests:**
- Test that embeddings with model metadata are accepted via trait interface
- Test that model name and dimensions are stored with embedding vectors

**Approach:**
- Extend VectorQueryProvider trait in core/retrieval/src/ports.rs to include model metadata parameters
- Implement trait method in adapters/retrieval/sqlite-vec/src/index.rs to accept caller embeddings with metadata
- Update vectors table schema to store model_name and dimensions in metadata field

**Done when:** Integration test passes storing embedding with model name "ollama/nomic-embed-text" and dimensions 768

### T3: Implement dimension validation at storage

**Depends on:** T2

**Tests:**
- Test dimension mismatch error when vector length ≠ declared dimensions (TDD)
- Test valid dimensions pass through without error (TDD)
- Test error propagation on missing or invalid dimension declarations (TDD)

**Approach:**
- Create validation module in adapters/retrieval/sqlite-vec/src/validation.rs
- Implement validate_dimensions function checking Vec<f32>.len() against declared dimensions
- Add validation call before embedding storage in SqliteVectorIndex
- Return DimensionMismatch error on validation failure

**Done when:** All dimension validation tests pass, including edge cases for empty vectors and zero dimensions

### T4: Implement error propagation without fallback

**Depends on:** T3

**Tests:**
- Test HTTP timeout errors propagate to caller without retry (TDD)
- Test HTTP connection errors propagate with original error context (TDD)
- Test invalid embedding format errors propagate without storage attempt (TDD)

**Approach:**
- Wrap HTTP client errors in framework error type preserving original context
- Remove any retry or fallback logic from embedding operations
- Ensure error paths return directly to caller without transformation
- Add error propagation tests using mock HTTP client failures

**Done when:** Error propagation tests pass for timeout, connection, and validation failures

### T5: Add vector search integration tests

**Depends on:** T4

**Tests:**
- Integration test storing multiple embeddings with different models and searching by similarity
- Integration test verifying cosine similarity ordering for search results
- Integration test for metadata retrieval alongside search results

**Approach:**
- Create integration test file in adapters/retrieval/sqlite-vec/tests/integration_test.rs
- Seed test data with embeddings from different Ollama models (different dimensions)
- Execute vector search queries and validate result ordering
- Verify metadata (model name, dimensions) is returned with search results

**Done when:** Integration tests pass showing sqlite-vec vector search with stored embeddings returns correct cosine similarity ordering



## Rollout

This is a framework integration with no infrastructure changes. Deployment sequencing: T1 (HTTP client dependency) ships before T2-T5 (feature implementation). No data migration required since sqlite-vec schema changes are additive only. External Ollama services are caller-managed with no Engram deployment dependencies.

- **Delivery:** Big bang — all tasks ship together since feature is complete only with HTTP client, validation, and integration tests together
- **Infrastructure:** No infrastructure changes — pure library update with new HTTP client dependency
- **External-system integration:** Ollama services are caller-managed, no Engram service dependencies
- **Deployment sequencing:** HTTP client dependency (T1) must ship before feature tasks (T2-T5), but no database migration or external service coordination required

## Risks

- **sqlite-vec fixed-dimension constraint** — Existing tables with 384-dimension FastEmbed embeddings may conflict with new Ollama models requiring different dimensions. Mitigation: Validate dimensions before storage and provide clear error messages when table dimensions don't match embedding dimensions.
- **HTTP client dependency compatibility** — Adding reqwest may introduce version conflicts or async runtime complexity. Mitigation: Use feature-gated dependency and keep HTTP calls isolated behind trait interface.
- **Breaking existing FastEmbed behavior** — Extending VectorQueryProvider trait may break existing FastEmbed integration. Mitigation: Trait extension is additive with default implementations preserving existing behavior.
- **Error propagation complexity** — HTTP errors may not map cleanly to existing framework error types. Mitigation: Wrap HTTP errors in framework-specific error types preserving original context.

## Changelog

- 2026-07-06: Initial plan for Ollama embeddings integration as framework-level feature accepting caller-provided embeddings with metadata
