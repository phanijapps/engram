# Position A — Automate stewardship; reserve sovereignty

**Thesis:** The constitution should manage its own maintenance, not legislate its own meaning.
A system that knows a boundary but waits for the owner to notice its breach has delegated clerical surveillance upward.
Automate detection, derivation, drafting and delivery; make the owner's scarce action an informed approve/reject/rework decision.
This is a proposal, not an amendment or a claim that these capabilities already ship.

## Verified ground truth — with corrections to the brief

- `docs/research/debate-knowledge/verdict.md` specifies 2,400-token advisory and 3,000-token blocking thresholds.
  It is explicitly a **verdict draft**, calls the size law a proposal, and also contains a conflicting ~150-line structural clause.
  Adopt tokens as the sole blocking measure only through ratification; do not pretend this draft is enforced law.
- `tools/ci/check_constitution.py:119-149` checks catalogue paths and requires each pack's `knowledge.md` and `rules/`.
  It neither measures tokens nor accepts the proposed manifest alternative. A split needs a same-change gate update.
- `docs/constitution/registry/constitution-catalogue.json` is a committed second representation of rule statements and references.
  `.agents/skills/engram-constitution/scripts/scaffold.py:64` seeds it; `scripts/validate.py:17-26` checks paths and duplicate IDs.
  Repository Python searches found no rules-to-catalogue regeneration command. Maintenance remains an authoring obligation.
  Existing entries visibly truncate statements/references; no dedicated gate field records enforcement status consistently.
- The umbrella gate does offer a small hint: “flip status” when a planned gate exists (`check_constitution.py:100-101`).
  Thus “suggests nothing” is too absolute. What is absent is a filed, actionable maintenance proposal, not all helpful wording.
- `.agents/skills/engram-constitution/SKILL.md` defines invoked interview/mining/drafting flows and owner ratification.
  Its scripts scaffold, validate and lint; none is a scheduled analyst or notification dispatcher.
- Factory L3 emission is real: `.agents/skills/engram-factory/scripts/factory.py:230-244` appends registered events.
  Local `.engram-factory/jobs/011-en004-fail-closed/events.jsonl` contains created/approved/released records; it is not committed evidence.
  `docs/constitution/registry/events/registry.json` declares `ci.run`; repository search found no emitter, including in `.github/workflows/ci.yml`.
- `.agents/skills/engram-factory/SKILL.md` already separates analysis, build, independent review, human approval and merge.
  Extend that human-in-loop pattern upstream to problem discovery; do not bypass it downstream.

Index mining: ran `python3 tools/hooks/index-mining.py`; recall returned zero hits for both queries:
**index empty for constitution automation catalogue knowledge split**; **index empty for self-managing constitution proposal engine**.
The installed binary rejected the hook's `search` command; code evidence above comes from direct repository inspection, not invented index results.

## 1. Derive, do not maintain a shadow constitution

Make `constitution-catalogue.json` a deterministic build artifact: humans edit canonical rules, never its entries.
A scanner over `<pack>/rules/*.md` extracts rule ID, pack, complete statement, typed references and declared gate/enforcement status.
Sort by stable ID, serialize deterministically, record generator/source identity, and compare regeneration byte-for-byte in CI.
Check both directions: every canonical rule appears once, every catalogue record resolves, references resolve, duplicate IDs fail.
Fail malformed or ambiguous input instead of quietly dropping a rule; a parser failure must never produce a smaller “healthy” catalogue.

Do not oversell today's format: actual rules combine “References / enforcement,” while the template separates those fields.
Root `00-INDEX.md` embeds additional law; `rules/rail-*.md` includes rail summaries, not uniformly one rule per file.
The owner must approve a source-ownership/metadata migration covering these cases before regeneration becomes authoritative.
Parse supported canonical forms explicitly; never count a cross-reference or rail summary as a second law.
A declared gate is not proof of enforcement: retain “review,” “partial,” and “planned,” and separately verify gate paths and coverage.
Do not let an LLM infer a blocking gate from prose and silently promote that inference into the generated catalogue.

This is maximal automation precisely because it removes duplicate work rather than assigning a robot to keep two sources synchronized.
Commit the generated JSON if consumers need it, but treat it like generated contracts: reproducible output, not an editorial surface.

## 2. A proposal engine, not another red dashboard

Introduce a repository-tooling `constitution-health` command; this is not a new Engram runtime service or LLM host.
Run it after merge, on a schedule to catch expiries during quiet periods, and on structured gate findings.
Use one deterministic collector for sizes, broken references, missing metadata, catalogue drift, manifest closure and expiry dates.
Pin tokenizer identity/version and measurement scope; report actual counts, source revision and measurement failures.
A missing tokenizer is “unknown/check failed,” never zero tokens. Ratify the threshold's equality semantics before blocking enforcement.

Findings feed an agent runner using the constitution skill's evidence-mining and drafting discipline, not `scaffold.py` against the live tree.
Give it a read-only source snapshot and an isolated proposal workspace; draft-only failures must not prevent deterministic findings being filed.
Persist one recommendation per issue/revision with evidence, severity, alternatives, exact patch, validation, rollback and unresolved judgments.
The owner should receive: “testing/knowledge.md crossed 2,400 tokens — split proposal attached; fragments + manifest drafted.”
That is an **illustrative future notification**, not a measured claim about today's small testing pack.

For a split, draft topic fragments, `knowledge/INDEX.md`, prerequisite closure, task routes and the full-read route.
Include measured token estimates, total full-read cost, every inbound-link update, and section-by-section preservation accounting.
Delete the old monolith or draft the verdict's ≤5-line redirect; never leave two canonical copies.
Draft the gate's manifest support and negative fixtures in the same proposal; passing today's monolith-only gate is not sufficient.
At 2,400, recommend before a crisis; at the ratified blocking threshold, preserve failure until a compliant change lands.
A pending recommendation, a rejection, or an unavailable drafting model cannot waive an already-ratified gate.

Use durable states: detected → drafted/needs-analysis → notified → approved/rejected/rework → applied → verified.
Bind approval to exact source and artifact hashes, following the factory's snapshot model; source changes invalidate approval.
Independent review precedes owner ratification; after ratification, automation applies and validates only that approved patch.
Record decisions with canonical evidence, registering any new event families before emission; wire `ci.run` rather than claiming it exists already.

## 3. Self-healing boundaries — a narrowly authorized fast lane

The owner first ratifies the automation policy itself; the following are proposed standing permissions, not permissions granted by this essay.

| Without per-change approval, under that policy | Requires explicit approval of the exact change |
|---|---|
| Rebuild catalogue from unchanged approved source; never change generator policy | Law text, IDs, references that express obligations, enforcement strength or gate scope |
| Refresh manifest-derived inventory/counts from approved fragments and approved routes | Split/merge/move content; add/remove fragments; change purposes, prerequisites or task routes |
| Whitelisted whitespace/format lint fixes with mechanically checked semantic invariance | Wording edits, including STE rewrites; any “fix” whose invariance cannot be proved |
| File/deduplicate proposals and repair generated delivery indexes | Exceptions, deadlines, thresholds, suppressions that change enforcement, automation policy changes |

Manifest regeneration cannot invent prerequisite relationships: those are semantic context boundaries, not filenames to sort.
Every automatic repair is allowlisted, logged, idempotent, path-confined, independently validated and reversible.
Unexpected paths, ambiguous parsing or failed validation stop the write and create a proposal; no best-effort law mutation.
A bot cannot edit its own permissions, weaken its checks, approve its own semantic change or merge through a failing mandatory gate.

## 4. Invert the interaction: the system advises; the owner decides

The durable recommendation queue is authoritative; notifications are replaceable delivery surfaces, not competing proposal stores.
- **CI/PR annotations and check summaries:** local finding, severity, exact evidence and a link to the drafted recommendation.
- **Repository issue or draft PR inbox:** reviewable patch, alternatives, routes/content accounting, approve/reject/rework controls.
- **Factory job queue and status view:** proposed maintenance jobs, dependencies and owner-decision waiting states; no automatic ratified brief.
- **Scheduled digest/dashboard:** trends, approaching thresholds/expiries, proposal age, rejected items and unresolved delivery failures.
- **Slack or equivalent opt-in messaging:** actionable summary and deep link, not approval inferred from an emoji or silence.
- **CLI/agent-session advisory:** current health and queued decisions before relevant work, without forcing the owner to originate a prompt.
These integrations are proposed, not existing surfaces; keep repo evidence access-controlled and verify delivery/retry against the durable queue.
The owner chooses cadence and priorities. The default experience is “here is the prepared decision,” not “please inspect seven packs.”

## 5. Automation's strongest objections, and enforceable guards

| Failure | Guard |
|---|---|
| Auto-drift into superficially neat but incoherent fragments | No unapproved splits; preservation map, companion closure, full-read route and independent task-context review |
| Proposal spam or rewrite loops | Stable finding fingerprints, one open proposal per issue, batching, change-sensitive retries, cooldown and recorded rejection rationale |
| Rubber-stamping hides new law in “maintenance” | Separate mechanical/semantic diffs; summarize changed obligations and rejected alternatives; independent review and hash-bound explicit approval |
| Agent hallucination or hostile instructions in source | Treat source as evidence, not authority; sandbox drafting, restrict paths/network, validate citations and keep write/approval credentials separate |
| False green from failed scans or stale approvals | Fail closed on incomplete inventory; show unknowns; rebase/revalidate and invalidate changed snapshots; never auto-waive a gate |
| Broken notifier, costly model loop or owner absence | Durable outbox/retries and delivery health, bounded time/cost, visible backlog aging; silence is not consent |

Track detection-to-proposal latency, duplicate rate, rejection reasons, delivery failures and post-apply regressions—not the volume of generated proposals.
Start in shadow mode, prove idempotent derivation and fault handling, then authorize the mechanical lane; semantic work remains owner-gated.

## 6. What remains genuinely human-directed

Purpose, acceptable risk, priorities, scar-born constraints and whether a split preserves the right mental model require human judgment.
A metric cannot decide whether two topics belong together; a green gate cannot establish that a rule is desirable or complete.
The owner may originate a new direction before any detector sees it, ask for an experiment, or supply missing historical intent.
That prompting is leadership, not failure. Repeatedly prompting the system to notice already-specified maintenance triggers **is** a failure.
The strongest self-managing constitution therefore automates the burden of noticing and preparing, not the responsibility of deciding.
