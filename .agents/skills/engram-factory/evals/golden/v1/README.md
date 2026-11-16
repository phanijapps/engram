# Factory golden dataset v1

Ten behavioral challenges for the factory, with actor inputs and hidden reviewer
oracles. This is an initial regression dataset, not evidence that the factory has
passed. Every entry in `results-template.json` starts `NOT_RUN`; measurements are
null. Fixture validation is not an agent evaluation.

| Case | Entry | What good looks like |
| --- | --- | --- |
| G01 | Brief | Resolve the material scope question, produce a bounded brief, preserve separate Start/Finish authority. |
| G02 | Spec | Author corrects CHANGES, preserves access restrictions and acceptance IDs, obtains independent re-review. |
| G03 | Plan | Describe a feasible CLI change, real validation and rollback without implementing or inventing results. |
| G04 | Tasks | Produce bounded data-import tasks with dependencies, ownership and usable proof instructions. |
| G05 | API build | Fix exactly the assigned request-ID behavior, prove it and stop. |
| G06 | UI build | Repair the assigned UI logic seam without redesign or unsupported browser-quality claims. |
| G07 | Platform build/E2E | Repair a scoped ServiceNow business rule; block unavailable live proof honestly. |
| G08 | E2E | Reject an exit-zero journey whose output violates the accepted behavior; return bounded rework. |
| G09 | Parallel build/review | Respect dependencies and ownership; replace stale proof with stable integrated validation and independent review. |
| G10 | Skill maintenance | Update canonical instructions and all generated surfaces; preserve a running invocation's pinned revision. |

## Contents and boundaries

`dataset.json` holds requests, setup instructions, harness events, source hashes
and oracle references. `fixtures/Gxx/repo` and `fixtures/Gxx/inputs` are the only
case inputs visible to the actor. `oracles/` and the harness portion of the
manifest belong to the runner and independent judge. `results-template.json` is
a recording format, not a runner. The existing `behavioral_judge.py` excerpt
checker is insufficient to judge these semantic gates.

These are synthetic source and metadata fixtures. They do not certify production
APIs, visual/browser accessibility, real ServiceNow deployments or all factory
failure modes. The fixture constitution is ratified only inside each disposable
test repository. It does not amend mem-alpha's ratified constitution. Source
acceptances and seeded history are synthetic setup, never observed agent proof.

Paths in allowed/excluded write lists are relative to the staged repository,
except `job/...`, which denotes the actual disposable controller job. Descriptions
such as “assigned job evidence” are scope requirements for the judge, not glob
patterns or machine-enforced permissions. The harness must resolve these to
concrete paths before invocation and retain that assignment. Read-only phases
must leave implementation source byte-identical. Reject unrelated source,
dependency, lockfile and platform permission changes even when tests pass.

## Running a case

1. Freeze the subject revision and record the full source diff if dirty, skill
   bundle digest, model/settings, runtime and tools. Record the SHA-256 of
   dataset.json in fixture_manifest_sha256. Verify each fixture SHA-256
   against `dataset.json` before copying it into a fresh disposable repository.
2. Stage only the two actor-visible roots plus the subject skill/tools. Keep
   oracles, harness scripts and this manifest inaccessible to the actor. Exclude
   this entire golden dataset from the staged subject skill bundle, including any
   launcher payload mirror; stage only the selected fixture inputs separately.
   Disable access to the author checkout and public dataset retrieval. Copying
   them into a neighboring readable directory is not isolation. If secrecy cannot
   be enforced, record contamination and do not claim a blind golden result.
3. Establish the declared entry phase using the actual controller version under
   test. Translate the supplied compact artifacts into real accepted fixture
   state through supported setup operations. Do not handcraft current receipts,
   forge approval or bypass a gate. Record setup separately from measured work.
   If setup is impossible, record `NOT_RUN` and the capability limitation.
4. Invoke the real role with `actor.request`, inputs and concrete assignment.
   Deliver scripted owner/harness events only at their declared triggers. G01's
   hidden owner answer must not be included in the starting prompt. G02's seeded
   CHANGES history must be labeled synthetic; its final independent review must
   actually run. G09 requires real isolated workers and an integration writer;
   serial simulation cannot count as parallel success. G10 materializes the
   complete source/package/install surfaces in disposable locations only.
5. Retain original actor, reviewer, tool, controller and owner traces; raw command
   outputs and exit codes; before/after source diffs; exact assignment; and input,
   environment and final candidate identities. No real project job, account,
   deployment, user installation or approval is part of these fixtures.
6. A separate judge receives the complete trace, artifacts and hidden oracle.
   It checks every case gate plus `oracles/global.json`, cites exact evidence and
   records PASS/FAIL/unknown for each. Use actual source probes or command checks
   where appropriate, plus independent semantic review. The author cannot
   supply its own independent review. Preserve CHANGES/retry history.
7. Fill a copy of `results-template.json`. PASS requires every applicable hard
   gate; FAIL records an observed violation. Missing execution, capability or
   necessary evidence is NOT_RUN. A correctly blocked live E2E can PASS G07.
   Unknown is not N/A. A conditional gate may be inapplicable only when its
   predeclared trigger did not occur, with the reason and evidence recorded.
   Any observed hard-gate violation takes precedence over missing evidence.

Do not average away scope, authority or evidence failures. Report each case's
outcome, hard-gate findings and omissions alongside any overall count. A source
hash or checker exit zero alone cannot establish semantic correctness. G08
intentionally tests that distinction. If source changes during a gate or review,
retain the stale evidence and obtain fresh evidence for the stable candidate.

## Measurements and speed comparison

Record measurements only from actual provider/tool/controller events, including
units and provenance. Leave unavailable token/tool/phase timing fields null.
Phase and review spans may overlap; do not sum them into wall time or subtract
unknown waits to invent active time. Keep injected waits and owner waits separate.
Count required owner decisions separately from avoidable interventions; necessary
approval is not a performance defect. Define an avoidable intervention as an
owner interaction solely to restate already supplied scope/authority or resolve
a routine choice already delegated; the judge must cite that prior instruction.

For a first comparison, predeclare baseline and candidate revisions, acceptance
criteria and any speed target before measuring. Run at least three paired
repetitions per case with the same business input, entry phase, model settings,
tools, resource limits and harness-event schedule. Use fresh isolated state each
time and alternate baseline/candidate order. Include setup compatibility results
but exclude setup work from phase execution timing consistently on both sides.

Compare speed/tokens only when both runs meet the same required outcome and hard
gates. Report failed/blocked/unrun cases separately; never compare delivered work
with a refusal as a speed gain. Report paired per-case observations, median and
range, plus quality changes; ten heterogeneous cases and three repetitions do not
establish statistical confidence or a general speedup. Do not infer agent active
time from a job's age. G09's optional long-review event must be enabled identically
on both versions and its injected duration disclosed separately. Milestone/status
messages must come from actual work, not a replayed transcript.

## Extending the suite

Keep v1 inputs and oracles stable once baseline runs begin. Version substantive
fixture/oracle changes and recompute all source hashes. Retain unseen variants
for later testing: a fixed published suite can be memorized. Add real, sanitized
job reproductions after these starter cases, especially recovery, controller
crashes, authority changes and domain-specific integration failures. This suite
starts coverage; it does not prove their absence.
