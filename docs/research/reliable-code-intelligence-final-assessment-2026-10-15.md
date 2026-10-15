# Reliable code intelligence: completed assessment, not shipped

The single frozen assessment completed on 2026-10-15. All 356 measured calls
completed without a call failure: 74 each for historical baseline, segmented,
balanced and fast, plus 60 actual balanced MCP requests. Quality targets failed;
the reliable profile is not beta-ready and the retained self-store rollout was
not executed.

The [machine-readable outcome](reliable-code-intelligence-final-assessment-2026-10-15.json)
records the raw report/freeze hashes, executable/runner/configuration/calibration
identities, observed model/representation bindings, all summaries and gates.
The raw report is
`~/.engram/evaluations/reliable-code-intelligence/results/t10-evidence-v2/assessment.json`.

| Balanced acceptance measure | Observed | Required | Outcome |
| --- | --- | --- | --- |
| Held-out candidate Recall@50 | 92.5% | >=90% | Pass |
| Held-out required-unit Recall@10 | 60% | >=80% | Fail |
| Engram required-unit Recall@10 | 60% | >=70% | Fail |
| Flask required-unit Recall@10 | 73.33% | >=70% | Pass |
| Vite required-unit Recall@10 | 46.67% | >=70% | Fail |
| Original questions with every required anchor | 1/10 | >=8/10 | Fail |
| Exact-symbol controls at rank one | 2/2 | 2/2 | Pass |
| Held-out negative insufficiency | 19/20 | >=18/20 | Pass |
| Warm actual MCP p95, concurrency one | 10.800065 seconds | <=30 seconds | Pass |

Held-out balanced MRR@10 is 0.49375, nDCG@10 is 0.50779049 and required-range
coverage is 0.58317567. The approved scorer/non-scorer nDCG nonregression gate
passed. All case-inventory, call-validity, baseline startup/warm-up and frozen
store preservation gates passed. No operational success overrides the failed
positive relevance requirements.

Historical baseline held-out required-unit Recall@10 is 42.5%, MRR@10 is
0.26919643 and nDCG@10 is 0.30511392. Balanced improves these held-out aggregate
measurements, but original-question recall is 10% versus the historical 15%;
both recover all anchors for only one original question. This is not evidence
of a uniform improvement. Baseline candidate metrics are unavailable; ambiguous
legacy rendered headers remain unjudged at their original ranks. Independently
calibrated comparator thresholds also prevent interpreting this assessment as
a causal scorer-only ablation.

Construction validation remains distinct: all eleven workspace/build/contract/
boundary commands passed, with 1,189 Rust tests passed and 10 ignored. The rebuilt
native addon passed five real embedding/scorer tests; a separate actual-model
paired long-tail fixture passed both ranking checks. Disposable retained-store
adoption had zero scan errors and preserved all 86,744 old canonical records
unchanged-current or exactly archived. These proofs do not complete the actual
self-store reindex, idle-repeat and evidence battery.

Final documentation and whitespace checks pass. The separate spec-status lint
reports 15 hard violations identical to the original baseline, with zero new
hard violations, plus two warn-only contract-backlink findings for this spec.
Those diagnostics are retained in the machine-readable outcome; the global
status lint is not reported as passing.

AC10 is unmet. AC9 and the actual-self portion of AC12 remain incomplete because
rollout requires passing quality first. The AC11 aggregate negative/comparative
gates passed; recorded human label/citation review remains pending. Earlier
construction proofs retain their checkpoint identities and are not relabeled as
new final-binary four-surface measurements.

The v1 run failed a harness preflight before held-out release. Its receipts were
preserved; all 270 development calls were rerun under the corrected v2 runner
before the single v2 frozen assessment. No post-assessment threshold, model,
label or target changes were made. A further tuning round needs a separately
reviewed development/validation plan and a fresh untouched assessment set;
this held-out result must remain an outcome, not become a tuning objective.

The independent [whole-spec quality review](../specs/reliable-code-intelligence/reviews/final-whole-spec-quality-2026-10-15.md)
recomputed the aggregate metrics and p95, checked receipt hashes and confirmed
the failed acceptance verdict. Its open acceptance findings remain unwaived.
