# Plan: memory-cue-anchors

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

> **Plan contract:** this is the implementation strategy. Unlike the spec,
> this document is allowed to change as you learn. When it changes
> substantially, note why in the changelog at the bottom.

## Approach

Three independent pieces: a pure extractor module, write-path enrichment, and
cue-mode dispatch in retrieval. T1 (extractor) and T3 (cue dispatch) have no
shared state and can be developed in parallel; T2 (write wiring) depends on
T1; T4 (integration smoke test) closes the round-trip and depends on T2+T3.

The riskiest part is the mode-aware dispatch change: the adapter currently
ignores `request.modes` and always runs keyword. After this change, keyword
is skipped when modes is non-empty and excludes `Keyword`. That is a
behaviour change for any caller that passes `modes:[Cue]` today (none exist
in production) and for callers that pass an explicit `modes:[Keyword]` (must
continue to work identically). The regression tests in T3 guard this.

## Constraints

- ADR-0015: `content.entities` is the canonical backing store; no new tables;
  entities live inside `record_json`.
- Extractor placement: `core/memory/src/extraction.rs` is justified by ADR-0015,
  which explicitly distinguishes inline cue anchors (memory-write enrichment) from
  graph entity resolution (`engram-knowledge`). The extractor is a pure write-path
  enrichment helper, not a knowledge port, source reader, or ingestion adapter.
  AGENTS.md scopes `engram-memory` to "memory service and repository ports"; this
  extractor is a private implementation detail of the write port, not a new public
  port boundary. It must not depend on SQL, async runtimes, or embedding providers.
- `validate_retrieval_request` (`validation.rs:51`) rejects empty `query`;
  unchanged — callers must pass a non-empty query even for cue-only requests.

## Construction tests

**Integration tests:** T4 covers the full write → retrieve round-trip.

**Manual verification:** none — all behaviors are machine-verifiable.

## Design (LLD)

### Data & schema

No schema changes. Entities are stored inside `record_json`. `MemoryContent.entities`
is already part of the serialised record. No migration.

### Component / module decomposition

| Component | Location | Responsibility |
|---|---|---|
| `extract()` | `core/memory/src/extraction.rs` | Pure `fn extract(text: &str) -> Vec<EntityRef>` |
| Write enrichment | `adapters/memory/sqlite/src/write.rs` | Merge extracted entities before `write_memory_transaction` |
| `cue_score()` | `adapters/memory/sqlite/src/retrieval.rs` | `fn cue_score(record: &MemoryRecord, cues: &[Cue]) -> CueMatch` where `CueMatch { score: f32, matched: Vec<Cue> }` |
| Mode dispatch | `adapters/memory/sqlite/src/retrieval.rs` | Mode-aware fan-out: keyword vs cue vs both |

### Behavior & rules

**Extraction rules (`extract()`):**

1. Tokenise `text` by whitespace.
2. Trim leading/trailing punctuation (`.,;:!?'"()[]{}`) from each token
   before checking capitalisation. Tokens that trim to empty string break
   the run (treated as a non-capitalised separator).
3. Find maximal runs of ≥ 2 consecutive non-empty trimmed tokens where every
   token starts with a Unicode uppercase letter (`c.is_uppercase()`).
4. Each run → `EntityRef { name: Some(<tokens joined by space>), kind:
   Some("unknown"), id: None, aliases: vec![] }`.
   `kind` is always `"unknown"` for extracted entities. Kind classification
   (person, project, concept, etc.) requires a lexicon or model call, which is
   out of scope for this pass (`spec.md` Boundaries/Ask-first: "dictionary lookup").
   Callers that know the kind at write time supply it in `content.entities`.
5. Deduplicate extracted results among themselves by `name.to_lowercase()`
   before the caller merge; keep the first occurrence in document order.
6. Cap: if the deduplicated extracted list exceeds 20, keep the 20 with the
   longest `name` by character count; ties broken by document order (first
   occurrence wins). The cap applies to extracted-only; caller-supplied
   entities are always preserved regardless, so the post-merge total may
   exceed 20.
7. Merge with caller-supplied entities: deduplicate by `name.to_lowercase()`;
   caller entries win on conflict; entities with `name: None` bypass
   name-dedup and are kept as-is.

**Cue dispatch rules:**

Cue scoring activates only when `request.modes` contains `RetrievalMode::Cue`.
`request.cues` being non-empty is not a dispatch trigger — it is an input to
scoring. For each record that passes all existing filters:

- `slot = "entity"`: match `cue.value` (string) against each
  `entity.name` (string) using the specified operator (case-insensitive).
- `slot = "kind"`: match `cue.value` (string) against each `entity.kind`
  (string) using the specified operator; entities with `kind: None` are
  skipped for kind-slot cues.
- Supported operators and semantics (all string, case-insensitive):
  - `Equals` / `None` (default): exact match after lowercasing both sides.
  - `Contains`: `entity_field.to_lowercase().contains(&value.to_lowercase())`.
  - `StartsWith`: `entity_field.to_lowercase().starts_with(...)`.
  - `EndsWith`: `entity_field.to_lowercase().ends_with(...)`.
- `In`, `Range`, `Exists` — not implemented in this pass; a cue with these
  operators is silently skipped (produces no match for that cue).
- Non-string `Scalar` values — silently skipped (produce no match).
- `Cue.weight` — accepted in the request, not applied to scoring.
- `cue_score` computation:
  1. Filter `cues` to only recognized-slot entries (`slot == "entity"` or
     `slot == "kind"`); call this `recognized`.
  2. If `recognized.is_empty()`, short-circuit to `0.0` (covers both
     all-unknown-slot and empty-cues cases).
  3. Otherwise: `cue_score = (recognized cues that matched) as f32 / recognized.len() as f32`.
  Unrecognised slots are silently skipped and excluded from the denominator.
  Non-string `Scalar` values on a recognized slot count as unmatched in the
  numerator and remain in `recognized.len()` (they dilute the score).
  Unsupported operators (`In`, `Range`, `Exists`) on recognized slots also
  count as unmatched and remain in `recognized.len()`.
- A record with `cue_score > 0.0` is kept; `== 0.0` is excluded from cue
  results.
- The merged candidate list (cue-only or both-modes) is sorted by
  `(score.total desc, record.created_at desc, record.id asc)` — the same
  key as the keyword path. Candidates beyond `effective_max_items` are
  placed in `omitted` with `OmittedReason::BudgetExceeded`.

**Mode-aware dispatch:**

`request.modes` is the sole dispatch authority. Activation is predicate-based
(any combination of modes is valid; unrecognised modes are ignored):

```
keyword_active = modes.is_empty() || modes.contains(RetrievalMode::Keyword)
cue_active     = modes.contains(RetrievalMode::Cue)
```

If neither `keyword_active` nor `cue_active`, return an empty payload.
A non-empty `request.cues` with `Cue` absent from `modes` does NOT activate
cue scoring.

Examples:

| `request.modes` | Keyword | Cue |
|---|---|---|
| empty / None | yes | no |
| `[Keyword]` | yes | no |
| `[Cue]` | no | yes |
| `[Keyword, Cue]` | yes | yes |
| `[Keyword, Semantic]` | yes | no (Semantic ignored here) |
| `[Cue, Semantic]` | no | yes (Semantic ignored here) |
| `[Semantic]` | no | no → empty payload |

When both Keyword and Cue run, results are merged by `record.id`:
- `score.relevance` = `total_score` from `keyword_score()` at `retrieval.rs:194`
  — the confidence-blended value `(relevance*0.85)+(confidence*0.15)`.
  `None` when keyword not active or no keyword match.
- `score.cue_match` = cue score (from cue pass). `None` when cue not active.
- `score.total` = `max(score.relevance.unwrap_or(0.0), score.cue_match.unwrap_or(0.0))`.
- `fusion_trace.fusion_strategy`: `FusionStrategy::None` for single-mode results
  (keyword-only or cue-only); `FusionStrategy::MaxScore` for both-modes records.
- `fusion_trace.source`: `"sql.memory.keyword"` for keyword-only results,
  `"sql.memory.cue"` for cue-only results, `"sql.memory.keyword+cue"` for
  records matched by both.
- `fusion_trace.source_rank`: 1-indexed position of this record in the
  sorted merged candidate list (same as the existing keyword-path convention).
- `fusion_trace.source_score`: `score.cue_match` for cue-only results;
  `score.relevance` for keyword-only results; `score.total` for both-modes.
- `fusion_trace.fusion_score`: `score.total` for all result types.
- `fusion_trace.rerank_score` and `fusion_trace.rerank_strategy`:
  - keyword-only results: unchanged from the existing path — `rerank_score:
    Some(total_score)`, `rerank_strategy: Some(RerankStrategy::None)`
    (`retrieval.rs:245-246`).
  - cue-only and both-modes results: `rerank_score: None`,
    `rerank_strategy: None` (no reranking applied in this pass).
- `explanation.reason`:
  - keyword-only: `"Matched memory content with SQL keyword retrieval."` (unchanged).
  - cue-only: `"Matched memory entity anchors with SQL cue retrieval."`.
  - both-modes: `"Matched memory content and entity anchors."`.
- `explanation.matched_cues`: populated from `CueMatch.matched` for this
  record (when `include_explanations` is true); empty for keyword-only results.
- `explanation.matched_terms`: keyword matched terms for keyword-only and
  both-modes records (unchanged from keyword path); empty (`[]`) for cue-only.
- `explanation.source_summary`: `record.content.summary` for all result types
  (unchanged from keyword path).
- `explanation.path`: `[]` for all result types (no graph/hierarchy path).
- `score.confidence`: carry `record.provenance.confidence` (same as keyword path).
- `score.recency`: `None` (recency scoring not implemented in this pass).
- `score.policy_fit`: `Some(1.0)` for records that passed all policy gates
  (same as keyword path).
- A record appears if it passes at least one active mode's threshold.

### Failure, edge cases & resilience

- Text with no capitalised runs → `content.entities` = caller-supplied only
  (possibly empty); write succeeds.
- Extraction is a total pure function over `&str` — it cannot panic; no
  `catch_unwind` wrapper.
- `entity.name = None` — excluded from name-based cue matching; kept in
  the record.

## Tasks

### T1: Pure entity extractor in `core/memory/src/extraction.rs`

**Depends on:** none

**Touches:** `core/memory/src/extraction.rs`, `core/memory/src/lib.rs`

**Tests:**
- `extract("Project Orion is on track")` → `[{name:"Project Orion", kind:"unknown"}]`
- `extract("Dave Smith approved the PR")` → `[{name:"Dave Smith", kind:"unknown"}]`
- `extract("no proper nouns here")` → `[]`
- `extract("Single capitalised word")` → `[]` (only one capitalised token in "Single"; "capitalised" starts lowercase)
- `extract("Dave Smith, the lead")` → `[{name:"Dave Smith", kind:"unknown"}]` (comma trimmed from "Smith,")
- `extract("Élise Martin joined")` → `[{name:"Élise Martin", kind:"unknown"}]` (Unicode uppercase)
- `extract("")` → `[]`
- Text with > 20 qualifying runs → exactly 20 returned; longest by char count, ties by document order (first occurrence).
- `extract` produces `kind:"unknown"` for all extracted runs regardless of structure.
- `extract("Alpha & Beta joined")` → `[]` — `"&"` trims to empty, breaks the run between `"Alpha"` and `"Beta"`; each is a single-token run, not qualifying.
- Caller entity `{name:"Orion", kind:"project"}` merged with `extract` result containing name `"orion"` → caller's entry preserved, no duplicate.
- Caller entity `{name: None, kind:"custom"}` → kept as-is regardless of extracted names.

**Approach:**
- `pub mod extraction;` in `core/memory/src/lib.rs`; re-export
  `pub use extraction::{extract, merge_entities};` so the SQL adapter can call
  both without reimplementing the merge logic.
- Implement `pub fn extract(text: &str) -> Vec<EntityRef>` per the rules in
  Design (LLD). No new third-party crates; `std` plus the existing
  `engram_domain` dependency already imported by `engram-memory`.
- `merge_entities(extracted: Vec<EntityRef>, caller: Vec<EntityRef>) -> Vec<EntityRef>`:
  dedup by `name.to_lowercase()`, caller wins; `None`-named entries bypass.

**Done when:** `cargo test -p engram-memory` green with the unit tests above;
`grep -E "async|dyn.*Provider|impl.*Embed" core/memory/src/extraction.rs` returns
no matches (structural purity check).

---

### T2: Wire extraction into `adapters/memory/sqlite/src/write.rs`

**Depends on:** T1

**Touches:** `adapters/memory/sqlite/src/write.rs`

**Tests:**
- Integration: write `content.text = "Alice Chen reviewed the Project Atlas proposal"`;
  read back; assert `content.entities` contains `{name:"Alice Chen", kind:"unknown"}`
  and `{name:"Project Atlas", kind:"unknown"}`.
- Integration: write with pre-populated `content.entities = [{name:"MyEntity", kind:"custom"}]`
  and `text = "MyEntity launched"` (single capitalised token, no run of ≥ 2);
  assert `content.entities` still contains exactly `{name:"MyEntity", kind:"custom"}`
  and extraction adds nothing new.
- Integration: write with no capitalised runs → `content.entities` equals whatever
  caller supplied; write succeeds.

**Approach:**
- In `write_memory()` before `MemoryRecord` construction, call
  `let extracted = engram_memory::extract(&request.content.text);`.
- Build merged entities via `merge_entities(extracted, request.content.entities.clone())`.
- Assign merged vec to `content.entities` in the `MemoryRecord`.

**Done when:** integration tests above pass; `cargo test -p engram-store-sql` green.

---

### T3: Mode-aware cue dispatch in `adapters/memory/sqlite/src/retrieval.rs`

**Depends on:** none

**Touches:** `adapters/memory/sqlite/src/retrieval.rs`

**Tests:**
- Unit `cue_score`: `slot="entity"`, `operator=Contains`, `value="Orion"` vs record
  with `entities=[{name:"Project Orion"}]` → `CueMatch { score: 1.0, matched: [cue] }`.
- Unit `cue_score`: two cues, one matching, one not → `CueMatch { score: 0.5, matched: [matching_cue] }`.
- Unit `cue_score`: `slot="kind"`, `value="person"`, `operator=Equals` vs
  `entities=[{kind:"person"}]` (caller-supplied kind) → `CueMatch { score: 1.0, matched: [cue] }`.
- Unit `cue_score`: empty entities → `0.0`.
- Unit `cue_score`: one matching recognized-slot cue + one unknown-slot cue →
  `1.0` (unknown slot excluded from denominator, recognized.len() == 1, matched == 1).
- Unit `cue_score`: all cues have unknown slots → `0.0` (recognized.is_empty(), short-circuit).
- Unit `cue_score`: one matching string cue + one non-string-value cue (JSON number),
  both `slot="entity"` → `0.5` (non-string stays in denominator, counts as unmatched).
- Unit `cue_score`: one matching string cue + one `operator=In` cue, both `slot=
  "entity"` → `0.5` (unsupported operator stays in denominator, counts as unmatched).
- Unit `cue_score`: two cues with identical slot/value but different `weight` values
  → same score as when `weight` is absent (weight is not read by the scorer).
- Integration (`retrieve()` with `include_explanations: true`, `modes:[Cue]`):
  a cue-matched result has `explanation.matched_cues` containing the matching
  cue(s) by value.
- Integration (`retrieve()` with `include_explanations: true`, `modes:[Keyword,
  Cue]`): a record matched only by keyword has `explanation.matched_cues` empty.
- Unit `cue_score`: `operator=StartsWith`, `value="Pro"` vs `{name:"Project Orion"}` → `1.0`.
- Unit `cue_score`: `operator=EndsWith`, `value="orion"` vs `{name:"Project Orion"}` → `1.0`.
- Unit `cue_score`: `operator=None` → behaves as `Equals`.
- Unit `cue_score`: `Cue.value` is a JSON number → cue skipped, `0.0` contribution.
- Unit `cue_score`: `operator=In` → cue silently skipped, `0.0`.
- Integration: seed two records — one with `entities=[{name:"Project Orion"}]`, one
  without. `RetrievalRequest{query:"Project Orion", modes:[Cue], cues:[{slot:"entity",
  value:"Orion", operator:Contains}]}` → only the first returned;
  `score.cue_match=Some(1.0)`; `fusion_trace.fusion_strategy=FusionStrategy::None`;
  `fusion_trace.rerank_score=None`; `fusion_trace.rerank_strategy=None`;
  `explanation.matched_terms=[]`.
- Regression: `RetrievalRequest` with empty `modes` and no `cues` → identical results
  to before this change (keyword-only path).
- Regression: `RetrievalRequest` with `modes:[Keyword]` → keyword runs, cue does not.
- Regression: `RetrievalRequest` with `modes:[Semantic]` → empty payload from SqlMemoryService (semantic is served by a separate RetrievalIndex adapter, not this path).
- Integration (both modes — same record matched by both): seed one record with
  `entities=[{name:"Alice Chen"}]` and `content.text="Alice Chen joined"`.
  `RetrievalRequest{query:"Alice Chen", modes:[Keyword, Cue], cues:[{slot:"entity",
  value:"Alice Chen", operator:Equals}]}` → record returned once; `score.relevance`
  set; `score.cue_match` set; `score.total == max(relevance, cue_match)`;
  `fusion_trace.source == "sql.memory.keyword+cue"`;
  `fusion_trace.fusion_strategy == FusionStrategy::MaxScore`;
  `fusion_trace.rerank_score == None`; `fusion_trace.rerank_strategy == None`;
  `explanation.matched_terms` non-empty (keyword matched "alice" and "chen").
- Integration (both modes — distinct records): seed record A with
  `content.text="Alice Chen joined"` (no pre-populated entities; extraction
  yields the `Alice Chen` anchor, which the `Helios Platform` cue does not
  match), record B with
  `entities=[{name:"Helios Platform"}]` and `content.text="unrelated text"`.
  `RetrievalRequest{query:"Alice Chen", modes:[Keyword, Cue], cues:[{slot:"entity",
  value:"Helios Platform", operator:Equals}]}` → two results returned; result A has
  `score.relevance` set and `score.cue_match: None`, `fusion_trace.source =
  "sql.memory.keyword"`; result B has `score.cue_match` set and `score.relevance:
  None`, `fusion_trace.source = "sql.memory.cue"`.
- Unit (cue-path sort and budget overflow): seed three records each with a cue-
  matching entity; set `request.limit = 2`. Assert exactly 2 items returned (highest
  `cue_score` first, then `created_at` desc, then `id` asc), and the third appears
  in `omitted` with `reason = OmittedReason::BudgetExceeded`.

**Approach:**
- Define `struct CueMatch { score: f32, matched: Vec<Cue> }` locally in
  `retrieval.rs`; add `fn cue_score(record: &MemoryRecord, cues: &[Cue]) -> CueMatch`.
  Return `matched` as the list of `Cue` values whose slot+operator+value check passed,
  so callers can populate `explanation.matched_cues` without re-running the match.
- Refactor the `retrieve()` loop to compute which modes are active (keyword /
  cue) from `request.modes`; empty modes → keyword only.
- Keyword branch: unchanged logic → candidate `(keyword_score, matched_terms, record)`.
- Cue branch: for each record passing the filter gauntlet, call `cue_score`; if
  `result.score > 0.0`, add to cue candidates with `score.cue_match = Some(result.score)`;
  populate `explanation.matched_cues = result.matched` when `include_explanations`.
- Merge by `record.id`: set `score.relevance`, `score.cue_match`, `score.total =
  max(...)`.

**Done when:** all unit and integration tests above pass; `cargo test -p engram-store-sql` green.

---

### T4: End-to-end integration smoke test

**Depends on:** T2, T3

**Touches:** `adapters/memory/sqlite/tests/cue_anchors_integration.rs`

**Tests:**
- Write `"Sarah Johnson and the Helios Platform team are deploying next week"`.
- Assert `content.entities` contains `{name:"Sarah Johnson", kind:"unknown"}` and
  `{name:"Helios Platform", kind:"unknown"}` (extraction always yields kind:"unknown").
- `{query:"Sarah Johnson", modes:[Cue], cues:[{slot:"entity", value:"Sarah Johnson",
  operator:Equals}]}` → returns the record; `score.cue_match=Some(1.0)`.
- Write a second memory with caller-supplied `content.entities=[{name:"Alice Chen",
  kind:"person"}]`. `{query:"person anchors", modes:[Cue], cues:[{slot:"kind",
  value:"person", operator:Equals}]}` → returns only the second record (kind supplied
  by caller; extraction-only records have kind:"unknown").
- `{query:"nobody", modes:[Cue], cues:[{slot:"entity", value:"Nobody Known",
  operator:Equals}]}` → empty `items`.

**Approach:**
- New integration test file; construct `SqlMemoryService` over an in-memory SQLite
  database and drive the full write → retrieve flow.

**Done when:** all smoke tests pass; `cargo test --workspace` green.

## Rollout

Additive — no schema migration, no flag. Existing records have empty
`content.entities`; they are not retrofitted. Cue-mode queries against old
records return no cue results (correct). New writes accumulate anchors
immediately. Rollback is safe: reverting removes extraction and cue dispatch;
persisted `record_json` is unaffected.

## Risks

- **Extraction false positives** — sentence-initial capitalised words may be
  extracted as entities. Accepted for the initial heuristic; a stoplist can
  be added later.
- **Scan-based cue dispatch** — same O(N) scan as keyword; acceptable at
  current scale. A dedicated index path can follow if latency grows.

## Changelog

- 2026-07-03: initial plan
- 2026-07-03: revised after adversarial review pass 1 — fixed query requirement,
  mode-aware dispatch, CueOperator scope, Scalar type handling, both-modes
  merge formula, extraction punctuation trimming, Unicode uppercase, tie-break,
  weight ignored, catch_unwind removed, constrained-by cleaned up.
- 2026-07-03: revised after adversarial review pass 10 — dropped person-kind
  heuristic (structurally unsatisfiable without dictionary); all extraction
  yields kind:"unknown"; kind-slot AC and T3/T4 tests now use caller-supplied
  kind entities; no structural inference of person vs project remains.
- 2026-07-03: revised after adversarial review pass 9 — ADR-0015 accepted;
  extractor placement justified against AGENTS.md boundary (write enrichment,
  not a port); explanation.reason specified per result type; cue-only score
  field defaults pinned (confidence=provenance, recency=None, policy_fit=1.0).
- 2026-07-03: revised after adversarial review pass 8 — split contradictory
  matched_cues AC into two (modes:[Cue] and modes:[Keyword,Cue]); cue_score
  return type changed to CueMatch{score,matched} so matched_cues is available
  without re-running the match; matched_cues test reclassified as integration
  test against retrieve() not cue_score unit test.
- 2026-07-03: revised after adversarial review pass 7 — spec Assumptions
  corrected: non-string on recognized slot dilutes (stays in denominator),
  only unknown-slot cues are excluded; added ACs for matched_cues and
  empty-payload on unsupported modes; added structural purity check to T1.
- 2026-07-03: revised after adversarial review pass 6 — non-string-value cues
  stay in denominator (dilute), unsupported-operator cues stay in denominator,
  distinguishing tests added for both; weight-invariance test added; FusionStrategy
  specified for single-mode results (None) vs both-modes (MaxScore).
- 2026-07-03: revised after adversarial review pass 5 — denominator is
  recognized-slot count (not cues.len()), short-circuit to 0.0 when recognized
  is empty (covers all-unknown-slot case), added tests for unknown-slot
  denominator exclusion and matched_cues population, fixed misleading
  "(no entities)" comment.
- 2026-07-03: revised after adversarial review pass 4 — cue/keyword/both-modes
  fusion_trace.source strings specified, matched_cues population specified, both-
  modes union test with distinct records added, cue-path sort+overflow test added.
- 2026-07-03: revised after adversarial review pass 3 — cue_match AC scoped to
  cue-pass matches only, dispatch table replaced with predicate form (mixed mode
  sets handled correctly), sort/limit/omitted defined for cue path, empty-after-
  trim tokens break run, unknown slot excluded from denominator.
- 2026-07-03: revised after adversarial review pass 2 — modes table is sole
  dispatch authority (cues non-empty is not a trigger), NaN guard on empty cues,
  non-Keyword/non-Cue modes return empty, relevance defined as blended score,
  FusionStrategy::MaxScore on both-modes path, cap scope clarified
  (extracted-only), within-extraction dedup added, kind:None skip for kind-slot
  cues, AC for both-modes relevance/cue_match corrected.
