---
layout: page
title: The factory
permalink: /factory/
toc:
  - id: the-team
    label: "The team"
  - id: the-loop
    label: "How it works"
  - id: the-board
    label: "The board"
  - id: review-rounds
    label: "Review rounds"
  - id: delivery
    label: "Delivery"
  - id: the-frame
    label: "The frame"
  - id: limitations
    label: "Limitations"
  - id: gallery
    label: "Gallery"
---
<p class="kicker">engram-factory · the staged delivery loop</p>
<h1>A factory that turns a wish into a merged, reviewed change.</h1>
<p class="lead">You bring intent; the factory brings discipline: an interview, agents that analyze, plan and build, adversarial reviewers that reproduce claims, and a human gate nothing merges without.</p>
<p>The factory exists to ship <a href="{{ '/engram/' | relative_url }}">engram</a> — a local-first memory and knowledge layer for AI agents — so this page carries both stories: what engram is, and the machine that builds it. Every diagram is drawn from the factory's own artifacts.</p>
<p>This page explains the whole machine. Every diagram is drawn from the factory's own artifacts — the rail below jumps to any stage.</p>
{% include toc.html %}

## The team — and its two shapes {#the-team}

<p class="kicker">high level · pick a shape, the rest follows</p>

<p>Every job spawns specialist agents: <code class="inline">engram-analyzer</code> writes <code class="inline">analysis.md</code>, <code class="inline">engram-planner</code> writes <code class="inline">plan.md</code>, <code class="inline">engram-tasks</code> writes the task/verification/release trio, <code class="inline">engram-builder</code> builds in an assigned worktree, and <code class="inline">engram-approver</code> writes <code class="inline">decision.md</code>.</p>
<p>Review comes in exactly two shapes, recorded on the job before the first build:</p>
<ul>
  <li><strong><code class="inline">full</code> mode</strong> — one specialist reviewer invocation per required discipline (up to eight), in parallel, each writing its own report.</li>
  <li><strong><code class="inline">light</code> mode</strong> — one <code class="inline">red-forman</code> invocation authors <em>every</em> required per-discipline report.</li>
</ul>
{% include diagram.html id="team-modes" alt="Architecture contrast: full mode spawns one specialist per discipline in parallel; light mode spawns a single red-forman invocation authoring every report; the authoring spine and approver are identical" caption="Team modes — the authoring spine and the approver are identical in both shapes." %}
<div class="plate plate-warn">
  <span class="plate-title">The light-mode tradeoff, stated plainly</span>
  <p>Light mode saves agent invocations, and it buys that with breadth: one author writes all reports instead of eight specialists reviewing their own lenses.</p>
  <p>What does <em>not</em> change: the mandatory reviewer set (code, security, platform always), one report file per discipline, the severity gates, the verdict vocabulary, the prior-findings recheck, the approver, and your gate.</p>
  <p>This very site was built under light mode — that choice is visible in the job's board record, not buried.</p>
</div>
{% include diagram.html id="agent-roster" alt="Tree of the factory roster: analyzer, planner, tasks, builder, the eight specialist reviewers, red-forman, qa and the approver, each with the artifact it writes" caption="The roster — every role and the artifact it owns." %}

## How it works — the loop {#the-loop}

<p>The spine is eight stages, in the loop vocabulary of the factory skill's own <code class="inline">SKILL.md</code>: <code class="inline">FEATURE</code> → <code class="inline">ANALYZE</code> → <code class="inline">PLAN</code> → <code class="inline">TASKS</code> → <code class="inline">BUILD</code> → <code class="inline">REVIEW</code> → <code class="inline">APPROVE</code> → <code class="inline">MERGE</code>.</p>
<p>The first build starts review round 1. <code class="inline">MAX_REVIEW_ROUNDS = 3</code> allows at most two review-driven rebuilds, and a third round that still requests changes escalates to you instead of looping forever.</p>
{% include diagram.html id="factory-loop" alt="The factory loop with the review-round rebuild edge, the human gate, merge to main, and the needs-human rework paths" caption="The loop, verbatim semantics from SKILL.md lines 21–59." %}
<p>(The board's own stage vocabulary is the lowercase nine in <a href="#the-board">the board</a>.)</p>

## The board — nine stages, one job at a time {#the-board}

<p>The controller is <code class="inline">factory.py</code>; its ledger is <code class="inline">board.json</code>. A job's <code class="inline">stage</code> is one of exactly nine values, and transitions are validated commands, not vibes.</p>
<ul>
  <li><code class="inline">analyze</code></li>
  <li><code class="inline">plan</code></li>
  <li><code class="inline">tasks</code></li>
  <li><code class="inline">build</code></li>
  <li><code class="inline">review</code></li>
  <li><code class="inline">approve</code></li>
  <li><code class="inline">merge</code></li>
  <li><code class="inline">merged</code></li>
  <li><code class="inline">needs-human</code></li>
</ul>
<p>Artifact acceptance advances a stage, verdict outcomes drive the review branches, and only a human approval bound to an exact snapshot sha256 unlocks <code class="inline">merge</code>. Only one job is active at a time.</p>
{% include diagram.html id="board-states" alt="Lifecycle of the nine board stages with the verdict-driven branches: changes rebuild while rounds remain, errors and exhausted rounds park on needs-human, human approval leads to merge and merged, rework starts a new cycle at analyze" caption="The board state machine — transitions as factory.py actually enforces them." %}

## Review rounds — how a round resolves {#review-rounds}

<p>When the builder finishes, the controller freezes evidence: a source snapshot plus hashed inputs. Reviewers are spawned against that frozen set — code, security and platform are always required; ux, ui, backend, data and operations are added by the accepted analysis.</p>
<p>Every reviewer returns one of three verdicts: <span class="chip chip-pass">PASS</span> <span class="chip chip-block">CHANGES</span> <span class="chip chip-block">ERROR</span> — and the round resolves only when every verdict is in:</p>
<ul>
  <li><strong>all <code class="inline">PASS</code></strong> — the approver reads the complete evidence and recommends <code class="inline">APPROVE</code> or <code class="inline">ESCALATE</code>.</li>
  <li><strong>any <code class="inline">CHANGES</code></strong> — the complete finding set goes back to the builder if rounds remain; a third-round <code class="inline">CHANGES</code> blocks instead.</li>
  <li><strong>any <code class="inline">ERROR</code></strong> — the round cannot conclude; the job parks on <code class="inline">needs-human</code>.</li>
</ul>
<p>Findings carry severities with fixed consequences:</p>
<table>
  <thead><tr><th>Severity</th><th>Consequence</th></tr></thead>
  <tbody>
    <tr><td>critical</td><td><code class="inline">CHANGES</code> plus immediate escalation</td></tr>
    <tr><td>high</td><td><code class="inline">CHANGES</code></td></tr>
    <tr><td>medium</td><td><code class="inline">CHANGES</code> (may carry an authorized scoped exception)</td></tr>
    <tr><td>low</td><td>a follow-up a <code class="inline">PASS</code> may retain</td></tr>
  </tbody>
</table>
<p>Missing access or evidence is never a low-severity nicety — it is an <code class="inline">ERROR</code>, because a review that cannot establish its conclusion cannot pass it.</p>
{% include diagram.html id="review-rounds" alt="Workflow of one review round: evidence freeze, spawning required reviewers, waiting for all verdicts, and the three resolution branches" caption="One review round — severity gates and the wait-for-all rule come from reviews.md." %}

## How it reaches your machine {#delivery}

<p>The factory itself is a skill: <code class="inline">.agents/skills/</code> is the single authored source, the launcher regenerates its npm payload from that tree at <code class="inline">prepack</code>, and <code class="inline">engram-mcp install --agent</code> delivers skills, hooks and the factory's specialist subagents into your agent host.</p>
<p>Nothing is hand-copied; drift is impossible by construction because the payload is generated, gitignored output.</p>
{% include diagram.html id="delivery-pipeline" alt="Dataflow from the authored .agents/skills tree through prepack payload generation and the npm tarball to client and agent installs in host directories" caption="Skill delivery — authored tree to installed harness." %}
<div class="plate plate-block">
  <span class="plate-title">Platform wart · Copilot is repo-scoped</span>
  <p>Copilot discovers agents only per repository, so <code class="inline">install --agent copilot</code> writes its 15 agent files into the <em>current repository's</em> <code class="inline">.github/agents/</code> even when you asked for user scope. Run that install from the repo that should carry them.</p>
</div>

## Before the factory: the frame {#the-frame}

<p>A job starts with <code class="inline">engram-frame</code>, an interview the orchestrator holds with you before any factory work: option-based questions, MoSCoW scoping, strike-lists instead of blank pages, and a decision ledger where every consequential choice is either human-decided (with your quote) or explicitly delegated with a boundary.</p>
<p>The failure mode it exists to prevent is a job where big choices got decided downstream with no recorded human decision — surfacing only at the approval gate.</p>
{% include diagram.html id="frame-handoff" alt="Sequence: the owner's wish, option-based interview questions, verbatim picks, brief ratification, factory.py create with feature and brief files, the board job record, and the immutable brief registration" caption="Frame → factory — the handoff that makes intent auditable." %}

## Limitations, stated here rather than hidden {#limitations}

<div class="grid g3">
  <div class="card">
    <p><span class="mark-block">LIMIT</span></p>
    <p><strong>Light mode trades specialist depth for fewer invocations</strong> — one author writes every report. The rubric binds that author identically, but one agent's breadth is not eight specialists' depth. If a job's blast radius is large, pin <code class="inline">full</code>.</p>
  </div>
  <div class="card">
    <p><span class="mark-block">LIMIT</span></p>
    <p><strong>The factory is only as honest as its evidence.</strong> Verdicts, hashes and receipts are recorded, and this site links them — but nothing here is a substitute for reading the artifacts when the stakes are high.</p>
  </div>
  <div class="card">
    <p><span class="mark-block">LIMIT</span></p>
    <p><strong>Diagrams on this page are hand-grounded exports</strong>, produced with the archify tool from the artifacts named below; if those artifacts change, a diagram can drift until it is regenerated. Regenerating pages mechanically from skill text is deliberately a future job, not a current behavior.</p>
  </div>
</div>

## The figure gallery {#gallery}

<p>Every figure on this page, titled and anchored — jump straight to a plate:</p>
<ul>
  <li><a href="#fig-factory-loop">The factory loop</a></li>
  <li><a href="#fig-team-modes">Team modes</a></li>
  <li><a href="#fig-agent-roster">The agent roster</a></li>
  <li><a href="#fig-board-states">The board state machine</a></li>
  <li><a href="#fig-review-rounds">One review round</a></li>
  <li><a href="#fig-delivery-pipeline">Skill delivery</a></li>
  <li><a href="#fig-frame-handoff">Frame → factory</a></li>
</ul>

<p class="grounded"><strong>Grounded in:</strong> <code class="inline">.agents/skills/engram-factory/SKILL.md</code> (loop, <code class="inline">MAX_REVIEW_ROUNDS = 3</code>, Agents table) · <code class="inline">.agents/skills/engram-factory/references/agents.md</code> (role contracts) · <code class="inline">.agents/skills/engram-factory/references/agent-team.md</code> (full vs light tables, mode selection) · <code class="inline">.agents/skills/engram-factory/references/reviews.md</code> (verdict vocabulary, severity gates, wait-for-all) · <code class="inline">.agents/skills/engram-factory/assets/board.schema.json</code> (nine-stage enum) · <code class="inline">.agents/skills/engram-factory/scripts/factory.py</code> (validated transitions) · <code class="inline">.agents/skills/engram-frame/SKILL.md</code> (interview, provenance gate, handoff) · <code class="inline">packages/launcher/README.md</code> and <code class="inline">packages/launcher/scripts/generate-skills-payload.cjs</code> (delivery pipeline, Copilot side effect). Diagrams were produced with archify; each one's finalize receipt is quoted in this job's <code class="inline">build.md</code>.</p>
