# @engramlabs/mcp

Local-first agentic **knowledge & memory layer** for AI agents — via MCP.

Engram gives your agent durable memory, source-grounded knowledge, a code
graph, beliefs with contradiction detection, and governed retrieval — all in
SQLite files on your machine. No cloud, no telemetry, no network calls on
the default path.

## Install

```bash
npm install -g @engramlabs/mcp@alpha
```

## Enable your MCP host + agent bundle

One command — no JSON editing:

```bash
engram-mcp install --agent claude          # skills + hooks + red-forman subagent
# or: codex, copilot, pi, all
```

MCP server registration only:

```bash
engram-mcp install --client claude          # Claude Code
engram-mcp install --client claude-desktop  # Claude Desktop
engram-mcp install --client cursor          # Cursor
engram-mcp install --client codex           # Codex CLI
engram-mcp install --client pi              # pi
engram-mcp install --client copilot         # GitHub Copilot / VS Code
```

Or run directly:

```bash
npx @engramlabs/mcp@alpha --storage ~/.engram
```

### Manual configuration

Add to your `.mcp.json` (or your host's config):

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
| **Remember** | `write_memory`, `store_knowledge` — decisions, observations, facts |
| **Recall** | `recall`, `search`, `get_context` — fused multi-lane retrieval |
| **Understand code** | `scan_repo` → `symbol_context`, `change_impact`, `architecture` |
| **Read source** | `fetch_content` — stitch any indexed file's content back |
| **Track beliefs** | `belief_get`, `contradiction_list` — derived stances, open tensions |
| **Runbooks** | `procedure_put`, `procedure_list` — replayable with success/fail counters |
| **Correct** | `forget`, `graph_health` — remove wrong data, monitor health |
| **Adversarial review** | `red-forman` subagent — severity-tagged findings on any diff before you declare done |

## Platforms

| OS | Architecture | Binary size |
|---|---|---|
| Linux | x86-64 | ~49 MB |
| Linux | ARM64 | ~49 MB |
| macOS | Apple Silicon | ~49 MB |
| Windows | x86-64 | ~49 MB |

Platform binaries install automatically as optional dependencies — you
don't need to pick one.

## Privacy

- All data lives in SQLite files under `--storage` (default: `~/.engram`)
- No telemetry, ever
- No cloud calls on the default path
- Delete the storage directory = everything is gone

## Learn more

- [GitHub](https://github.com/phanijapps/engram) — source, architecture, docs
- [HOW-TO](https://github.com/phanijapps/engram/blob/main/mcp/engram-mcp/HOW-TO.md) — full setup guide
- [RFC-0021](https://github.com/phanijapps/engram/blob/main/docs/rfcs/0021-npm-distribution-rollout.md) — distribution design

## License

MIT
