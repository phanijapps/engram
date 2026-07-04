# Plan: contract-first-ingestion

- **Spec:** [`spec.md`](spec.md)
- **Status:** Drafting

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog
> at the bottom.

## Approach

Add an OpenAPI-aware extraction path inside the existing `adapters/ingest`
crate, alongside the current code-symbol `GraphExtractor`. During a scan, a
document recognized as an OpenAPI spec is parsed into a small internal model
(paths → operations), each operation is normalized to a stable key
(`METHOD /path/template`, params folded to placeholders), and emitted as an
`EntityKind::Api` entity plus an `exposes` `KnowledgeRelationship` from the
declaring `KnowledgeSource`, through the existing
`KnowledgeRepository::put_entity`/`put_relationship` ports. Because entity
identity is derived from the normalized key, two sources declaring the same
operation upsert into one entity that accrues both `source_refs` — the cross-repo
link. The riskiest parts are (a) the new parse dependency (an "Ask first"
boundary) and (b) keeping entity keying stable and scope-correct so the merge
happens without colliding across tenants. Testing is TDD for the pure key
derivation and the operation→entity mapping, and an integration test for the
cross-repo merge over the ingest→store path.

## Constraints

- **ADR-0016** — contract nodes keyed by a normalized contract identifier with
  typed `exposes` edges; not symbol-name matching.
- **ADR-0017** — repository identity rides on the `KnowledgeSource` within one
  shared scope; this slice attaches to that source (the Repository node is a
  separate foundation spec).
- **RFC-0008** — OpenAPI is the first, highest-reliability rung; consumer-side
  and AsyncAPI/`.proto` are later phases. Edge-level authority is OQ1 (not here).

## Construction tests

**Integration tests:** (T5) ingest two sources declaring the same OpenAPI
operation → assert a single `Api` entity whose `source_refs` include both; (T6)
malformed-document skip-and-continue over a full ingest run.
**Cross-cutting checks:** (T7) AC-6 goal-based check — `engram-ingest` imports no
model/LLM dependency; crate + top-level module inventory shows no new boundary.
**Manual verification:** none beyond the above.

## Design (LLD)

Shape: `service`. Stack: Rust workspace; `adapters/ingest` (extraction),
`core/domain` (`KnowledgeEntity`/`KnowledgeRelationship`/`EntityKind::Api`),
`core/knowledge` ports (`KnowledgeRepository`). No reference architecture file
present; conforms to the established crate layout.

### Design decisions
- Reuse `EntityKind::Api` for REST operations rather than adding a kind — keeps
  the slice contract-neutral (a channel kind is deferred, RFC-0008 OQ2).
  Traces to: AC-1 · none.
- Derive entity identity from `(scope, normalized-key)` so cross-repo merge is
  an upsert, not a post-hoc resolver — reuses the existing content-addressed
  keying discipline. Traces to: AC-4 · none.

### Data & schema
- New records are `KnowledgeEntity { kind: Api, name: normalized-key, ... }` and
  `KnowledgeRelationship { predicate: "exposes", subject: source, object: api,
  confidence: Some(..) }`. Operation detail (method, path, summary, media types)
  is carried in the entity `metadata`. No schema/table change — the SQLite
  adapter stores these as existing entity/relationship records. Traces to:
  AC-1, AC-2, AC-3 · none.

### Interfaces & contracts
- Consumes *external* OpenAPI documents (v3.x). No engram interface surface is
  exposed. Traces to: AC-1 · none.

### Failure, edge cases & resilience
- Malformed/unparseable OpenAPI: caught per-document, logged as a warning,
  scan continues (AC-5). Non-OpenAPI YAML/JSON: not misclassified — detection
  requires an OpenAPI marker (`openapi:`/`swagger:` version field). Traces to:
  AC-5 · none.

### Quality attributes (NFRs)
- Deterministic, no model calls (AC-6). No new top-level crate/module boundary
  (AC-6). Traces to: AC-6 · none.

## Tasks

### T1: OpenAPI/YAML parse dependency added (Ask-first)

**Depends on:** none · **Enables:** AC-1, AC-2 (parse prerequisite)

**Tests:**
- Goal-based: `cargo check -p engram-ingest` passes with the new dependency.

**Approach:**
- Propose the specific crate(s) to the owner before adding — candidate:
  `serde_yaml` (or `serde_yml`) for YAML, parsing into a minimal local
  `openapi` model with `serde` (avoid a heavy full-model crate unless needed).
- Add to `adapters/ingest/Cargo.toml` only after sign-off (spec Boundaries →
  Ask first).

**Done when:** the chosen parse dependency is present and the ingest crate
compiles.

### T2: Normalized contract-key derivation

**Depends on:** none

**Tests:**
- TDD: `GET /orders/{id}` and `GET /orders/{orderId}` both normalize to
  `GET /orders/{}`.
- TDD: method is upper-cased; trailing slash normalized; two documents' matching
  operations produce byte-identical keys (AC-1, AC-4).

**Approach:**
- Add a pure `fn normalize_contract_key(method, path) -> String` in a focused
  module under `adapters/ingest/src/`.

**Done when:** the key-derivation unit tests are green.

### T3: OpenAPI document detection + operation parse

**Depends on:** T1

**Tests:**
- TDD: a fixture OpenAPI doc parses into the expected `(method, path, summary,
  request/response media types)` operation list.
- TDD: a YAML/JSON file without an OpenAPI/`swagger` version marker is not
  treated as a contract (AC-5 guard).

**Approach:**
- Detect via the `openapi:`/`swagger:` version field; parse `paths`→operations
  into the minimal local model from T1.

**Done when:** the parse unit tests are green for a valid fixture and a non-spec
YAML file is rejected.

### T4: Emit contract entities + `exposes` edges

**Depends on:** T2, T3

**Tests:**
- TDD: one parsed document yields one `EntityKind::Api` entity per operation
  (keyed by the normalized key) with operation detail in metadata and a
  `source_ref` to the declaring source (AC-1, AC-2).
- TDD: each entity has an `exposes` `KnowledgeRelationship` from the source with
  a populated `confidence` (AC-3).

**Approach:**
- Add a contract-extraction step to the ingest path that maps parsed operations
  to entities/relationships and writes them via
  `KnowledgeRepository::put_entity`/`put_relationship`. Derive the contract-entity
  id **solely** from `(full-scope-key, normalized-key)` — document-independent,
  with no `graph_id` tie — so two repos declaring the same operation upsert into
  one entity. Do **not** reuse `extractor.rs`'s `entity_id(graph_id, name)`
  (`extractor.rs:362-366`), which hashes a per-document `graph_id` and would give
  each repo a distinct id, defeating the merge (AC-4).

**Done when:** the extraction unit tests are green.

### T5: Cross-repo merge on shared key

**Depends on:** T4

**Tests:**
- Integration: ingest two sources (distinct `KnowledgeSource`s, same scope) each
  declaring the same operation → assert one `Api` entity whose `source_refs`
  contains both sources, and two `exposes` edges (AC-4).

**Approach:**
- `put_entity` overwrites `record_json` on conflict (`service.rs:291-297`), so
  accrual is an explicit **read-modify-write union in the ingest path**: read the
  existing contract entity by id, union the new `source_ref`, write back. Do
  **not** change shared `put_entity` semantics — that would alter write behaviour
  for every entity kind and needs its own ADR.
- Run the read-modify-write inside the store's connection-lock critical section
  so a concurrent same-key ingest cannot lose a `source_ref`; ingestion is
  otherwise assumed serialized per scope (see Risks).

**Done when:** the integration test is green.

### T6: Malformed spec skipped, scan continues

**Depends on:** T3

**Tests:**
- Integration (assertion-based): a truncated/invalid OpenAPI document increments
  `ScanSummary.skipped`, leaves `ScanSummary.errors` unchanged, emits a logged
  warning, and the job completes successfully with no contract entity for it
  (AC-5).

**Approach:**
- Wrap the per-document contract parse in a recover-and-warn boundary: on parse
  failure, log a warning and increment `skipped` (not `errors`); never propagate
  the error to fail the job.

**Done when:** the malformed-document test is green and the job reports success
with `errors` unchanged.

### T7: AC-6 conformance check — no model calls, no new boundary

**Depends on:** T4 · **Verifies:** AC-6

**Tests:**
- Goal-based: a check (grep/inventory) proves `engram-ingest`'s manifest imports
  no model/LLM dependency, and the crate + top-level module inventory shows no
  new crate or module boundary introduced by this feature (AC-6).

**Approach:**
- Add the check as a repo-level goal-based assertion (a small script or a
  documented `grep`/`cargo tree` invocation) runnable in CI.

**Done when:** the check passes and is recorded as the AC-6 artifact.

## Rollout

Pure library/ingestion-logic change inside `adapters/ingest`. No infra, no flag,
no external-system dependency, no deployment sequencing. Reversible: contract
records are new and additive; removing the extractor stops producing them.

## Risks

- **Parse dependency choice** — a heavy full-OpenAPI-model crate could pull a
  large tree; mitigate by parsing into a minimal local model over `serde_yaml`.
- **`put_entity` overwrite semantics** — if the store overwrites rather than
  accrues `source_refs`, the cross-repo merge (T5) needs explicit merge logic;
  flagged in T5.
- **Scope keying** — deriving entity id from `(scope, normalized-key)` must
  include the full scope discriminator so two tenants never collide on the same
  operation (mirrors the reconcile scope-key discipline).
- **Concurrent same-key ingest** — the `source_refs` union is a read-modify-write
  over an overwrite-on-conflict `put_entity`, so it must run inside the store's
  connection-lock critical section (or ingestion be serialized per scope) to
  avoid a lost update. Flagged in T5.

## Changelog

- 2026-07-04: initial plan.
