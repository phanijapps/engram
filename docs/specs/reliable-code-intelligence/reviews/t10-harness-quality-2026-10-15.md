# T10 harness quality/testability review

Independent read-only review of the five code_evidence Python evaluation modules, their synthetic tests, label-free Rust observer, Rust metric CLI, and approved predeclared T10 design. No actual held-out label entries, queries or retrieval results were opened. No production edits, real-store operations or benchmark/model runs performed. Synthetic tests use temporary data and synthetic query strings only.

## Findings

**P1, resolved during review — freeze omitted durable Tantivy state.** Initial logical_store_identity bound SQL and vector shadow rows but ignored STORE/lexical, even though the configured lexical lane reads that file-backed namespace. A synthetic lexical/meta.json mutation left the store identity unchanged. The corrected implementation now binds the complete lexical file/directory inventory, content hashes and lengths alongside SQL/vector rows; initial and final assessment checks inherit this coverage. Independent synthetic suite rerun: 22 tests pass, including metadata mutation, segment addition/removal, vector blob mutation and WAL/checkpoint stability.

**P2 — verify the runtime development rejection target before freeze.** development() validates installed calibration IDs/states but does not tally/assert that the actual calibrated rerun rejects at least the fixed target (14 of 15 development negatives) per variant. The offline fit count is not an independent runtime confirmation. A threshold/filter/binding bug could leave all calibrations marked validated yet fail the stated fitting criterion; the development report can still become complete and freeze before spending the one untouched held-out assessment. Record the actual negative empty-primary/insufficient count, require the fixed negative inventory and target, and fail development before freeze on a mismatch. Do not refit against held-out outcomes.

**P2 — persist failed complete observations before restart/validation can fail again.** run_variant() calls strategy_maximum outside the exchange exception block, so a complete degraded/wrong-state response aborts development without recording that query or raw packet. assess_variant() records an error but drops the complete observed packet on validation failure; its poisoned-child branch constructs the replacement process before appending/checkpointing the failed case, so a replacement Popen failure loses the original failure record too. Retain rejected complete responses and per-child stderr, append/checkpoint each failed case before closing/restarting, and preserve that record if restart itself fails. The existing final failure gates prevent false passing quality, but the raw failure evidence is needed to diagnose a costly frozen run honestly.

**P2 — transient lock exclusions bypass the stated type checks.** lexical_identity() excludes lock basenames before testing symlink/nonregular entries. A symlink, FIFO or directory named .tantivy-writer.lock/.tantivy-meta.lock is silently skipped despite the declared symlink/nonregular rejection rule. Apply exclusion only to regular transient lock files after validating entry type; keep all other entries bound. This is a narrow closure item on the otherwise correct Tantivy fix.

## Sound construction

The harness separates label-free runtime observation from the offline judgment/metric process. Canonical judgments require exact path/anchor/declaration/hash identity. Packet excerpts are checked against pinned UTF-8 source bytes before range coverage. Candidate Recall@50 consumes distinct pre-score eligible implementation parents, and actual validated final packets control final metrics. Baseline placeholders retain raw rank slots and cannot create excerpts from entity headers. Rust metric computation is reused rather than duplicated.

Development threshold candidates and f32 edge cases match the predeclaration, preserve the full negative denominator and reject mismatched strategy/state/reindex/lane failure; historical code_sources warnings remain separate. Binding consistency and same-candidate segmented/balanced comparison are explicit. Immutable artifacts/configs/executables and exact runtime roots/path sets are bound before held-out release. Assessment marker exclusivity prevents silently repeating an untouched run. Full variant inventories, all-call validity, baseline startup/warmup and fixed-nearest-rank 60-call MCP operational gates prevent the previously reported false pass.

JsonProbe poisoning/reaping prevents a timed-out response from being relabeled as the next query. The synthetic delayed-child test meaningfully exercises an actual child process. Work remains serialized under the shared machine lock; observer timing is separate from actual MCP timing. Assessment quality failure leaves featureShipped=false rather than retuning. The observer smoke is explicitly nonbenchmark and cannot satisfy held-out/p95 gates.

## Independent checks

Ran python3 -m unittest discover -s scripts/dev/tests -p 'test_code*eval*.py' -v: 22 passed, including prior adversarial regressions and the new lexical freeze checks. Independently reproduced the initial missing lexical-state identity with a temporary SQLite/lexical fixture before its fix. git diff --check passes. The parent/other reviewer report seven pure Rust evaluator tests passing; no redundant heavy builds run here.

Current verdict: close the three remaining P2 items before a clean harness/development-ready checkpoint. No held-out execution is required for these fixes.

## Final focused closure

All three remaining P2 findings are resolved. verify_calibrated_development requires the exact ordered runtime case inventory, fixed 15-negative set, validated installed artifact ID/state and at least 14 actual empty-primary/insufficient negatives per variant. It records runtime counts; freeze independently recomputes them against selected development entries and compares with the recorded result. The synthetic regression distinguishes validated IDs from actual rejection, reproduces 13/15 failure, and rejects a missing case.

Development now retains rejected complete observations before aborting. Assessment checkpoints a failed case plus any complete observation before metric processing or child teardown/restart, retains each child stderr record, and explicitly records replacement-constructor failure. The regression covers both rejected complete packets and a poisoned protocol child followed by Popen failure, confirming persisted failure/child records remain available. Poisoning still prevents a late old response from being relabeled.

Lexical exclusion now applies only after entry-type validation. Symlink, FIFO and directory entries with either transient lock name reject; regular declared lock files remain the sole exclusions. The added real temporary-filesystem regression covers these types without scanning any evaluation corpus.

Independently reran the complete selected Python construction suite: 26 passed in 0.453 seconds. git diff --check passes. No actual development/held-out model or benchmark run was performed, and no actual held-out query/judgment/result opened.

Final verdict: Clean — ready to commit the T10 harness and begin the predeclared development phase. This does not approve a calibration value, permit held-out retuning, establish quality/p95 outcomes or mark the feature shipped.
