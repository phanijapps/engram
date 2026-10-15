# Development rank/threshold diagnosis — evidence review

Verdict: verified development-only attribution; no acceptance/shipping verdict.

Independent agent review, read-only. Reviewed /tmp/reliable_development_rank_diagnosis.py and results/t10-development-rank-diagnosis-v1.json, then independently recomputed its rows from completed v1 development packets and exact canonical judgments. Only inputs(args, 'development') was selected. The shared loader hashes/parses the combined JSON envelope and reads split discriminators, but does not traverse unselected held-out query/judgment fields. No model calls, retrieval queries or store writes were performed; canonical SQLite reads used mode=ro. Existing v1 artifacts were not changed.

## Verified arithmetic

All 90 positive rows (30 per variant), completed-development SHA256 and final reviewed manifest SHA256 match. Required sets resolve exact source path, qualified anchor, declaration range and source hash to grade-3 canonical parents; there are 34 required units over 30 answerable development queries.

| Variant | Candidate query-macro recall | Raw top-ten inspection query-macro recall | Calibrated top-ten query-macro recall | Required units candidate / raw / calibrated | Queries losing raw required units | Units lost below threshold |
| --- | ---: | ---: | ---: | --- | ---: | ---: |
| Segmented, no scorer | 0.900000 | 0.783333 | 0.066667 | 29 / 25 / 2 of 34 | 22 | 23 |
| Balanced, real scorer | 0.900000 | 0.800000 | 0.533333 | 29 / 25 / 16 of 34 | 8 | 9 |
| Fast | 0.833333 | 0.666667 | 0.133333 | 27 / 22 / 4 of 34 | 16 | 18 |

These are arithmetic means of per-query required-unit recall, matching the diagnosis definitions. They are not pooled-unit ratios: for example balanced raw 25/34 is approximately 0.7353, whereas its query-macro mean is 0.80. Preserve the macro label when reporting the supplied .90/.80/.5333 numbers.

## Attribution strength

For every positive pair I checked identical observed candidate arrays and diagnostic configHash, valid final calibration identity, unvalidated raw state, identical source-failure lists, no reindex requirement and no failed-case flag. All 135 raw/calibrated development pairs additionally have identical actual runtime binding and sourceRevision. Balanced uses actual ready cross-encoding; other variants use disabled/no reranker. Common retained parents have identical raw/final relevant scores across all 90 positive pairs.

All raw top-ten required parents missing from calibrated top ten have raw scores strictly below the installed strategy-specific threshold. Balanced's threshold is -0.97909373; all nine missing required units in eight queries fall below it. No lost raw required parent is at or above threshold. The production assembly explicitly applies parent.score < threshold before output/diversity caps. This supports the diagnosis that threshold filtering accounts for the observed raw-inspection-to-calibrated loss in these saved development calls, rather than candidate/config drift, source-race revocation, model fallback or score reassignment.

The saved calibrated runtime checks independently reject 14/15 development negatives for each variant, matching the predeclared fit objective. Balanced's relevant-unit scores overlap the negative-query score distribution at that operating point. Its successful ranking/inspection performance therefore does not establish adequate evidence discrimination under the required negative rejection setting. RRF-only variants suffer still larger threshold losses; their fusion score is not a semantic probability.

## Root cause for final evidence reconciliation

On the completed development run, balanced retrieval finds many relevant parents and ranks them better than the validated packet recall suggests. The fixed negative-rejection threshold then filters nine of the 25 required units visible in raw top ten, reducing query-macro recall from 0.80 to 0.5333. This is a measured score-separation limitation at the predeclared calibration operating point. The threshold does not explain the earlier candidate-to-top-ten loss (.90 to .80), the candidate misses, or required units never present in raw top ten.

## Limits

- Raw packets are explicitly unvalidated inspection output. Their ranks cannot replace calibrated acceptance metrics or satisfy a positive-quality gate.
- The diagnosis measures parent identity presence, not decisive excerpt-range coverage or complete functional answers. Final metric/citation review remains necessary.
- Matching saved inputs and scores supports this specific observed filtering attribution; it is not a new model experiment or proof of generalization. No counterfactual threshold/profile/model change was evaluated or recommended.
- Counts cover the development split only. They predict neither held-out targets nor operational p95 and do not justify changing frozen labels, targets or profiles.
- V1 remains historical diagnosis. Fresh v2 under the corrected, strictly frozen harness must supply its own development/assessment evidence; no report rebinding or score substitution is implied.

Reproduction used a separate read-only checker /tmp/reliable_development_diagnosis_review.py; the original diagnosis generator was not executed because it overwrites its report. No new construction defect was found in the diagnosis arithmetic. Its headline should retain the macro/inspection qualifiers and distinguish the threshold loss from earlier candidate/rank losses.
