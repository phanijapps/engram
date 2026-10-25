# Template contract

## Writing rules (v2 — house style, enforced by reviewers and the placeholder gate)

**Line discipline.** One sentence per line. Do not hard-wrap prose — the
renderer wraps. Never break a path or a file:line citation across lines;
grep-ability is a gate-worthy property. Keep lists tight: no blank line between
items of one list.

**Sentence discipline (STE-derived, for imperative content).** Short sentences.
Active voice. Present tense. One idea per sentence. One instruction per step.
For tasks, steps, rollback, and self-check sections: write imperatives you can
execute verbatim.

**Vocabulary discipline.** Use approved terms from the job glossary. One term,
one meaning. Do not invent synonyms for mechanisms that already have a name.
Modality is fixed vocabulary: MUST / SHOULD / MAY (RFC-2119) for requirements;
PASS / CHANGES / ERROR for verdicts; NOT_RUN / PASS / FAIL for checks.

**Requirement statements.** Use EARS: `When <trigger> [<precondition>], the
system shall <response>`. One requirement, one sentence, one id.

**Tables.** One idea per row. A cell that needs a paragraph becomes a named
detail block below the table instead. Never wrap a table row across a cell.

**Structure discipline.** Declare reader and question. Respect the word budget;
state a reason when over. Decisions carry authority tags (`[HUMAN-DECIDED]` or
`[DELEGATED: rationale]`). Negative evidence is mandatory: state what was
deliberately not verified, with reason, owner, and revisit trigger. Each fact
lives once — link ids, never restate another artifact's content.

## Metadata

Use assets/templates. Preserve YAML frontmatter using **one JSON value per line**,
a valid YAML subset parsed by the controller without dependencies. Use quoted
strings, JSON arrays, numbers, booleans, or null. Do not use block scalars, comments,
aliases, nested YAML, or unquoted values in frontmatter.

All artifacts carry schema_version, artifact_type, job_id, cycle, status,
author_role, profiles, source_snapshot, input_artifacts, and updated_at. Set status
to ready only after checking the artifact; use blocked for material missing evidence.
Use UTC timestamps with timezone. input_artifacts is a JSON array of objects with
path and sha256, using job-relative paths and controller-returned hashes. Record
all direct inputs. The controller checks hashes, required dependencies and containment.

source_snapshot is null before build. Afterward use the job-relative snapshot path.
The recorded environment fingerprint is evidence, not an automatic live drift
detector. Recheck before acceptance and promotion. Never store secrets or private
records in evidence; reference sanitized access-controlled records instead.

## Identifiers and traceability

| Prefix | Meaning |
| --- | --- |
| REQ-001 | Functional requirement |
| AC-001 | Observable acceptance criterion |
| NFR-001 | Measurable quality requirement |
| RISK-001 | Risk, owner, mitigation and trigger |
| DEC-001 | Consequential design decision |
| TASK-001 | Work item with dependencies |
| TEST-001 | Verification case and execution evidence |
| FIND-code-001 | Discipline-scoped review finding |
| EXC-001 | Authorized exception with scope and expiry |

Never silently renumber identifiers already referenced. Record supersession.
Map requirement -> acceptance -> decision/component -> task -> test -> evidence
-> review -> release step. Reuse identifiers instead of duplicating source text.
In traceability tables, list each complete identifier explicitly. Do not shorten
TASK-001 to 001 or compress AC-001, AC-002 and AC-003 into a range. Every criterion
must remain discoverable through literal search. Receiving agents must inspect the
actual relationships; identifier presence alone does not prove adequate coverage.

## Readiness

Analysis must declare required_reviewers as an inline JSON array matching its
applicability table. Include code, security and platform plus every applicable
specialist. The controller prevents later builds from dropping this accepted set.

Separate observed facts, assumptions, proposals, and unresolved questions. Cite
repository path and revision, platform object ID and export/version, or official
documentation with product version and retrieval date. Never invent owners,
approvals, capabilities, load figures, integrations or execution results.

Replace every placeholder before ready. N/A requires scope evidence and a reason.
Missing credentials yield NOT_RUN, never PASS. A ready verification plan can contain
NOT_RUN before build; build completion requires execution outcomes and resolution
or authorized exception for relevant gaps. Structural validation cannot judge
substantive correctness; the receiving agent must assess it.

Use organization quality policy. If absent, propose measurable thresholds and
identify approval dependencies. Do not invent universal SLO, recovery, coverage,
performance, or regulatory requirements.

## Ownership

| Artifact | Author | Mutation |
| --- | --- | --- |
| feature.md | Orchestrator, verbatim input | Immutable |
| brief.md | engram-frame with the human owner, before the job | Immutable once registered by create; required analysis input while pinned |
| analysis.md | Analyzer | Freeze after acceptance |
| plan.md | Planner | Freeze after acceptance |
| tasks.md | Task author, then builder progress | Versioned handoffs |
| verification.md | Task author, then builder results | Versioned handoffs |
| release.md | Task author, then builder release details | Versioned handoffs |
| build.md | Builder | New accepted evidence each round |
| round-N/review-*.md | Assigned independent reviewer | Immutable after ingestion |
| decision.md | Approver | Immutable within cycle |
| rework-N.md | Orchestrator, verbatim instruction | Immutable |
| source-*.json | Controller | Immutable |

The controller archives accepted bytes under evidence/. Preserve cycle/round
history. Scope-changing rework repeats analysis, plan and tasks; it is not a minor
builder fix. Never edit evidence archives to make hashes agree.
