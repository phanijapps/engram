---
layout: default
title: Home
---
<section class="hero">
  <p class="kicker">engram · contract-first agentic memory · shipped by a software factory</p>
  <h1>Memory your agents can audit.<br>A factory that ships it with discipline.</h1>
  <p class="lead">engram is a local-first memory and knowledge layer for AI agents — a Rust core, TypeScript bindings, structured durable recall. The factory that builds it runs staged jobs, specialist agents and one human gate.</p>
  <p>
    <a class="btn btn-primary" href="{{ '/get-started/' | relative_url }}">Get started</a>
    <a class="btn btn-ghost" href="{{ '/factory/' | relative_url }}">See the factory</a>
  </p>
</section>

## Start here {#where-to-go-next}

<div class="grid g3">
  <div class="card">
    <p class="kicker">Get it</p>
    <h3><a href="{{ '/get-started/' | relative_url }}">Get Started</a></h3>
    <p>One npm command registers the MCP server with your agent; a second installs the skill bundle. Platform notes and honest limitations included.</p>
  </div>
  <div class="card">
    <p class="kicker">Deep-dive</p>
    <h3><a href="{{ '/factory/' | relative_url }}">The factory</a></h3>
    <p data-diagrams-for="{{ '/factory/' | relative_url }}">The loop, the agent roster, full versus light team modes, the board's nine stages, and how review rounds resolve — seven diagrams, each grounded in the skill's own references.</p>
  </div>
  <div class="card">
    <p class="kicker">The product</p>
    <h3><a href="{{ '/engram/' | relative_url }}">What engram is</a></h3>
    <p>Memory, knowledge, belief, hierarchy, retrieval, consolidation — the conceptual model, the current validated surface, and the alpha status stated plainly.</p>
  </div>
</div>

## The factory in one picture, compact {#the-factory-in-one-picture}

<p>A wish enters, gets analyzed, planned and decomposed, built in a worktree, reviewed by discipline specialists — and merges only after an approver recommendation and your explicit approval.</p>

{% include diagram.html id="factory-loop" size="small" alt="The engram-factory loop: feature through analyze, plan, tasks, build and review to the approver and the human gate, merge to main, review-round rebuilds capped at three, and needs-human rework paths" caption="The factory loop, drawn from SKILL.md — stage names are the skill's loop vocabulary." %}

<p>Review rounds cap at three; exhausted rounds park the job on you instead of looping forever. <a href="{{ '/factory/' | relative_url }}">Read the factory deep-dive →</a></p>

<div class="plate plate-ok">
  <span class="plate-title">Grounding</span>
  <p>Every figure on this site is an export of the repository's own artifacts — the plate caption names the source, and each page ends with the full listing.</p>
</div>

## Why this site reads the way it does {#why-this-site-reads-the-way-it-does}

<div class="plate plate-warn">
  <span class="plate-title">No fabrication</span>
  <p>This site is agent-written under a no-fabrication rule: every page ends with a <strong>grounded in</strong> listing naming the repository artifacts its claims come from, and limitations are stated on the page, not hidden.</p>
  <p>There is no marketing copy — if a sentence cannot be traced to an artifact, it does not ship.</p>
</div>

<p class="grounded"><strong>Grounded in:</strong> <code class="inline">README.md</code> (what engram is, lines 20–31) · <code class="inline">.agents/skills/engram-factory/SKILL.md</code> (factory loop, lines 21–56) · <code class="inline">.engram-factory/briefs/design-preview.html</code> (content voice) · the brief for this site (<code class="inline">LED-2</code> no-fabrication ledger row).</p>
