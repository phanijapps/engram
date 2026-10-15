# T11 actual self-store rollout — security preflight

Date: 2026-10-15
Verdict: **Clean — ready for gated execution.**

Scope: read-only review of `/tmp/reliable_self_rollout.py` and the relevant process/config/packet-verification contracts. The script was neither executed nor imported. No assessment/held-out inputs, model calls or actual-store writes were performed.

## Resolved High — optimized Python removes the mutation gates

Lines 12–26 enforce quality acceptance, freeze/executable/config/calibration identity, backup preservation, unchanged actual-store identity and output nonoverwrite entirely through `assert`. Python `-O` or inherited `PYTHONOPTIMIZE` removes these checks, allowing the actual-store force scan without the required prerequisites. Post-mutation verification assertions disappear too. Use explicit exceptions for required gates, or reject optimized execution unconditionally before imports/any side effect (`if not __debug__: raise ...`). The guard itself must not be an assertion.

Resolution: the script now raises an explicit RuntimeError when `not __debug__` before helper imports, assessment reads or store opens. Inspected the recorded optimized-execution negative test; it exits at that guard. Required assertions therefore cannot be silently removed during an admitted run.

## Resolved Medium — inherited code environment can override the frozen launch artifacts

`EvaluationServer` inherits the parent environment. The runtime resolver applies ENGRAM_CODE* environment settings above the explicit code config file. The rollout hashes the frozen balanced file but does not reject this environment namespace or verify actual resolved preparation settings before the force scan. It can therefore mutate/reindex the self store using settings different from the declared fast preparation profile, only discovering the mismatch after mutation or during calibrated evidence calls. The existing evaluation entrypoint already rejects inherited ENGRAM_CODE* variables; apply that same fail-closed preflight here, before any actual-store server opens.

Resolution: the next unconditional preflight rejects every inherited key beginning ENGRAM_CODE. Inspected the sentinel negative-test log; it exits before helper imports, assessment reads or store opens.

## Resolved Medium — aggregate excerpts do not establish four positive self probes

The evidence loop requires only validated calibration per packet and one aggregate excerpt across all four queries. Three empty/insufficient packets would still satisfy line 115. For the declared four positive self-verification probes, require per-query primary items, successful actual scorer diagnostics/scored pairs and current exact excerpts, or mark missing outcomes explicitly as unmet instead of declaring the rollout verification complete. Calibration state alone does not mean evidence survived threshold/output limits or that a scorer processed any candidates.

Resolution: each probe now records byte-verified excerpt and primary counts, requires nonempty primary evidence and `candidates_available`, and checks validated calibration plus ready/effective cross-encoder state. Successful cross-encoder primary items originate only from completed scorer views in the shared operation. An empty or fallback self probe cannot produce a complete rollout report.

## Other reviewed controls

The script selects the authorized actual self store explicitly, hashes the assessed executable and immutable balanced artifacts, verifies actual/backup logical identities before mutation, supplies an explicit exclusion filter, bounds reindex calls, compares canonical/history identities across derived work and unchanged incremental scan, verifies old canonical JSON either unchanged or exactly archived, preserves memory/event/history rows, verifies exact source bytes through the existing packet observer and checks an unrelated-scope empty control. Failure records incomplete state and closes child servers; it does not pretend rollback exists. Actual source policy and scope enforcement remain in the shared Rust operation.

Final re-review found no remaining scoped blocker. This preflight does not claim a passing quality assessment, a completed rehearsal or actual-store safety proof. Those data were not opened. No additional user confirmation is required for the already-authorized self-index/vector work once the mandatory gates pass. The reviewer did not run or import the rollout script; only source and the two recorded guard-negative logs were read.
