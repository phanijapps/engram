# Engram Codegraph MCP Server

A standalone MCP (Model Context Protocol) server that exposes structural code
analysis — dead-code detection, blast-radius analysis, centrality ranking,
community detection, and more — to any MCP-compatible AI coding agent.

Built entirely on top of [Engram](https://github.com/phanijapps/engram) as a
pure-Rust library. No external services, no API keys, no LLM calls.

## Quick start

```bash
# Build
cargo build -p engram-codegraph-mcp --release

# Run (in-memory store)
./target/release/engram-codegraph-mcp

# Run (persistent file-backed store)
./target/release/engram-codegraph-mcp /path/to/store.db
```

## Agent configuration

### Codex CLI (`~/.codex/config.toml`)

```toml
[mcp_servers.codegraph]
command = "/absolute/path/to/engram-codegraph-mcp"
args = []
```

### Claude Code (`.mcp.json`)

```json
{
  "mcpServers": {
    "codegraph": {
      "command": "/absolute/path/to/engram-codegraph-mcp",
      "args": []
    }
  }
}
```

### Cursor (`~/.cursor/mcp.json`)

```json
{
  "mcpServers": {
    "codegraph": {
      "command": "/absolute/path/to/engram-codegraph-mcp",
      "args": []
    }
  }
}
```

## Tools (17)

### Indexing
| Tool | Description |
|---|---|
| `scan_repo` | Index a repository — returns file/entity/relationship counts |

### Impact analysis
| Tool | Description |
|---|---|
| `dead_code` | Symbols with zero callers |
| `blast_radius` | Transitive callers of a symbol (depth-limited) |
| `dependency_path` | Shortest call path from one symbol to another |

### Architecture overview
| Tool | Description |
|---|---|
| `central_symbols` | PageRank-ranked most-depended-on symbols |
| `bridge_symbols` | Betweenness-ranked chokepoints |
| `call_communities` | Louvain community detection (tightly-coupled clusters) |
| `symbol_context` | 360° view: callers + callees + community for one symbol |
| `repository_stats` | Node + edge counts |

### Temporal scoring
| Tool | Description |
|---|---|
| `temporal_recent` | Rank by recency (exponential decay from `valid_from`) |
| `temporal_impact` | Rank by blast-radius-weighted impact |
| `temporal_compound` | Normalized blend of recency + impact |

### Source-text analysis (pass source code directly)
| Tool | Description |
|---|---|
| `cyclomatic_complexity` | Decision-point counting (text heuristic) |
| `find_endpoints` | HTTP endpoint detection (Express/FastAPI/Actix/etc.) |
| `find_api_calls` | HTTP call-site detection (fetch/axios/requests) |
| `find_entry_points` | Entry-point detection (main/handler/__main__) |
| `process_flow` | Execution-flow trace from an entry point |

## Usage workflow

```
1. scan_repo({ "path": "/abs/path/to/repo" })
   → "Indexed: 1206 files, 14042 entities, 41622 relationships"

2. central_symbols({ "limit": 10 })
   → [{ "name": "create_app", "kind": "function", "file": "src/app.rs", "score": 0.035 }]

3. blast_radius({ "target": "create_app", "depth": 5 })
   → [{ "name": "main", "kind": "function", "file": "src/main.rs" }, ...]

4. dead_code({})
   → [{ "name": "unusedHelper", "kind": "function", "file": "src/helpers.rs" }, ...]
```

## Agent skills

Four workflow skills ship alongside the server (in `codegraph/skills/`):

- **codegraph-first** — "understand this codebase" (60-second overview)
- **codegraph-impact** — "what breaks if I change X?" (blast-radius analysis)
- **codegraph-dead-code** — "find refactoring candidates"
- **codegraph-onboarding** — "I'm new here" (architecture narrative)

Install them with:
```bash
./codegraph/mcp-server/install.sh --codex   # or --claude or --cursor
```

## Architecture

```
MCP Client (Claude Code / Codex / Cursor)
  ↓ JSON-RPC over stdio
engram-codegraph-mcp (this binary)
  ↓ calls directly
engram-codegraph-queries (18 graph operations)
engram-codegraph-temporal (6 scoring modes)
engram-graph-analytics (PageRank / betweenness / Louvain / reachability)
engram-ingest (tree-sitter AST extraction + scanner)
engram-store-knowledge-sqlite (SQLite knowledge graph store)
```

No external services. No network calls. Everything runs locally.

## Limitations

- **Line numbers**: entity source_refs include file paths but not line numbers
  (the extractor sets path but not `start_line`). File path is always available.
- **Co-occurrence edges**: for files without tree-sitter AST call data, the
  extractor falls back to name co-occurrence, which creates noisy edges to
  common tokens (`self`, `Some`, `Ok`). These appear as `kind: "unknown"` in
  results.
- **Dynamic dispatch**: trait objects, callbacks, and event handlers won't
  appear as `calls` edges — they'll look like dead code.
- **Cross-language calls**: FFI and subprocess calls don't appear in the graph.
