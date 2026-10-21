# @engramlabs/mcp

Local-first **knowledge & memory layer** for AI agents — via MCP.

Durable memory, a code graph, beliefs, and governed retrieval in SQLite
files on your machine. No cloud, no telemetry.

## Quick start

```bash
npm install -g @engramlabs/mcp@alpha
engram-mcp install --agent claude    # skills + hooks + red-forman subagent
```

Hosts: `claude` · `claude-desktop` · `cursor` · `codex` · `pi` · `copilot` · `all`

**Project scope** — install skills, hooks, and agents into the repo
(committed, shared with the team) instead of your home directory:

```bash
cd your-repo
engram-mcp install --agent claude --scope project
```

Lands in `.claude/`, `.agents/skills/` (Codex + Copilot + pi read this
cross-tool dir), `.codex/hooks.json`, `.github/hooks/`, `.pi/`.

MCP server only (no skills/hooks): `engram-mcp install --client <host>`

Or run directly: `npx @engramlabs/mcp@alpha --storage ~/.engram`

### Manual config

```json
{
  "mcpServers": {
    "engram": {
      "command": "npx",
      "args": ["-y", "@engramlabs/mcp@alpha", "--storage", "~/.engram"]
    }
  }
}
```

## What your agent gains

| | |
|---|---|
| **Memory** | remember decisions, gotchas, invariants — recall them automatically |
| **Code graph** | index a repo → symbols, blast radius, architecture |
| **Skills** | `engram-index/learn/search/plan/implement` — evidence-labeled workflows |
| **Auto-recall** | hooks inject relevant memory per prompt; irrelevant stays silent |
| **Adversarial review** | `red-forman` subagent — severity-tagged findings before you ship |

## Platforms

Linux x64/ARM64 · macOS Apple Silicon · Windows x64 — binaries install
automatically per machine.

## Local embeddings (the `-full` variant)

The default install uses the **slim** binary: all local-first features work,
but semantic (vector) recall needs a remote embedding endpoint configured.
For **fully offline embeddings**, install the `-full` platform package for
your machine alongside the launcher — it bundles fastembed with a local
BGE-small model:

```bash
npm install -g @engramlabs/mcp @engramlabs/mcp-linux-x64-gnu-full
# darwin ARM:  @engramlabs/mcp-darwin-arm64-full
# linux ARM:   @engramlabs/mcp-linux-arm64-gnu-full
# win32 x64:   @engramlabs/mcp-win32-x64-full
```

The launcher prefers the `-full` binary when present (`ENGRAM_NO_FULL=1`
forces slim). The embedding model
(~130 MB) downloads from Hugging Face on **first use** and is cached —
behind restricted networks point `FASTEMBED_CACHE_DIR` at a pre-populated
cache directory.

## Privacy

All data in SQLite under `~/.engram`. No telemetry, no cloud. Delete
the directory = everything is gone.

## Links

[GitHub](https://github.com/phanijapps/engram) · [Setup guide](https://github.com/phanijapps/engram/blob/main/mcp/engram-mcp/HOW-TO.md) · MIT
