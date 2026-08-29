# Engram — From Zero to Wow in 3 Commands

> **You are an AI agent, developer, or curious human.** Engram gives you a
> living memory of your codebase — a knowledge graph you can search, ask
> questions about, and watch unfold in 3D. No cloud, no API keys, no Docker
> required to start (it's all local SQLite).

## What you get in 60 seconds

```bash
# 1. Build
cargo build --release -p engram-mcp

# 2. Index a repository (any repo — Java, Rust, TypeScript, Python, Apex…)
./target/release/engram-mcp --storage ~/.engram/myproject --project myproject \
  --no-vector &
echo '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"scan_repo","arguments":{"path":"/path/to/your/repo"}}}' | ./target/release/engram-mcp --storage ~/.engram/myproject --project myproject --no-vector

# 3. See it in 3D
cd engram-cc/backend && ENGRAM_STORAGE=~/.engram/myproject ENGRAM_WORKSPACE=myproject PORT=3001 npx tsx src/index.ts &
cd ../frontend && npx vite --port 5173
# open http://localhost:5173 → the Graph tab → 3D GRAPH
```

**That's it.** You now have an interactive 3D knowledge graph of your codebase.

---

## What you're looking at

The **Graph tab** shows your code as it actually is:

| Element | Meaning |
|---|---|
| **Amber nodes** | Functions/methods — the doers |
| **Green nodes** | Classes/structs/interfaces — the types |
| **Violet nodes** | API endpoints — the surface |
| **Node size** | How connected it is (degree) |
| **Lines** | Actual call relationships (resolved, not guessed) |
| **White glow** | Your selected symbol |

**Drag to orbit. Scroll to zoom. Click a node** to see its detail panel —
what calls it, what it calls, where it lives in the codebase.

---

## The three views

| View | What it shows | When to use it |
|---|---|---|
| **3D GRAPH** (default) | The actual symbol graph — every function, class, and their call relationships in an immersive force-directed 3D layout | Exploring, demos, understanding overall structure |
| **2D GRAPH** | Same data, flat canvas — faster for precise clicking | When you need to select specific nodes |
| **COMMUNITIES** | Louvain-clusted communities (module/group detection) | Seeing the natural boundaries in your codebase |

---

## What you can ask

Once indexed, the MCP server exposes 44 tools. The ones you'll use most:

### Search (find things)
```
search "authentication flow"           → ranked results with source files
recall "how does login work"           → fused multi-lane retrieval
```

### Understand (see relationships)
```
symbol_context "UserController"         → who calls it, what it calls
change_impact "DatabaseService"         → blast radius of a change
graph_neighbors "PaymentService"        → what's connected to it
```

### Assess (judge health)
```
code_health                             → dead code + repository stats
architecture                            → central symbols, bridges, communities
whats_changed                           → recent churn (after ≥2 scans)
```

### Remember (persist knowledge)
```
write_memory "We decided to use Redis for caching because..."
belief_put "The auth system" "JWT tokens are the standard" 0.9
```

---

## Languages supported

Tree-sitter grammars for **13 languages**: Rust, TypeScript/JavaScript/TSX,
Python, Java, Kotlin, C/C++/C#, Go, PHP, Bash, Perl, Scala, Swift, and
**Salesforce Apex** (classes + triggers).

---

## Power features

### Incremental re-scans
Re-scanning an unchanged repo takes **0.1 seconds** (the manifest tracks
what changed). Changed files are the only ones re-ingested.

### Embedding (vector search)
```bash
# Start WITHOUT --no-vector to enable local FastEmbed BGE-small embeddings
./target/release/engram-mcp --storage ~/.engram/myproject --project myproject
```
The first scan embeds chunks into a vector index; `reindex` drains the
backlog with content-hash dedup (identical texts share vectors — 69% reuse
measured on a real store).

### Postgres backend
```bash
./target/release/engram-mcp --storage ~/.engram/myproject \
  --backend pgvector --pg-connection-string "postgres://user:pass@localhost:5432/db"
```
Same API, same tools, different engine. See the
[migration runbook](guides/how-to/migrate-sqlite-to-pg.md).

### The Ask tab (agentic RAG)
The web UI's **Ask** tab lets an LLM autonomously call `recall`,
`list_memories`, and `graph_overview` to answer questions about your code —
you see the full tool-call trace.

---

## Architecture (the 30-second version)

```
┌──────────────┐     ┌─────────────────┐     ┌──────────────┐
│  Your Repo   │────▶│  engram-mcp     │────▶│  SQLite /    │
│  (any lang)  │     │  (tree-sitter)  │     │  Postgres    │
└──────────────┘     └────────┬────────┘     └──────┬───────┘
                              │                      │
                              ▼                      ▼
                     ┌─────────────────┐     ┌──────────────┐
                     │  Knowledge Graph │────▶│  3D Web UI   │
                     │  (entities+edges)│    │  (engram-cc) │
                     └─────────────────┘     └──────────────┘
```

- **Core** (`core/`): storage-neutral Rust crates — domain types, ports,
  retrieval composition, consolidation. No SQL, no engine names.
- **Adapters** (`adapters/`): one crate per engine (SQLite, pgvector,
  Tantivy, FastEmbed). Swap by config string.
- **MCP server** (`mcp/engram-mcp`): 44 tools over stdio JSON-RPC — the
  agent surface. Works with Claude, Cursor, pi, or any MCP client.
- **Web UI** (`engram-cc/`): React + three.js — the human surface with
  3D graph, search, memory, maintenance, and the Ask tab.

---

## Troubleshooting

| Symptom | Fix |
|---|---|
| "No communities" / empty graph | Scan a repo first — the store starts empty |
| Scan times out | Remove `--no-vector` and re-scan (the vector lane adds time) |
| UI shows no data | Check `ENGRAM_WORKSPACE` matches the `--project` flag exactly |
| "no embedding provider" | Build with `--features pgvector,fastembed` for vector support |

---

## What's next

- [README](../../README.md) — the full product story
- [Connect via MCP](connect-via-mcp.md) — wire engram into your AI agent
- [Migration](migrate-sqlite-to-pg.md) — move between storage engines
- [Backup & Restore](backup-restore.md) — protect your knowledge graph
- [Architecture](../architecture/overview.md) — the deep dive

---

*This guide improves with every change. If you add a feature, update the
relevant section — the docs gate enforces it.*
