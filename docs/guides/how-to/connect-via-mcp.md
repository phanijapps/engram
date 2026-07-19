# Connect to engram via MCP

> Engram ships two MCP servers — one for **agent memory**, one for the
> **codegraph** — so any MCP client (Claude Desktop, Cursor, Copilot, Codex) can
  read and write engram over stdio JSON-RPC 2.0. For build/run commands, see the
> [build guide](./build-and-run.md); for what engram does, see the
> [architecture overview](../../architecture/overview.md).

The Model Context Protocol (MCP) lets a host (your editor or agent runtime) spawn
a server as a subprocess and call its tools. Engram's two servers are workspace
binaries, not network services: the client starts them, talks JSON-RPC over their
stdin/stdout, and they own the storage connection.

## Server 1 — memory MCP (`engram-memory-mcp`)

Exposes engram's memory operations against a storage path. **6 tools**:

| Tool | What it does | Key args |
| --- | --- | --- |
| `write_memory` | Persist a fact, observation, or episode to the memory layer. | `content`, `tenant?` |
| `recall` | Unified recall — one query fans across facts + graph + vector + lexical + beliefs, fused via RRF. | `query`, `tenant?` |
| `forget` | Delete, redact, tombstone, or archive a memory by ID. | `target_id`, `mode?` (`delete`\|`redact`\|`tombstone`\|`archive`), `tenant?` |
| `put_entity` | Add an entity to the knowledge graph. | `name`, `kind?`, `tenant?` |
| `put_relationship` | Add a relationship between two entities. | `subject`, `object`, `predicate`, `tenant?` |
| `consolidate` | Run consolidation (reflection + decay): synthesize derived beliefs from active memories + expire past-deadline memories. | `tenant?`, `dry_run?` |

`recall` is the workhorse: a single natural-language query is fanned across every
retrieval mode and fused into one ranked context packet, with provenance. An agent
calls `write_memory` to remember, `recall` to bring relevant memory back, and
`consolidate` periodically to compress and expire.

## Server 2 — codegraph MCP (`engram-codegraph-mcp`)

Exposes the on-top codegraph layer ([RFC-0012](../../rfcs/)) over a knowledge
store. **23 tools**, grouped by family. The flow is **index, then query**: call
`scan_repo` once, then query/rank.

| Family | Tools |
| --- | --- |
| **Indexing** | `scan_repo`, `search_code`, `repository_stats`, `capability_report` |
| **Query** | `dead_code`, `blast_radius`, `dependency_path`, `symbol_context`, `process_flow`, `find_entry_points` |
| **Ranking** | `central_symbols` (PageRank), `bridge_symbols` (betweenness), `call_communities` (Louvain), `cyclomatic_complexity`, `most_complex` |
| **HTTP / endpoint** | `find_endpoints`, `find_api_calls`, `match_api_topology` |
| **Temporal** | `temporal_recent`, `temporal_impact`, `temporal_compound`, `temporal_overview`, `temporal_directional` |

Typical use: "who breaks if I change `parseRequest`?" → `blast_radius`; "find the
shortest call path from the handler to the DB" → `dependency_path`; "what changed
recently that matters most?" → `temporal_compound`. (Tool list verified against
`codegraph/mcp-server/src/main.rs` `tool_list()`; re-check at write time — it may
have grown.)

## Launch a server

```bash
# Memory MCP — agent memory operations against a storage path
cargo run -p engram-memory-mcp -- <storage-path>

# Codegraph MCP — in-memory store, or file-backed with a path
cargo run -p engram-codegraph-mcp -- /path/to/store.db
```

Both speak stdio JSON-RPC 2.0. You normally do **not** run these by hand — your
client spawns them from its MCP config (below).

## Client configs

The MCP stdio config is the same shape everywhere: a `command` + `args` that
start the server. Use the `cargo run` form during development, or point `command`
at a built binary (`cargo build -p engram-memory-mcp` → `target/release/engram-memory-mcp`)
for a faster cold start.

```json
{
  "mcpServers": {
    "engram-memory": {
      "command": "cargo",
      "args": ["run", "-p", "engram-memory-mcp", "--", "/absolute/path/to/store"],
      "env": {}
    },
    "engram-codegraph": {
      "command": "cargo",
      "args": ["run", "-p", "engram-codegraph-mcp", "--", "/absolute/path/to/codegraph.db"]
    }
  }
}
```

Where each client expects this file:

| Client | Config location |
| --- | --- |
| **Claude Desktop** | `claude_desktop_config.json` (macOS: `~/Library/Application Support/Claude/`; Linux: `~/.config/Claude/`) |
| **Cursor** | `.cursor/mcp.json` in the project, or the global MCP settings |
| **GitHub Copilot** (VS Code) | `.vscode/mcp.json` (or the editor's MCP settings UI) |
| **Codex / other** | the client's `mcpServers` stdio config |

After saving, restart the client; it lists the engram tools alongside its
built-ins. (Paths are the well-known per-client locations at the time of writing —
prefer your client's current MCP docs if a path differs.)

## MCP vs the N-API library vs the Rust facade

Three ways to use engram, one underlying engine:

| Surface | Use when | Shape |
| --- | --- | --- |
| **MCP server** | An agent/editor should call engram as a tool, no code on your side | stdio JSON-RPC; client spawns the binary |
| **N-API / TypeScript SDK** (`@engram/node`, `@engram/client`) | A JS/TS app embeds engram in-process | native module over the Rust core; same `EngramProvider` surface |
| **Rust facade** (`engram-integration` → `EngramProvider`) | A Rust app embeds engram as a library | direct `Arc<dyn>` handles; zero transport |

Every capability is reachable from all three (surface parity, [AGENTS.md](../../../AGENTS.md)).
Pick by where your code runs; the storage backend is selected separately by config.

## See also

- [Build and run](./build-and-run.md) — prerequisites + build/test commands.
- [Architecture overview](../../architecture/overview.md) — the pipeline + layers.
- [RFC-0012](../../rfcs/) — the codegraph layer design.
- [`README`](../../../README.md) — project overview, use cases, and the doc map.
- `memory/mcp-server/` and `codegraph/mcp-server/` — the server source.
