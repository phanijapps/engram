# T6 targeted source-access performance review

Read-only review of adapters/sqlite/src/knowledge/source_access.rs, the SQLite recipe lexical resolver call, the added source-access equivalence test, and the supplied before/SQL/after phase artifacts. Other T7/scorer work and held-out labels/results were excluded.

## Finding

**P2 — The new equivalence fixture never exercises the second changed ownership query with an owned target.** The new target_access_matches_full_access_for_every_owner_without_visiting_unrelated_rows test creates two active code sources, entity contribution rows, denied source snapshots, a legacy marker and unmanaged IDs. It never creates a generic DocumentHead-owned chunk/entity/relationship, so the changed document_head_records query returns no rows for every tested ID. Its loop over three target kinds mostly compares default-allow for absent records; there is no positive relationship ownership. A regression that dropped the target-specific generic ownership query would still pass while allowing stale/private managed generic targets through the lexical resolver. Add a real generic DocumentHead-owned target, independent/shared ownership with policy equality, a denied/revoked head case, and a same-ID/different-kind control. At minimum the second changed SELECT must have both permitted and restricted positive ownership fixtures.

## Implementation assessment

The SQL filter is bound through parameters and applied to record_id only, retaining all record kinds, independent owners and historical attempts for that target. Code-source owner order is preserved. Both ownership branches share their existing authorization logic inside one read transaction: current attempt/head, state, exact scope, actor/policy/expiry, source snapshot exclusion and whole-owner policy selection. The lexical caller asks only about the requested target ID, consistent with the narrow result's documented authority. Existing full-store access callers retain their original query behavior. No production authorization defect was found by inspection.

The unrelated malformed ownership control is meaningful: a malformed relevant source owner still errors; an unrelated contribution no longer forces an unrelated target lookup to fail. It does not test malformed global manifest JSON isolation, nor does the implementation claim that isolation.

## Measurement limits

Before logs identify repeated full-store ownership reads at about 3.3–4.1 seconds per lexical hit, while raw Tantivy search is below one millisecond. Exact SQL returned 300823 contribution rows in about 2324ms; the attribution is grounded in measurement. After filtering, source access is roughly 4–6ms per hit, all 50 raw lexical hits resolve in about 285ms, and the lexical candidate stage completes in about 906ms. This is evidence for eliminating the repeated ownership sweep. The after run has no compatible active passage representation and yields zero canonical segment candidates, so it does not establish complete indexed/scored query latency or final retrieval quality. The parent explicitly discloses this limitation.

Temporary PERF timing prints remain in source_access.rs and recall_lanes.rs at this review; remove them and format before commit as already planned. git diff --check passes. No heavy build or store mutation performed by this reviewer.

Verdict: implementation is sound on inspection; close the changed generic-ownership-branch regression gap before a clean checkpoint verdict. Full live indexed candidate proof remains separate.

## Final targeted re-review

The P2 coverage gap is closed. The fixture now creates and activates a real generic document/head/paragraph through the fenced writer, with paragraph ID owner-only deliberately colliding with a separately code-owned entity ID under a different private actor. Explicit expectations check permitted vs denied chunk/entity decisions, positive generic and code relationship policies, and cross-kind separation before comparing full/filtered decisions and complete policies across actors/scopes/kinds. The generic relationship row and code relationship row exercise their respective ownership queries. Both permitted and restricted generic ownership paths now execute.

The new SourceTargetAccess wrapper keeps both its selected target ID and RecordAccess private and exposes only allows(kind)/policy(kind). The lexical caller therefore cannot accidentally ask the narrow access result to authorize an unrelated omitted ID via RecordAccess's legacy unmanaged-record default. Full-store access remains unchanged. Temporary PERF instrumentation has been removed from the reviewed production files. git diff --check passes. Parent reports code_source22/document_head5 tests passing; no heavy test/model run performed in this read-only review.

Final verdict: Clean — ready to commit (targeted ownership lookup implementation and regression coverage only). Full indexed live candidate proof remains pending and is not inferred from the zero-compatible-segment timing run.
