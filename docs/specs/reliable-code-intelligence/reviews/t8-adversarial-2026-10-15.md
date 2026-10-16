# T8 independent adversarial/spec review

Reviewer: `/root/code_eval_blind_review` (agent, not human). Read-only review of `core/integration/src/code_evidence/*`, provider wiring, additive DTO/schema fields, and the accepted T8 construction/spec. No held-out labels or evaluation outputs were inspected. The two explicitly identified T8 live-probe calls/artifact were reviewed as operation evidence; no heavy inference was rerun.

## Verdict

Clean — ready to commit the T8 slice. Both initial P2 findings below are resolved. Focused re-review found no new blocking pool, threshold, source-identity or scoring-view integrity issues. This verdict does not imply T9 transport parity, T10 calibrated quality or the completed feature has shipped.

## Focused re-review closure

- The new optional neutral `CodeSourceRepository::indexed_code_revision(file, scope, requester)` port supplies persisted indexed provenance without conferring raw-source authorization. Its SQLite implementation runs in one transaction, requires a current active head/source tuple, checks exact trusted scope and independent source/document retrieval policy, and uses document version, source version, then source metadata revision. Candidate collection carries the result; final eligibility checks it again. All primary/excerpt/rerank references use the stored value, and same-file/generation/hash context facts are enriched consistently. No filesystem HEAD lookup is used by runtime.
- Ranking now retains only numeric prepared/unscored/attempted/pool-omitted/tail work counts on failure; fallback still clears every view, source text and successful/partial score. Assembly emits separate preparation/attempt counters, and discarded inference work from final-recheck retries is reported separately.
- The adjacent security correction re-resolves current authority before every `Cursor::next`, so revocation between candidate collection and preparation reaches neither source/context inspectors nor pair inference. The new regression checks both counters remain zero.
- Independently reran `cargo test -p engram-integration --offline --features sqlite,fastembed,cross-encoder-rerank --test code_evidence`: **15 passed**, including known Git revision preservation, zero-inspector revocation and numeric fallback counts.
- Independently ran `cargo test -p engram-store-sqlite --offline --test code_source indexed_revision_is_authorized_current_metadata_with_document_source_fallbacks`: **1 passed**, covering version precedence, unknown metadata, wrong requester/scope and updating source.
- Actual-model v2 probe was still underway at focused review time; this closure relies on source review and focused deterministic tests, not an unobserved new live artifact.

## Initial findings (both resolved)

1. **P2 — known indexed revisions are discarded.** `core/integration/src/code_evidence/assembly.rs:20` sets `indexed_revision: None` unconditionally for primary source references, excerpts and rerank ranges. The spec's Current source section requires indexed revision when known. Git-backed ingestion persists that information in source metadata (`revision`) and Git source documents can carry it in `version`. Current packets therefore report an unknown revision even for sources whose indexed provenance is known. Carry the authorized source/document revision through assembly, including supporting/context references as appropriate, and retain None only when genuinely unavailable. Add a fixture with a known indexed revision and verify all references preserve it without substituting current disk HEAD.

2. **P2 — fallback erases prepared-but-unscored work.** `core/integration/src/code_evidence/ranking.rs:214` returns `fallback()` after preparation/inference/validation errors, while `fallback()` resets `views`, `scores` and continuation state to empty/zero. `assembly.rs` derives `prepared_scoring_views` from `ranking.views.len()`, so a failed call can prepare and submit 50 pairs yet report zero prepared views, and there is no separate prepared-but-unscored counter. This contradicts the accepted T8 plan's explicit omission/work units. Preserve only safe numeric preparation/attempt counts across fallback, never scores, source text, ranges or partial success. A malformed-score or injected inference-failure regression should assert nonzero preparation and zero successful scored pairs/score stamps. Keep intentionally discarded retry-attempt counts separately defined.

## Independently checked behavior

- Preflight rejects the fixed input byte bounds before tokenizer/scorer use, checks the embedding query window and scorer query count before vector inference, and checks locality before invoking injected scorer inspectors.
- Implementation/supporting partition precedes scorer pool selection. Existing one-parent-first/round-robin original-segment selection is reused; continuation views are generated lazily within the total pair cap, use unique internal IDs and exact UTF-8 source subranges, and retain the original embedding segment mapping.
- Successful scores must be complete and finite; failed scoring discards all partial scores and restores the full original bounded RRF parent ordering. Successful parent aggregation uses maximum segment/view score, keeps stable ties and excludes unscored parents from cross-encoder ranking.
- Winning-view collapse selects one successful view per original segment. Emitted original embedding sources/coverage remain distinct from exact rerank sources/context/coverage. Source excerpts derive from canonical bytes without prefix truncation.
- Fresh resolution checks the complete canonical segment/parent and all original context donors before inference and after assembly. Index revision changes or policy revocation trigger at most one whole-pipeline retry; persistent change returns an empty insufficient packet rather than rebinding stale scores.
- Calibration binding includes effective request settings, actual embedding/scorer identities, query/passage/tokenizer contracts and the versioned scorer-view rendering contract. Validated calibration has a sealed finite threshold and a non-placeholder development digest; strategy mismatch, actual fallback, missing/stale identity, failed lanes and reindex requirements prevent validated positive results. Threshold filtering precedes diversity.
- Assembly counts emitted excerpt/context tokens including duplicated context fields, enforces packet and array bounds, drops a whole winning record if it cannot fit, and computes positive status from final emitted primary items. It never retains a lower view with the previous maximum score after dropping the maximum view.
- The actual T8 artifact deliberately has no calibration, reports insufficient evidence, records real BGE/Jina execution and exact excerpts, and claims no feature/quality completion. T9 transports and T10 quality remain pending.

## Independent tests

- `cargo test -p engram-integration --offline --features sqlite,fastembed,cross-encoder-rerank --test code_evidence`: **13 passed**.
- `cargo test -p engram-integration --offline --features sqlite,fastembed,cross-encoder-rerank --lib code_evidence::assembly::tests`: **2 passed**.

The initial tests covered most core invariants. The additional focused regressions recorded above cover both initial findings.
