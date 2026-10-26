---
layout: page
title: What engram is
permalink: /engram/
---
<p class="kicker">engram · the product the factory ships</p>
<h1>Local-first memory and knowledge for AI agents.</h1>
<p class="lead">engram is an open-source agentic knowledge and memory layer: a Rust core that owns deterministic memory, knowledge-graph, belief, hierarchy and retrieval behavior, with TypeScript bindings — so agents get reliable, structured, long-lived knowledge and memory instead of opaque, disposable context windows.</p>

<h2>What it is not</h2>
<p>The README is blunt about this, and so is this page: engram is <strong>not a vector database</strong> — vectors are one retrieval mode among six. It is <strong>not just a memory store</strong> — knowledge is source-grounded, not free-floating, and memory is experiential, not factual. It is the layer where raw observations become durable beliefs, where code becomes a queryable graph, and where policy, provenance and scope govern every read and write — so knowledge and memory are auditable, not accidental.</p>

<h2>The conceptual model — eight commitments</h2>
<div class="grid g2">
  <div class="card">
    <p class="kicker">1 · Memory</p>
    <p>A first-class domain, not just rows: writes, retrieval, forgetting, lifecycle events, scope isolation and policy enforcement.</p>
  </div>
  <div class="card">
    <p class="kicker">2 · Knowledge graph</p>
    <p>Source-grounded: source → document → chunk → entity → relationship, with taxonomy and ontology — every node traceable to where it came from.</p>
  </div>
  <div class="card">
    <p class="kicker">3 · Belief synthesis</p>
    <p>What agents <em>believe</em> stays distinct from what they observed: beliefs carry valid time, can be superseded, retracted and contradicted.</p>
  </div>
  <div class="card">
    <p class="kicker">4 · Hierarchy</p>
    <p>Aggregation and navigation above the graph — context compression you can walk instead of a wall of text.</p>
  </div>
  <div class="card">
    <p class="kicker">5 · Retrieval composition</p>
    <p>Never one mode: graph, lexical and vector lanes fused (RRF), with cross-encoder rerank, associative (PPR) and community-summary (GraphRAG) paths.</p>
  </div>
  <div class="card">
    <p class="kicker">6 · Consolidation</p>
    <p>An explicit pipeline: reflection synthesizes derived beliefs; decay retires what stops earning its keep (an Ebbinghaus curve); both run behind a gate.</p>
  </div>
  <div class="card">
    <p class="kicker">7 · Policy, provenance, scope</p>
    <p>Governance baked into every path — policy checks stay visible on write, retrieve, ingest, consolidate and forget.</p>
  </div>
  <div class="card">
    <p class="kicker">8 · Contract-first</p>
    <p>Domain types outrank SQL: portable contracts first, engines behind adapter cells, TypeScript wraps generated contracts instead of redefining them.</p>
  </div>
</div>

<h2>How data flows</h2>
<p>One direction on the write path: an agent writes (or ingests a source) → engram processes and stores across swappable engine cells, stamping policy and provenance on every row → a separate retrieval-composition layer fuses the recall modes into one policy-filtered context packet returned to the agent. Storage and retrieval are deliberately decoupled.</p>

<h2>Status — alpha, stated plainly</h2>
<div class="note">
  <p><strong>engram is pre-1.0 — demo-driven, not production-ready.</strong> The conceptual model is direction-fixed; the frozen v1 contract covers the memory + retrieval + evaluation vertical; belief, contradiction, hierarchy and consolidation are direction-fixed draft extension contracts, not frozen. The validated surface today: memory (SQLite), knowledge graph (SQLite), belief and hierarchy (SQLite + SurrealDB), vector retrieval, retrieval composition with fusion and rerankers, consolidation, MCP servers, the full <code class="inline">EngramProvider</code> surface reachable from TypeScript, and backend swap by config string. Read the README's Status section before depending on any of it.</p>
</div>

<p><a href="{{ '/install/' | relative_url }}">Install engram →</a> · <a href="{{ '/factory/' | relative_url }}">Meet the factory that develops it →</a></p>

<p class="grounded"><strong>Grounded in:</strong> <code class="inline">README.md</code> — lines 20–31 (what engram is and is not), 122–305 (what it is for, concepts, why it exists, the eight-part conceptual model), 340–357 (architecture and data flow), 361–395 (status: pre-1.0, validated surface). Alpha status is the README's own badge and Status section, not a marketing line.</p>
