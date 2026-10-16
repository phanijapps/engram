# Assessment pre-release path correction — adversarial review

2026-10-15. Read-only review of the bounded assessment path fix, release guards and focused synthetic tests. No actual held-out query/judgment/result, model/config changes or model/evaluator execution.

## Path fix — clean

`assessment` now converts JSON-deserialized artifact path strings to Path before calling the real Path-based sha256 helper. Adjacent persisted store identity checks already perform this conversion. Other inspected sha256 call sites supply Paths from argparse/explicit Path construction or filesystem-derived artifacts. The new synthetic fixture writes actual files and serializes the freeze through JSON, retains the real hash helper, proves altered bytes reject before marker creation/inputs selection, and proves restored bytes reach a mocked held-out selector only after creating the exclusive marker. This avoids the original all-hashes-mocked blindspot while keeping actual labels out of the test.

Preserving the original v1 freeze/report and rerunning the fixed complete development workload into v2 is appropriate: changed runner identities must not be rebound onto an older report. No actual held-out release is inferred from the initial preflight failure.

## P2 — failed negative can inflate rejection count

`code_evidence_assessment.py::summarize` counts any negative carrying evidenceStatus insufficient_evidence, including failed=True structured cases caused by invalid calibration. The aggregate allVariantCallsValid gate independently prevents overall falsepass, but the displayed negative rejection count and heldoutNegatives18of20 subgate can still be inflated by invalid responses.

Require a nonfailed case for negativeInsufficient while retaining failed negatives in negativeCount. Extend the existing denominator test with a failed negative that explicitly carries insufficient_evidence; assert zero contribution from it and unchanged denominator. This is failure accounting, not threshold/config/label tuning, and should be corrected before the v2 runner freeze.

Verdict: bounded path fix clean; close the reporting P2 before final construction signoff of the corrected assessment helper.

## Final reporting closure

Read-only recheck confirms summarize requires a nonfailed negative before counting insufficient_evidence and retains all negative cases in the denominator. The focused summary regression now supplies the previously missed failed+insufficient structured case. The existing finish_metrics fixture also exercises the real subgate with 17 valid insufficients, one failed insufficient and two candidate responses: denominator20, valid rejection numerator17, 18-of-20 gate false. The reported full29 synthetic suite is green. No target/config/label changes or actual held-out release; v1 artifacts remain preserved.

**Clean — ready to commit.** Scoped path and failure-accounting findings are closed. Full v2 development/freeze/assessment execution and feature outcome evidence remain separate pending work.
