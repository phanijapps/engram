# Position B: Human-directed structure; automation as servant

## Thesis

Automate evidence, bookkeeping, and reversible mechanics. Keep the choice of
law and structure human-originated. A constitution is not only a set of files:
it is a taxonomy of responsibilities, obligations, and trade-offs. Selecting
that taxonomy is governance, not maintenance.

The practical objective is not to make the owner notice less. It is to make an
owner's informed direction cheap: accurate measurements, visible failures,
reproducible derivations, and an assistant ready to analyze the question the
owner chooses to ask.

## Ground truth checked

- `docs/research/debate-knowledge/verdict.md` specifies the knowledge-file
  size policy: advisory at 2,400 tokens and blocking at 3,000. It describes
  the intended enforcement, but `tools/ci/check_constitution.py` contains no
  token measurement or knowledge-file threshold; it currently validates
  catalogue paths and pack layout.
- `docs/constitution/registry/constitution-catalogue.json` is a checked-in
  static JSON catalogue. The constitution gate reads it and confirms that its
  declared rule paths exist; it does not derive the entries from rule files.
- `.agents/skills/engram-constitution/SKILL.md` describes an invoked drafting
  workflow. No repository evidence found a scheduled constitution advisor.
- `.agents/skills/engram-factory/references/workflow.md` separates review,
  approval, and merge: an approval recommendation is not human approval, and
  explicit human rework restarts the consequential stages.

The brief also records the session history: owner prompts initiated the
renames, splits, and replatforming, while several autonomous structural
choices required owner correction. That conversational history is not
recoverable from repository files, so it should be treated as supplied session
evidence, not as an independently audited Git fact. It is still the relevant
operational warning.

## Why a proposal is already a structural act

A proposed split does more than surface a threshold breach. It names the
fragments, puts content in each, selects dependencies, and supplies a manifest.
That is a boundary taxonomy. It answers *what belongs together* before the
owner has asked the question.

The apparent human control of “approve or reject” is weaker than it sounds.
The prepared diff becomes the default, alternatives are unseen, and rejecting
it costs more than accepting it. Repeated approval under that asymmetry is
structural drift with a human signature. The system has originated the design;
the owner has merely ratified its framing.

This matters most where a size signal is underdetermined. A 3,001-token file
might warrant a split by capability, lifecycle, audience, stability, or not at
all if a full mental model is the useful unit. Measurement can establish the
condition. It cannot choose the organizing principle.

## The full derivation concession

The catalogue should be derived, not hand-maintained. A generator may scan the
human-authored rule files and reproduce their IDs, paths, statements, and gate
references; CI should fail if the checked-in result is stale. That removes
clerical drift without moving legislative authority.

Likewise, automation should regenerate a manifest's mechanical inventory,
hashes, links, and other fields that are fully determined by human-authored
sources. It should apply deterministic lint or formatting fixes whose result is
meaning-preserving and reviewable. Regeneration and lint repair are useful
proactive work because they do not select a law, create a boundary, or alter a
semantic claim.

The exact line is this: **derived artifacts automate; law and structure stay
human.** A tool may reproduce the consequences of a human decision. It may not
write or revise rule text, decide that a file should be split, choose fragment
names or membership, define prerequisite routes, rename a component, or
replatform a surface. If a manifest contains those choices, those fields are
human-authored inputs; only its derived portions regenerate.

## Detection is not proposal

Build gates that detect and report:

- a size-law measurement, including the method and threshold crossed;
- catalogue/rule drift and stale generated output;
- orphan manifest entries, missing targets, and unresolved links;
- mechanically safe fixes available for regeneration or linting.

Put these facts in CI output and, if desired, an owner dashboard. A good report
says, “`testing/knowledge.md` measured 3,104 tokens; threshold 3,000; last
measurement and command attached.” It does **not** attach a split plan,
fragment list, drafted manifest, or diff. That is a nudge supported by evidence
rather than a design preloaded for ratification.

The owner can then originate the structural request: retain the monolith with
an approved exception, split it, revise the law, or ask the assistant to
compare options. Once asked, the assistant can give strong, source-grounded
advice and draft the chosen implementation. Human origin is preserved without
withholding expertise.

## Advisor on invocation, not a nag engine

A scheduled proposal queue has predictable failure modes: cadence spam,
recommendations with no active decision context, and a boy-who-cried-split
effect after enough weak suggestions. The most important proposals become
indistinguishable from routine noise, and owners either rubber-stamp or stop
reading.

The safe proactive surface is a quiet status signal: failing gate, stale
derived artifact, or a dashboard count linked to evidence. It should not claim
a preferred architecture. The active advisory surface is an invoked assistant:
when the owner opens a session or asks about a finding, it can retrieve the
evidence, articulate alternatives and costs, and challenge a premature plan.

“You should be advising me” should therefore mean that the agent advises the
owner within the owner's working session, using current gate evidence; it does
not necessarily authorize the system to generate unsolicited structural
recommendations. Advice is valuable because it is responsive to an owner-set
question. A background system can report facts, but it cannot safely infer
which structural question is worth asking now.

## What should run without asking

Proactive automation is both safe and worthwhile for deterministic catalogue
and mechanical-manifest regeneration, stale-derived-output checks, orphan and
drift detection, measurement, formatting, and meaning-preserving lint fixes.
It is also safe to preserve evidence and make failures visible. These reduce
latency and error while leaving the owner free to choose the repository's
shape.

That is not anti-automation. It assigns automation the job it can prove it did:
measure, reconcile, reproduce, and report. It assigns people the job only they
can own: choosing the concepts that organize future work.
