# Spec: memory-cue-anchors

- **Status:** Shipped
- **Owner:** @phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0015
- **Contract:** none

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

When a `MemoryRecord` is written, the memory layer runs a rule-based entity
extractor against `MemoryContent.text` and stores the discovered anchors in
`MemoryContent.entities`. Callers may also pre-populate `content.entities`;
extraction is additive and preserves caller-supplied values. The SQLite
adapter dispatches `RetrievalMode::Cue` against the stored entities, enabling
callers to retrieve memories by named entity or entity kind. A query with
`slot=entity, value="Project Orion", operator=Contains` returns every memory
whose extracted anchors contain a name matching "Project Orion"; a query with
`slot=kind, value=person` returns memories with a person anchor. This enables
the multi-hop retrieval pattern documented in ADR-0015 without relying on
keyword or semantic similarity.

## Boundaries

### Always do

- Run extraction synchronously in the write path — pure Rust, no async, no
  model calls, no I/O — before the record is handed to
  `write_memory_transaction`.
- Preserve caller-supplied `content.entities` values: merge extracted results
  with caller-supplied ones, deduplicate by `name.to_lowercase()` (caller
  wins on conflict); entities with `name: None` bypass name-dedup and are
  kept as-is.
- Apply all existing lifecycle, scope, expiry, redaction, and policy filters
  to cue-mode candidates; cue scoring runs only after a record passes every
  existing guard.
- Populate `score.cue_match` on every result matched by the cue pass (i.e.
  `cue_score > 0.0`); a record surfaced only by keyword in a both-modes
  request has `cue_match: None`.
- Use `request.modes` as the sole activation authority for mode dispatch.
  Never activate a mode because `request.cues` is non-empty — cues are
  inputs to cue scoring, not a dispatch trigger.
- Run keyword scoring only when `modes` is empty (backward-compatible default)
  or `modes` explicitly contains `RetrievalMode::Keyword`. When `modes` is
  non-empty and excludes `Keyword`, keyword scoring is skipped.
- When `modes` is non-empty and contains neither `Keyword` nor `Cue`,
  `SqlMemoryService` returns an empty payload — those modes are served by
  separate adapter implementations.
- Require a non-empty `query` string on every `RetrievalRequest` (the
  existing validation gate at `validation.rs:51` is unchanged). For
  cue-only requests, `query` carries the intent description (e.g. the entity
  name); keyword scoring is skipped when `Keyword` is not in `modes`.

### Ask first

- Expanding extraction beyond capitalised-sequence heuristics (dictionary
  lookup, regex crate, NLP call).
- Capping max entities per record at a value other than 20.
- Changing `EntityRef.kind` inference after initial ship.
- Supporting `CueOperator::In`, `CueOperator::Range`, or
  `CueOperator::Exists` (deferred; see Never do).

### Never do

- Make LLM or embedding calls in the write path.
- Add new SQLite tables or columns — entities are stored inside `record_json`;
  no schema migration.
- Modify `EntityRef` or `MemoryContent` domain types (both already exist in
  the accepted v1 schema).
- Return any result from a cue query that did not pass the existing scope,
  policy, lifecycle, and expiry filters.
- Implement `CueOperator::In`, `CueOperator::Range`, or
  `CueOperator::Exists` in this pass (deferred to a later spec).
- Match `entity.aliases` in cue dispatch — `slot="entity"` matches only
  `entity.name` in this pass; alias matching is deferred.

## Testing Strategy

- **Entity extraction invariants — TDD.** The extractor is a pure function
  (`&str → Vec<EntityRef>`); every extraction rule is a compressible
  invariant testable without I/O.
- **Cue dispatch correctness — TDD.** Given seeded `MemoryRecord` values with
  known `content.entities`, a `RetrievalRequest` with the relevant modes must
  return exactly the expected subset. Equals, Contains, StartsWith, EndsWith,
  and operator=None (Equals default) each have dedicated unit tests.
- **Write-path wiring — goal-based check.** An integration test writes through
  `SqlMemoryService`, reads back via `store.list_memories()`, and asserts
  `content.entities` is populated as expected.
- **Regression — goal-based check.** All existing retrieval tests pass
  unchanged; extraction must not alter keyword behavior for requests that do
  not include `RetrievalMode::Cue`.

## Acceptance Criteria

- [x] A memory written with text containing ≥ 2 consecutive capitalised words
  (e.g. `"Project Orion"`, `"Dave Smith"`) has those sequences stored in
  `content.entities` after the write call returns, with punctuation stripped
  from token boundaries.
- [x] A memory written with pre-populated `content.entities` has caller values
  preserved; extraction appends only names not already present (by
  `name.to_lowercase()`); entities with `name: None` are kept as-is.
- [x] A `RetrievalRequest{query:"Project Orion", modes:[Cue], cues:[{slot:
  "entity", value:"Project Orion", operator:Contains}]}` returns records
  whose `content.entities` contain a name matching "Project Orion"
  (case-insensitive).
- [x] A `RetrievalRequest{query:"person anchors", modes:[Cue], cues:[{slot:
  "kind", value:"person", operator:Equals}]}` returns records whose
  `content.entities` contain at least one entry with `kind="person"` (kind
  supplied by the caller at write time; extraction always yields
  `kind="unknown"`).
- [x] `RetrievalScore.cue_match` is set (non-None) on every result matched
  by the cue pass; a record surfaced only by keyword in a both-modes
  request has `cue_match: None`.
- [x] A `RetrievalRequest` with `modes:[Keyword, Cue]` returns the union of
  keyword and cue candidates; each result carries `score.cue_match` and/or
  `score.relevance` depending on which active mode matched it (a record
  surfaced only by cue has `relevance: None`; one surfaced only by keyword
  has `cue_match: None`); `score.total` is
  `max(score.relevance.unwrap_or(0.0), score.cue_match.unwrap_or(0.0))`.
- [x] A `RetrievalRequest` with `modes:[Cue]` and a cue matching no stored
  entity returns an empty `ContextPayload.items`.
- [x] `Cue.weight` is accepted in the request but is not applied to scoring
  in this pass (no weighted aggregation).
- [x] Extraction adds no new third-party crates beyond `std` and the existing
  `engram_domain` dependency, and is free of model calls (verified by the
  absence of any `async` keyword or trait-provider `use` in
  `core/memory/src/extraction.rs`).
- [x] A `RetrievalRequest` with `modes:[Cue]` and `include_explanations:
  true` returns results whose `explanation.matched_cues` contains the cues
  that matched for that record.
- [x] A `RetrievalRequest` with `modes:[Keyword, Cue]` and
  `include_explanations: true` where a record is matched only by keyword
  (no cue matched) returns that record with `explanation.matched_cues`
  empty.
- [x] A `RetrievalRequest` with `modes:[Semantic]` (or any set that contains
  neither `Keyword` nor `Cue`) returns an empty `ContextPayload.items`.
- [x] `cargo test --workspace` passes with no new failures.

## Assumptions

- Technical: `EntityRef { id: Option<EntityId>, kind: Option<String>, name:
  Option<String>, aliases: Vec<String> }` is the existing type
  (`core/domain/src/types.rs:94`).
- Technical: `MemoryContent.entities: Vec<EntityRef>` is in the v1 schema,
  always `Vec::new()` in production; write path passes `request.content`
  through unchanged (`adapters/memory/sqlite/src/write.rs:43`).
- Technical: `Cue.value` is `Scalar` (`serde_json::Value`); this spec
  supports only string-valued `Scalar` variants. A non-string value on a
  recognised slot (`"entity"` or `"kind"`) counts as unmatched and remains
  in the score denominator (dilutes the score); it is not excluded like an
  unknown-slot cue. Unknown-slot cues are excluded from the denominator.
- Technical: `retrieve()` in the SQL adapter ignores `request.modes`; mode-
  aware dispatch is purely additive (`retrieval.rs:21–116`).
- Technical: `validate_retrieval_request` rejects empty `query`
  (`validation.rs:51`); this validation is unchanged.
- Technical: `CueOperator` defines Equals, Contains, StartsWith, EndsWith,
  Exists, In, Range; only Equals, Contains, StartsWith, EndsWith and
  operator=None (defaults to Equals) are implemented in this pass.
- Product: extraction runs synchronously in the write path; capitalised-
  token-sequence heuristic for the first pass (user confirmation 2026-07-03).
