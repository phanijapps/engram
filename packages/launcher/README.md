# @engramlabs/mcp

Local-first **knowledge & memory layer** for AI agents — via MCP.

Durable memory, a code graph, beliefs, and governed retrieval in SQLite
files on your machine. No cloud, no telemetry.

## Quick start

```bash
npm install -g @engramlabs/mcp@alpha
engram-mcp install --agent claude    # skills + hooks + red-forman subagent
```

The default install ships **local embeddings** (the fastembed-equipped
`-full` binary) — semantic recall works out of the box, fully offline.
A slim no-model binary is the fallback (`ENGRAM_NO_FULL=1`, see
[Embeddings](#embeddings)).

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

## Embeddings

Embeddings give `search`/`recall` a **semantic lane**: results matched by
meaning, not just keywords — "how do retries back off" finds the exponential
backoff memory even when those exact words never appear.

### 1. Local & offline — the default

`npm install -g @engramlabs/mcp` installs the **`-full`** binary for your
machine (fastembed + local BGE-small model) as an automatic
optionalDependency — no extra step. Platforms: Linux x64/ARM64 + Apple
Silicon today; Windows ships the slim binary until its `-full` build lands
(an upcoming release); Intel Macs have no prebuilt binaries (build from
source or use the slim variant for your platform once published). The embedding model (~130 MB) downloads
from Hugging Face on **first use** and is cached — behind restricted
networks point `FASTEMBED_CACHE_DIR` at a pre-populated cache directory.

Variants, if you ever need them explicitly:

```bash
npm install -g @engramlabs/mcp-linux-x64-gnu-full   # the default binary
npm install -g @engramlabs/mcp-linux-x64-gnu         # slim fallback (no model)
```

`ENGRAM_NO_FULL=1` forces the slim binary even when `-full` is installed
(e.g. to skip the first-use model download).

### 2. Remote — Ollama or any OpenAI-compatible endpoint

An alternative to the bundled local model:

```bash
# Ollama (endpoint defaults to http://localhost:11434)
engram-mcp --embedding-provider ollama --embedding-model nomic-embed-text

# OpenAI-compatible (endpoint + API key read from the named env var)
engram-mcp --embedding-provider openai \
  --embedding-model text-embedding-3-small \
  --embedding-endpoint https://api.openai.com/v1 \
  --embedding-api-key-env OPENAI_API_KEY
```

Same settings as environment variables (no flags in your MCP config):
`ENGRAM_EMBEDDING_PROVIDER`, `ENGRAM_EMBEDDING_MODEL`,
`ENGRAM_EMBEDDING_ENDPOINT`, `ENGRAM_EMBEDDING_API_KEY_ENV`,
`ENGRAM_EMBEDDING_DIMENSIONS`.

### 3. Remote via profile file

```toml
# embedding.toml — passed via --embedding-config (or ENGRAM_EMBEDDING_CONFIG)
[embedding_provider]
provider_type = "ollama"
model = "nomic-embed-text"
```

### Disabling the vector lane

`--no-vector` turns embeddings off entirely (lexical + graph lanes only —
fastest startup, fully deterministic). Re-enable with `--vector`.

## Privacy

All data in SQLite under `~/.engram`. No telemetry, no cloud. Delete
the directory = everything is gone.

## Links

[GitHub](https://github.com/phanijapps/engram) · [Setup guide](https://github.com/phanijapps/engram/blob/main/mcp/engram-mcp/HOW-TO.md) · MIT
