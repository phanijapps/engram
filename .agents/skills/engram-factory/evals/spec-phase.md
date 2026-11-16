# Spec-phase evals

Run `python3 -m unittest discover -s evals -p test_spec_phase.py` from the skill.
Tests use synthetic content and disposable Git repositories, not owner jobs.

| Behavior under test | Expected observation |
| --- | --- |
| Single change authored once | Validate canonical content/rules; no authored hashes or readiness JSON |
| Broken/duplicate/missing requirement and AC links | Preparation rejects the candidate before review |
| Multi-surface coverage | One table validates scope, journey links and state coverage; no parallel indexes |
| Missing exclusion authority, navigation, negative state or review condition | Structural rejection; unknown is not N/A |
| Valid comparisons, HTML and generic type names | Accepted content; not mistaken for template placeholders |
| Noisy output | Default template has no machine arrays, methodology or repeated gate/coverage map |
| Unchanged preparation | Authored bytes unchanged and receipt bytes identical |
| Full review handoff | Spec and receipt freeze together; independent PASS permits analysis completion only |
| Changed spec or wrong source base | Preflight rejects stale identity |
| Tampered generated format/reviewer/readiness fields | Rejected before freeze; receipt cannot hide itself or override authored choices |
| CHANGES correction | Prepare revised bytes and freeze candidate 2 without resetting review history |
| Pending/PASS candidate edits | Cannot rewrite its receipt to inherit the review |
| Malformed receipt shapes | Both preflight and preparation emit clear errors, no traceback |
| Unsupported spec format | Preparation and preflight reject it; original bytes remain unchanged |

Independent content evaluation remains separate from deterministic checks. For
an actual agent-authored candidate, supply its brief, source revision and source
inventory to a reviewer independent of its author. Require cited findings on:

1. Requirements and ACs express the owner's outcomes, including meaningful
   failure behavior, without unauthorized scope additions or exclusions.
2. Source inventory and rule applicability are complete for the actual change;
   evidence, authority and N/A reasons are valid rather than merely present.
3. Each behavior/AC is defined once, links resolve, and prose adds decisions or
   explanation rather than restating tables, hashes or template instructions.
4. Format is proportional: small changes have one scenario and only relevant
   content; multi-surface work retains necessary navigation, states and review
   conditions. Code/test plans belong to later phases.
5. Receipt and review identify the same authored bytes; preparation is neither
   semantic acceptance nor owner Start/Finish.

Record actual reviewer evidence and unresolved findings. Passing the automated
suite does not demonstrate a live author's quality or a measured speedup. The
included fixture is synthetic, not a finished owner specification.
