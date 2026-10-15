# T10 harness independent adversarial review

Reviewer: `/root/code_eval_blind_review` (agent, not human). Read-only review of the predeclared T10 design, label-free Rust observer, Rust metric bridge, five Python harness modules and synthetic tests. No actual held-out label entries, query texts, retrieval results or quality outputs were opened. Only the explicitly named public T9-query observer smoke artifact was reviewed. No production edits or held-out assessment were performed.

## Verdict

Clean — ready to commit the T10 harness and begin the predeclared development phase. All findings below are resolved. This verdict does not approve any calibration value, held-out outcome or feature shipment.

## Final focused closure

Re-read the timeout/protocol correction. `JsonProbe` is permanently poisoned after timeout, EOF, write failure, malformed JSON or non-object JSON and refuses another input before writing to the child. Closing also poisons it. On a desynchronizing protocol failure, `assess_variant` retains the failed case and zero metrics, closes/reaps the old child, and creates a fresh process before the next query. Late output from a previous child cannot become another query's observation. Legitimate complete helper error responses remain distinguishable from protocol failures.

Independently reran `python3 -m unittest discover -s scripts/dev/tests -p 'test_code*eval*.py' -v`: **20 passed**. This includes the real synthetic Python-child test: a delayed first result times out, the poisoned child refuses the second write, and a fresh child returns the second query correctly. No held-out inputs were opened or assessed.

## Late finding (resolved)

- **P2 — a late response after a JsonProbe timeout could be assigned to the next query.** The initial `assess_variant()` caught per-case exceptions and continued using the same uncorrelated process. A late result could be dequeued for the next case and judged against the wrong labels; binding IDs intentionally identify representation/config rather than query text and could not detect the shift. The all-call failure gate prevented a passing assessment, but subsequent per-query records could still be misleading. The poison/reap/restart correction and delayed-child regression described above resolve this issue.

## Initial findings, resolved on focused re-review

1. **P1 — freeze checked different paths from those executed.** The original assessment verified arbitrary recorded absolute artifact/store paths but executed paths independently derived from `args.output`/`args.base`; copied freezes could validate untouched originals while consuming modified copies and using a fresh marker. The fix binds exact resolved output/base roots, assessment marker, expected artifact path inventory and all six actual store paths before any held-out release. Copy/root/path mismatch and repeated-marker tests pass.

2. **P1 — baseline/fast failures did not invalidate the overall result.** Only balanced/segmented failure rows were gated. A synthetic label-free 72-case report with every baseline and fast row failed still produced `allQualityAndWarmGatesPass=True`. The fix checks complete ordered inventories and call validity for every variant, plus baseline startup/warmup validity; synthetic otherwise-passing reports now fail for baseline, fast or startup failures. Failure rows remain in their fixed metric denominators.

3. **P2 — development traversed held-out judgment fields before freeze.** The initial shared loader decoded and validated every label entry before split selection. The new T10 `selected_frozen` hashes the immutable combined envelope, selects only split metadata, and verifies/traverses only selected judgments. The plan now accurately distinguishes unavoidable combined JSON-envelope materialization from semantic selection/use: unselected query/judgment fields are not traversed, verified, evaluated, emitted or supplied to models. A malformed unselected sentinel passes development-only loading without being touched. No evidence was found that the original fitting calculation used held-out labels; this was an isolation/claim mismatch.

## Methodology and implementation checks

- The runtime observer receives query DTOs and optional immutable calibration only; expected labels stay in the offline Python process and pure Rust metric helper. No judgment or expected range is sent to retrieval/scoring.
- Candidate Recall@50 uses the first 50 distinct eligible implementation parents in the independent pre-score stable RRF observation, not the scorer pool or post-threshold packet. The observer records source/index revision and confirms fused-count consistency with the operation; store identities fence mutation over assessment.
- Canonical judgments require pinned path, qualified anchor, exact declaration span and source hash. Ambiguous/missing identities fail rather than being replaced by nearby symbols. Actual excerpt bytes, unit IDs and source hashes are verified against frozen corpus bytes before counting range coverage.
- The metric bridge reuses the accepted Rust evaluator. Unvalidated positive items cannot earn final recall/ranking/range credit. Unjudged or ambiguous legacy headers retain original top-ten rank slots; baseline entity headers supply no source excerpts and candidate/phase metrics remain explicitly unavailable.
- Threshold fitting uses the full fixed development negative denominator and actual strategy score space. It selects the least restrictive finite f32 threshold satisfying the predeclared rejection target, with nextUp tie handling, signed zero/subnormal behavior and finite-overflow failure. No arbitrary grid or held-out threshold search appears in the implementation.
- Real calibrated development reruns verify the installed artifact ID and validated state; fit diagnostics alone are not promoted to validated runtime quality. Degradation, wrong scorer space/state, reindex requirements and nonhistorical lane failures abort fitting.
- The freeze binds scripts/executables, configuration/calibration artifacts, manifest and logical persisted SQLite/vector shadow rows, independently of WAL/checkpoint layout. Model/tokenizer/input identities enter the actual runtime calibration binding; pinned model loaders separately verify their bytes.
- The held-out assessment marker is created exclusively before semantic release; failed/incomplete attempts remain recorded and cannot be silently resumed as another untouched run. Automatic recovery from a failed quality gate does not retune.
- Warm latency comes from real balanced MCP calls, not the double-collection observer; the predeclared 60 requests, nearest-rank p95 and startup/warmup/failure checks are explicit. Packet parity excludes timing and normalizes only declared f32 score bits. Feature shipment remains false in the harness.

## Independent verification

- Initial synthetic Python construction suite: **10 passed**.
- Focused re-review suite, `python3 -m unittest discover -s scripts/dev/tests -p 'test_code*eval*.py' -v`: **19 passed** (13 new harness tests plus 6 frozen-corpus tests).
- `cargo test -p engram-eval --offline --test code_intelligence`: **7 passed**.
- Independently reproduced the original baseline/fast failure-gate false positive with synthetic data only; the new regression covers the correction.

This is harness readiness review, not approval of any threshold, source label, held-out outcome, beta claim or operational performance result. Human source/citation review remains separately required by the spec.
