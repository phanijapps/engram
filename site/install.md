---
layout: page
title: Install
permalink: /install/
---
<p class="kicker">install · two commands, honest platform notes</p>
<h1>Install engram.</h1>
<p class="lead">One npm command installs the MCP server; one <code class="inline">engram-mcp</code> command registers it with your agent or delivers the full skill bundle. Everything below is the launcher's own documented flow — including the warts.</p>

<h2>Quick start (any MCP host)</h2>
<p>Install the server (alpha) and register with your agent — one command, no JSON editing:</p>

```
npm install -g @engramlabs/mcp@alpha

engram-mcp install --client claude          # Claude Code (project scope)
engram-mcp install --client claude-desktop  # Claude Desktop (user scope)
engram-mcp install --client cursor          # Cursor (project scope)
engram-mcp install --client codex           # Codex CLI (user scope)
engram-mcp install --client pi              # pi (project scope)
engram-mcp install --client copilot         # GitHub Copilot / VS Code
```

<p>The installer registers the installed package version and defaults storage to <code class="inline">~/.engram</code>. Restart your MCP client after registration. Or run directly without installing:</p>

```
npx @engramlabs/mcp@alpha --storage ~/.engram
```

<h2>Embeddings at startup — one install, two modes</h2>

```
engram-mcp --storage ~/.engram          # Local embeddings enabled
engram-mcp --storage ~/.engram --slim   # Embeddings off
```

<p>Local embeddings may download model assets on first launch; inference then runs on your machine. <code class="inline">--slim</code> skips embedding setup and downloads — memory, knowledge, lexical and graph features remain available, but search ranking can differ. It changes runtime behavior, not installation size. To configure a client without embeddings: <code class="inline">engram-mcp install --client claude --slim</code>. Restart to switch modes; content added while embeddings were off needs vector backfill via the MCP <code class="inline">reindex</code> tool after re-enabling.</p>

<h2>Agent bundle — skills, hooks, specialists</h2>

```
engram-mcp install --agent claude    # or codex | copilot | pi | all
engram-mcp install --agent claude --remove   # uninstall (name-tagged)
```

<p>What lands on your machine: <strong>seven skills</strong> (<code class="inline">engram-index</code>, <code class="inline">engram-know</code>, <code class="inline">engram-mem</code>, <code class="inline">engram-distill</code>, <code class="inline">outcome-reporting</code>, and the software-factory pair <code class="inline">engram-factory</code> + <code class="inline">engram-frame</code>), the factory's <strong>14 specialist subagents</strong> in your host's native dialect, <strong>recall hooks</strong> (every prompt recalls relevant memory, gated so irrelevant results stay silent; a kill switch at <code class="inline">ENGRAM_HOOKS=0</code>; fail-open always), and the <strong><code class="inline">red-forman</code></strong> read-only adversarial reviewer. The skill set derives from the package's authored tree at build time — new skills flow with no installer change, and retired skills are removed automatically unless you replaced them. Uninstall is name-tagged: files you replaced or added yourself are left untouched.</p>

<h2>Platform notes — including the warts</h2>
<ul>
  <li><strong>Copilot is repo-scoped, period.</strong> That platform discovers agents only per repository, so <code class="inline">install --agent copilot</code> writes its 15 agent files into the <em>current repository's</em> <code class="inline">.github/agents/</code> even with <code class="inline">--scope user</code>. Run it from the repo that should carry them — and expect that side effect in any repo you are standing in.</li>
  <li><strong>Copilot hooks cannot inject per-prompt context</strong> (platform limitation): session-start recall only.</li>
  <li><strong>Codex</strong> agents use YAML (<code class="inline">~/.codex/agents/*.yaml</code>).</li>
  <li><strong>claude-desktop</strong> hosts MCP registration only — no skills/agents filesystem; <code class="inline">install --agent all</code> prints that limitation.</li>
  <li><strong>pi</strong> gets an in-process extension.</li>
  <li><strong>Prebuilt binaries</strong>: Linux x64/ARM64 (glibc), macOS Apple Silicon, Windows x64. Other platforms require a source build.</li>
</ul>

<div class="note">
  <p><strong>Status reminder:</strong> the package is alpha — install commands pin <code class="inline">@alpha</code> deliberately. Data stays in your chosen storage directory; no telemetry.</p>
</div>

<p class="grounded"><strong>Grounded in:</strong> <code class="inline">README.md</code> lines 33–70 (quick start, client matrix, agent bundle, per-platform notes) · <code class="inline">packages/launcher/README.md</code> (install variants, <code class="inline">--slim</code> behavior, agent installs, the Copilot repo-scoping side effect, platforms). The command strings above are verbatim from those files.</p>
