---
layout: page
title: Get Started
permalink: /get-started/
toc:
  - id: quick-start
    label: "Quick start"
  - id: embeddings
    label: "Embeddings"
  - id: agent-bundle
    label: "Agent bundle"
  - id: platform-notes
    label: "Platform notes"
---
<p class="kicker">get started · two commands, honest platform notes</p>
<h1>Install engram.</h1>
<p class="lead">One npm command installs the MCP server; one <code class="inline">engram-mcp</code> command registers it with your agent or delivers the full skill bundle.</p>
<p>Everything below is the launcher's own documented flow — including the warts. Copy any block with the button at its top-right corner.</p>
{% include toc.html %}

## Quick start {#quick-start}

<p>Install the server (alpha) and register with your agent — one command each, no JSON editing:</p>

<div class="card" markdown="1">
  <p class="kicker">01 · Install the server</p>

```
npm install -g @engramlabs/mcp@alpha
```

</div>

<div class="card" markdown="1">
  <p class="kicker">02 · Register with your client</p>
  <div class="tabs">
    <input type="radio" id="t-claude" name="qs-client" checked>
    <label for="t-claude">claude</label>
    <input type="radio" id="t-claude-desktop" name="qs-client">
    <label for="t-claude-desktop">claude-desktop</label>
    <input type="radio" id="t-cursor" name="qs-client">
    <label for="t-cursor">cursor</label>
    <input type="radio" id="t-codex" name="qs-client">
    <label for="t-codex">codex</label>
    <input type="radio" id="t-pi" name="qs-client">
    <label for="t-pi">pi</label>
    <input type="radio" id="t-copilot" name="qs-client">
    <label for="t-copilot">copilot</label>
    <div class="tab-panel p-t-claude" markdown="1">

```
engram-mcp install --client claude
```

</div>
    <div class="tab-panel p-t-claude-desktop" markdown="1">

```
engram-mcp install --client claude-desktop
```

</div>
    <div class="tab-panel p-t-cursor" markdown="1">

```
engram-mcp install --client cursor
```

</div>
    <div class="tab-panel p-t-codex" markdown="1">

```
engram-mcp install --client codex
```

</div>
    <div class="tab-panel p-t-pi" markdown="1">

```
engram-mcp install --client pi
```

</div>
    <div class="tab-panel p-t-copilot" markdown="1">

```
engram-mcp install --client copilot
```

</div>
  </div>
  <p class="small">Claude Code (project scope) · Claude Desktop (user scope) · Cursor (project scope) · Codex CLI (user scope) · pi (project scope) · GitHub Copilot / VS Code.</p>
</div>

<div class="card" markdown="1">
  <p class="kicker">03 · Or run directly without installing</p>

```
npx @engramlabs/mcp@alpha --storage ~/.engram
```

</div>

<p>The installer registers the installed package version and defaults storage to <code class="inline">~/.engram</code>. Restart your MCP client after registration.</p>

<div class="plate plate-warn">
  <span class="plate-title">Status reminder</span>
  <p>The package is alpha — install commands pin <code class="inline">@alpha</code> deliberately. Data stays in your chosen storage directory; no telemetry.</p>
</div>

## Embeddings at startup — one install, two modes {#embeddings}

<div class="card" markdown="1">
  <p class="kicker">04 · Pick your runtime mode</p>

```
engram-mcp --storage ~/.engram          # Local embeddings enabled
engram-mcp --storage ~/.engram --slim   # Embeddings off
```

</div>

<p>Local embeddings may download model assets on first launch; inference then runs on your machine. <code class="inline">--slim</code> skips embedding setup and downloads — memory, knowledge, lexical and graph features remain available, but search ranking can differ.</p>

<p>It changes runtime behavior, not installation size. To configure a client without embeddings: <code class="inline">engram-mcp install --client claude --slim</code>. Restart to switch modes; content added while embeddings were off needs vector backfill via the MCP <code class="inline">reindex</code> tool after re-enabling.</p>

## The agent bundle — skills, hooks, specialists {#agent-bundle}

<div class="card" markdown="1">
  <p class="kicker">05 · Install the agent bundle</p>
  <div class="tabs">
    <input type="radio" id="a-claude" name="qs-agent" checked>
    <label for="a-claude">claude</label>
    <input type="radio" id="a-codex" name="qs-agent">
    <label for="a-codex">codex</label>
    <input type="radio" id="a-copilot" name="qs-agent">
    <label for="a-copilot">copilot</label>
    <input type="radio" id="a-pi" name="qs-agent">
    <label for="a-pi">pi</label>
    <input type="radio" id="a-all" name="qs-agent">
    <label for="a-all">all</label>
    <div class="tab-panel p-a-claude" markdown="1">

```
engram-mcp install --agent claude
```

</div>
    <div class="tab-panel p-a-codex" markdown="1">

```
engram-mcp install --agent codex
```

</div>
    <div class="tab-panel p-a-copilot" markdown="1">

```
engram-mcp install --agent copilot
```

</div>
    <div class="tab-panel p-a-pi" markdown="1">

```
engram-mcp install --agent pi
```

</div>
    <div class="tab-panel p-a-all" markdown="1">

```
engram-mcp install --agent all
```

</div>
  </div>

```
engram-mcp install --agent claude --remove   # uninstall (name-tagged)
```

</div>

<p>What lands on your machine — <strong>seven skills</strong>:</p>
<ul>
  <li><code class="inline">engram-index</code>, <code class="inline">engram-know</code>, <code class="inline">engram-mem</code>, <code class="inline">engram-distill</code>, <code class="inline">outcome-reporting</code></li>
  <li>the software-factory pair <code class="inline">engram-factory</code> + <code class="inline">engram-frame</code></li>
</ul>
<p>Plus the factory's <strong>14 specialist subagents</strong> in your host's native dialect, <strong>recall hooks</strong> — every prompt recalls relevant memory, gated so irrelevant results stay silent; a kill switch at <code class="inline">ENGRAM_HOOKS=0</code>; fail-open always — and the <strong><code class="inline">red-forman</code></strong> read-only adversarial reviewer.</p>
<p>The skill set derives from the package's authored tree at build time — new skills flow with no installer change, and retired skills are removed automatically unless you replaced them. Uninstall is name-tagged: files you replaced or added yourself are left untouched.</p>

## Platform notes — including the warts {#platform-notes}

<div class="plate plate-block">
  <span class="plate-title">Wart · Copilot is repo-scoped, period</span>
  <p>That platform discovers agents only per repository, so <code class="inline">install --agent copilot</code> writes its 15 agent files into the <em>current repository's</em> <code class="inline">.github/agents/</code> even with <code class="inline">--scope user</code>. Run it from the repo that should carry them — and expect that side effect in any repo you are standing in.</p>
</div>
<div class="plate plate-block">
  <span class="plate-title">Wart · Copilot hooks cannot inject per-prompt context</span>
  <p>Platform limitation: session-start recall only.</p>
</div>
<div class="plate plate-block">
  <span class="plate-title">Limit · claude-desktop hosts MCP registration only</span>
  <p>No skills/agents filesystem; <code class="inline">install --agent all</code> prints that limitation.</p>
</div>
<ul>
  <li><strong>Codex</strong> agents use YAML (<code class="inline">~/.codex/agents/*.yaml</code>).</li>
  <li><strong>pi</strong> gets an in-process extension.</li>
  <li><strong>Prebuilt binaries</strong>: Linux x64/ARM64 (glibc), macOS Apple Silicon, Windows x64. Other platforms require a source build.</li>
</ul>

<p class="grounded"><strong>Grounded in:</strong> <code class="inline">README.md</code> lines 33–70 (quick start, client matrix, agent bundle, per-platform notes) · <code class="inline">packages/launcher/README.md</code> (install variants, <code class="inline">--slim</code> behavior, agent installs, the Copilot repo-scoping side effect, platforms). The command strings above are verbatim from those files.</p>
