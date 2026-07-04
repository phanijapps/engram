# Spec: contract-first-ingestion

- **Status:** Draft
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0016, ADR-0017, RFC-0008
- **Brief:** none
- **Contract:** none (parses *external* OpenAPI files found in scanned repos; exposes no engram interface surface)
- **Shape:** service

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

A developer indexing several repositories wants to see how their services
connect through the REST contracts they share, not just through code symbols.
When an ingested repository contains an **OpenAPI** document, ingestion parses
it and emits a first-class **contract node** per REST operation — an
`EntityKind::Api` entity keyed by a normalized contract identifier
(`METHOD /path/template`) — carrying the operation's detail (method, path,
summary, request/response media types) drawn from the spec, with an `exposes`
edge from the repository that declares it. When two repositories declare the
same normalized contract key, they resolve to **one** contract node that
accrues evidence (`source_refs`) from both, so a cross-repo link is visible on
the shared contract. In this phase the link is proven by **duplicate
declaration** — two repos each *declaring* the same operation (a shared or
shipped OpenAPI document) — not by a producer→consumer call; matching a
consumer's code to a contract is Phase B (declarative-in-code).
Extraction is deterministic (no model calls) and runs as
part of the existing scan/ingest pipeline. This is the highest-reliability rung
of RFC-0008's reliability gradient: the OpenAPI document states the contract
explicitly, so the join key is unambiguous.

## Boundaries

The three-tier guard that keeps an implementing agent inside the lines.
*Always do* applies without asking; *Ask first* requires human sign-off
before proceeding; *Never do* is a hard rule, even under time pressure.

### Always do

- Emit contract nodes as `EntityKind::Api` entities keyed by the normalized
  identifier `METHOD /path/template` (path parameters folded to positional
  placeholders, e.g. `GET /orders/{}`), so two repos declaring the same
  operation resolve to one entity.
- Carry the participation edge as a `KnowledgeRelationship` with predicate
  `exposes` and a `confidence` value, following the existing `GraphExtractor` →
  `KnowledgeRepository::put_entity`/`put_relationship` path.
- Attach the `exposes` edge to the repository's existing `KnowledgeSource`
  identity, and record each declaring source in the contract entity's
  `source_refs`.
- Skip malformed or unparseable OpenAPI documents with a recorded warning and
  continue the scan; a bad spec never fails the job.

### Ask first

- Adding any new crate dependency (a YAML parser and/or an OpenAPI model crate)
  — confirm the specific crate before adding it.
- Introducing a new `EntityKind` variant (e.g. for event channels) — this slice
  reuses `EntityKind::Api`; a new kind is out of scope here.
- Adding an `authority_level` field to `KnowledgeRelationship` — edge-level
  authority representation is RFC-0008 OQ1 and is not decided here.

### Never do

- No consumer-side detection from application code (matching `fetch`/client
  calls to contract keys) — that is Phase B (declarative-in-code) and stays out.
- No new top-level module boundary or crate; contract parsing lives inside the
  existing `adapters/ingest` crate.
- No model/LLM calls in extraction; parsing is deterministic.
- No AsyncAPI or `.proto` handling in this slice (fast-follows, out of scope).

## Testing Strategy

- **Normalized-key derivation** (`METHOD /path/template`, params → placeholders;
  same operation across two documents → same key): **TDD** — a compressible
  invariant with clear inputs/outputs.
- **OpenAPI → contract-node/edge extraction** (a document yields the expected
  `Api` entities + `exposes` edges with the right predicate/confidence):
  **TDD**, exercised at the extractor boundary against a small fixture spec.
- **Cross-repo merge on shared key** (two sources declaring the same operation
  resolve to one entity accruing both `source_refs`): assertion-based
  **integration** test over the ingest→knowledge-store path — the behaviour only
  proves out across the extract→store boundary.
- **Malformed spec is skipped, scan continues** (`ScanSummary.skipped`
  increments, `errors` does not, job succeeds): assertion-based **integration**
  test — a warn-and-continue behaviour observable only through a full ingest run.
- **No model calls / no new crate or module boundary** (AC-6): **goal-based
  check** — a layout/manifest assertion (grep that `engram-ingest` imports no
  model/LLM dependency; crate + top-level module inventory unchanged).

## Acceptance Criteria

- [ ] An ingested repository containing an OpenAPI document produces one
  `EntityKind::Api` contract entity per REST operation, keyed by
  `METHOD /path/template` with path parameters folded to positional placeholders.
- [ ] Each contract entity carries operation detail from the spec (method, path,
  summary if present, request/response media types) and a `source_ref` to the
  declaring `KnowledgeSource`.
- [ ] Each contract entity has an `exposes` `KnowledgeRelationship` from the
  declaring repository's source, with a populated `confidence`.
- [ ] Two sources declaring the same normalized contract key resolve to a single
  contract entity whose `source_refs` include both sources.
- [ ] A malformed or unparseable OpenAPI document is skipped: it increments
  `ScanSummary.skipped` and emits a logged warning, does **not** increment
  `ScanSummary.errors`, and the scan job completes successfully.
- [ ] Extraction performs no model/LLM calls and adds no new top-level crate or
  module boundary.

## Assumptions

- Technical: extraction lives in `adapters/ingest` as `GraphExtractor`
  (`extract`/`extract_with_calls`/`extract_into<R>`), writing via
  `KnowledgeRepository::put_entity`/`put_relationship` (source:
  core/knowledge/src/lib.rs:37,45; adapters/ingest/src/extractor.rs:32).
- Technical: `EntityKind::Api` exists; there is no event/channel variant, so
  REST operations reuse `Api` (source: core/domain/src/knowledge.rs:174-193).
- Technical: `KnowledgeRelationship` has a free-string `predicate` and
  `confidence: Option<f32>`, so the `exposes` edge needs no contract change
  (source: core/domain/src/knowledge.rs:218-236).
- Technical: no OpenAPI/YAML parser exists in the workspace (ingest has only
  `serde_json`; no `serde_yaml`/`openapiv3` in Cargo.lock), so a new dependency
  is required (source: adapters/ingest/Cargo.toml; Cargo.lock grep).
- Technical: `.yaml`/`.yml`/`.json` files are already scanned but classified as
  generic `Code`/`Text`, so contract-aware parsing is additive (source:
  adapters/ingest/src/scanner.rs:62-93).
- Process: this feature parses external contract files and exposes no engram
  interface surface, so no `contracts/` artifact is authored (source:
  new-spec step 4b; CONVENTIONS §4).
- Process: the constraining docs are not yet accepted — ADR-0016/0017 are
  `Proposed` and RFC-0008 is `Draft`; this spec stays `Draft` and does not move
  to `Implementing` until ADR-0016/0017 are accepted (source: docs' status
  headers, 2026-07-04).
- Product: the first slice is OpenAPI (REST) only, producer/declared side, with
  code-level consumer detection and AsyncAPI/`.proto` deferred to later phases;
  edges attach to the existing `KnowledgeSource` rather than a
  not-yet-built Repository node (source: user confirmation 2026-07-04).
