# Assessment preflight path and failed-negative quality review

Verdict: Clean — ready to commit.

Independent agent review, read-only. Scope: scripts/dev/code_evidence_assessment.py and its synthetic regression tests before fresh v2 execution. No held-out label/query/result data was opened, no inference or evaluation queries ran, and no production/store files were changed by this reviewer.

## Closed findings

1. **Preflight path type, corrected.** JSON object keys deserialize as strings, while code_eval_corpus.sha256 requires a Path exposing read_bytes(). Wrapping each frozen artifact key in Path fixes the actual failure without changing artifact selection or accepting a different hash. The new synthetic test uses JSON-round-tripped string keys and real file bytes/hashes instead of mocking sha256. Drift rejects before marker creation or inputs(..., 'heldout'); restored valid bytes reach the mocked release boundary only after the marker is created. Existing root/path-inventory/store-identity/repeated-marker checks remain before semantic release. The supplied red log reproduces the exact AttributeError, and the green test exercises the real contract.

2. **Failed negative success credit, corrected.** summarize() previously counted any negative with insufficient_evidence, even failed/unvalidated calls. allVariantCallsValid already prevented a false overall pass, but the negative numerator and its separate acceptance gate could be overstated. The corrected numerator requires not failed while retaining every negative in the denominator. The existing denominator regression now gives a failed negative an insufficient status and still requires only one success. The complete synthetic finish_metrics fixture separately checks 17 valid insufficient + one failed insufficient + two other negatives: denominator 20, success count 17 and heldoutNegatives18of20 false. This tests the individual quality gate, not merely the pre-existing overall invalid-call rejection.

## Independent verification

- Full synthetic command `python3 -m unittest discover -s scripts/dev/tests -p 'test_code*eval*.py'`: **29 passed**, after both corrections.
- Reviewed /tmp/reliable-assessment-path-red.log and /tmp/reliable-assessment-path-green.log.
- File-presence-only check of results/t10-evidence-v1: freeze.json exists; assessment-started.json and assessment.json do not exist. No v1 report/label/query contents were opened. This is consistent with the recorded pre-marker/pre-held-out failure and the source ordering.

The strict fresh-v2 approach is appropriate: preserve v1 artifacts, rerun development under the changed harness identity and freeze the new inputs; do not rebind the old report or bypass identity drift. The fixes change harness correctness, not metrics definitions, labels, profiles or gold targets. This is a scoped harness signoff only; actual fresh development, single held-out assessment, operational and retained-store outcomes remain pending.
