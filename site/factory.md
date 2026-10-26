---
layout: page
title: The factory
permalink: /factory/
---
<p class="kicker">engram-factory · the staged delivery loop</p>
<h1>A factory that turns a wish into a merged, reviewed change.</h1>
<p class="lead">You bring intent; the factory brings discipline: an interview that captures what you want without designing it, agents that <code class="inline">ANALYZE</code>, <code class="inline">PLAN</code>, decompose and <code class="inline">BUILD</code>, discipline reviewers that reproduce claims before passing them, and a human gate nothing merges without. This page explains the whole machine, and every diagram on it is drawn from the factory's own artifacts.</p>

<h2>1 · The loop</h2>
<p>The spine is eight stages, in the loop vocabulary of the factory skill's own SKILL.md: <code class="inline">FEATURE</code> → <code class="inline">ANALYZE</code> → <code class="inline">PLAN</code> → <code class="inline">TASKS</code> → <code class="inline">BUILD</code> → <code class="inline">REVIEW</code> → <code class="inline">APPROVE</code> → <code class="inline">MERGE</code>. The first build starts review round 1; <code class="inline">MAX_REVIEW_ROUNDS = 3</code> allows at most two review-driven rebuilds, and a third round that still requests changes escalates to you instead of looping forever. (The board's own stage vocabulary is the lowercase nine in §3.)</p>
{% include diagram.html id="factory-loop" pair=true alt="The factory loop with the review-round rebuild edge, the human gate, merge to main, and the needs-human rework paths" caption="The loop, verbatim semantics from SKILL.md lines 21–59." %}

<h2>2 · The team — and its two shapes</h2>
<p>Every job spawns specialist agents: <code class="inline">engram-analyzer</code> writes <code class="inline">analysis.md</code>, <code class="inline">engram-planner</code> writes <code class="inline">plan.md</code>, <code class="inline">engram-tasks</code> writes the task/verification/release trio, <code class="inline">engram-builder</code> builds in an assigned worktree, and <code class="inline">engram-approver</code> writes <code class="inline">decision.md</code>. Review comes in exactly two shapes, recorded on the job before the first build:</p>
<ul>
  <li><strong><code class="inline">full</code> mode</strong> — one specialist reviewer invocation per required discipline (up to eight), in parallel, each writing its own report.</li>
  <li><strong><code class="inline">light</code> mode</strong> — one <code class="inline">red-forman</code> invocation authors <em>every</em> required per-discipline report.</li>
</ul>
{% include diagram.html id="team-modes" pair=true alt="Architecture contrast: full mode spawns one specialist per discipline in parallel; light mode spawns a single red-forman invocation authoring every report; the authoring spine and approver are identical" caption="Team modes — the authoring spine and the approver are identical in both shapes." %}
<div class="note">
  <p><strong>The light-mode tradeoff, stated plainly.</strong> Light mode saves agent invocations, and it buys that with breadth: one author writes all reports instead of eight specialists reviewing their own lenses. What does <em>not</em> change: the mandatory reviewer set (code, security, platform always), one report file per discipline, the severity gates, the verdict vocabulary, the prior-findings recheck, the approver, and your gate. This very site was built under light mode — that choice is visible in the job's board record, not buried.</p>
</div>
{% include diagram.html id="agent-roster" pair=true alt="Tree of the factory roster: analyzer, planner, tasks, builder, the eight specialist reviewers, red-forman, qa and the approver, each with the artifact it writes" caption="The roster — every role and the artifact it owns." %}

<h2>3 · The board — nine stages, one job at a time</h2>
<p>The controller is <code class="inline">factory.py</code>; its ledger is <code class="inline">board.json</code>. A job's <code class="inline">stage</code> is one of exactly nine values: <code class="inline">analyze</code>, <code class="inline">plan</code>, <code class="inline">tasks</code>, <code class="inline">build</code>, <code class="inline">review</code>, <code class="inline">approve</code>, <code class="inline">merge</code>, <code class="inline">merged</code>, <code class="inline">needs-human</code>. Transitions are validated commands, not vibes: artifact acceptance advances a stage, verdict outcomes drive the review branches, and only a human approval bound to an exact snapshot sha256 unlocks <code class="inline">merge</code>. Only one job is active at a time.</p>
{% include diagram.html id="board-states" pair=true alt="Lifecycle of the nine board stages with the verdict-driven branches: changes rebuild while rounds remain, errors and exhausted rounds park on needs-human, human approval leads to merge and merged, rework starts a new cycle at analyze" caption="The board state machine — transitions as factory.py actually enforces them." %}

<h2>4 · Review rounds — how a round resolves</h2>
<p>When the builder finishes, the controller freezes evidence: a source snapshot plus hashed inputs. Reviewers are spawned against that frozen set — code, security and platform are always required; ux, ui, backend, data and operations are added by the accepted analysis. Every reviewer returns one of three verdicts: <code class="inline">PASS</code>, <code class="inline">CHANGES</code> or <code class="inline">ERROR</code>. The round resolves only when every verdict is in:</p>
<ul>
  <li><strong>all <code class="inline">PASS</code></strong> — the approver reads the complete evidence and recommends <code class="inline">APPROVE</code> or <code class="inline">ESCALATE</code>.</li>
  <li><strong>any <code class="inline">CHANGES</code></strong> — the complete finding set goes back to the builder if rounds remain; a third-round <code class="inline">CHANGES</code> blocks instead.</li>
  <li><strong>any <code class="inline">ERROR</code></strong> — the round cannot conclude; the job parks on <code class="inline">needs-human</code>.</li>
</ul>
{% include diagram.html id="review-rounds" pair=true alt="Workflow of one review round: evidence freeze, spawning required reviewers, waiting for all verdicts, and the three resolution branches" caption="One review round — severity gates and the wait-for-all rule come from reviews.md." %}
<p>Findings carry severities with fixed consequences: <em>critical</em> means <code class="inline">CHANGES</code> plus immediate escalation, <em>high</em> and <em>medium</em> mean <code class="inline">CHANGES</code> (medium may carry an authorized scoped exception), <em>low</em> is a follow-up a <code class="inline">PASS</code> may retain. Missing access or evidence is never a low-severity nicety — it is an <code class="inline">ERROR</code>, because a review that cannot establish its conclusion cannot pass it.</p>

<h2>5 · How it reaches your machine</h2>
<p>The factory itself is a skill: <code class="inline">.agents/skills/</code> is the single authored source, the launcher regenerates its npm payload from that tree at <code class="inline">prepack</code>, and <code class="inline">engram-mcp install --agent</code> delivers skills, hooks and the factory's specialist subagents into your agent host. Nothing is hand-copied; drift is impossible by construction because the payload is generated, gitignored output.</p>
{% include diagram.html id="delivery-pipeline" pair=true alt="Dataflow from the authored .agents/skills tree through prepack payload generation and the npm tarball to client and agent installs in host directories" caption="Skill delivery — authored tree to installed harness." %}
<p>One honest platform wart: Copilot discovers agents only per repository, so <code class="inline">install --agent copilot</code> writes its 15 agent files into the <em>current repository's</em> <code class="inline">.github/agents/</code> even when you asked for user scope. Run that install from the repo that should carry them.</p>

<h2>6 · Before the factory: the frame</h2>
<p>A job starts with <code class="inline">engram-frame</code>, an interview the orchestrator holds with you before any factory work: option-based questions, MoSCoW scoping, strike-lists instead of blank pages, and a decision ledger where every consequential choice is either human-decided (with your quote) or explicitly delegated with a boundary. The failure mode it exists to prevent is a job where big choices got decided downstream with no recorded human decision — surfacing only at the approval gate.</p>
{% include diagram.html id="frame-handoff" pair=true alt="Sequence: the owner's wish, option-based interview questions, verbatim picks, brief ratification, factory.py create with feature and brief files, the board job record, and the immutable brief registration" caption="Frame → factory — the handoff that makes intent auditable." %}

<h2>Limitations, stated here rather than hidden</h2>
<ul>
  <li><strong>Light mode trades specialist depth for fewer invocations</strong> — one author writes every report. The rubric binds that author identically, but one agent's breadth is not eight specialists' depth. If a job's blast radius is large, pin <code class="inline">full</code>.</li>
  <li><strong>The factory is only as honest as its evidence.</strong> Verdicts, hashes and receipts are recorded, and this site links them — but nothing here is a substitute for reading the artifacts when the stakes are high.</li>
  <li><strong>Diagrams on this page are hand-grounded exports</strong>, produced with the archify tool from the artifacts named below; if those artifacts change, a diagram can drift until it is regenerated. Regenerating pages mechanically from skill text is deliberately a future job, not a current behavior.</li>
</ul>

<p class="grounded"><strong>Grounded in:</strong> <code class="inline">.agents/skills/engram-factory/SKILL.md</code> (loop, <code class="inline">MAX_REVIEW_ROUNDS = 3</code>, Agents table) · <code class="inline">.agents/skills/engram-factory/references/agents.md</code> (role contracts) · <code class="inline">.agents/skills/engram-factory/references/agent-team.md</code> (full vs light tables, mode selection) · <code class="inline">.agents/skills/engram-factory/references/reviews.md</code> (verdict vocabulary, severity gates, wait-for-all) · <code class="inline">.agents/skills/engram-factory/assets/board.schema.json</code> (nine-stage enum) · <code class="inline">.agents/skills/engram-factory/scripts/factory.py</code> (validated transitions) · <code class="inline">.agents/skills/engram-frame/SKILL.md</code> (interview, provenance gate, handoff) · <code class="inline">packages/launcher/README.md</code> and <code class="inline">packages/launcher/scripts/generate-skills-payload.cjs</code> (delivery pipeline, Copilot side effect). Diagrams were produced with archify; each one's finalize receipt is quoted in this job's <code class="inline">build.md</code>.</p>
