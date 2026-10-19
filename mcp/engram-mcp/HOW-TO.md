# HOW-TO — Engram MCP for first-time users

From zero to an MCP server Claude can talk to, in about 10 minutes. No API keys,
no cloud, no LLM inside the server — it runs locally and builds a searchable graph
of your **code + docs**, with the docs linked to the code they describe.

---

## What you get

- A local MCP server (`engram-mcp`) that **indexes your code** (treesitter) and
  **your docs**, then **connects them** (a doc section *describes* a function).
- One SQLite file (`engram_data.db`) in **WAL** mode, so several tools can read it
  at once (the MCP, a gateway, a visualizer).
- 21 tools the agent can call by default: scan, search, graph traversal, beliefs,
  recall, etc. (larger surfaces via `--tools full|all` — see the
  [argument contract](#launch--runtime-arguments-the-contract) below).

---

## Step 1 — Install Rust

Rust toolchain via [rustup](https://rustup.rs):

```bash
# macOS / Linux:
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
# Windows (PowerShell): winget install Rustlang.Rustup   (or see rustup.rs)

source "$HOME/.cargo/env"      # or reopen your terminal
rustc --version                 # sanity check — prints a version
```

You need a C toolchain too (for the SQLite build). macOS has it; Linux:
`sudo apt install build-essential`; Windows: the “Desktop development with C++”
workload in Visual Studio Build Tools.

---

## Step 2 — Get the code and build the release binary

```bash
git clone https://github.com/phanijapps/engram.git engram
cd engram

# Release build (optimized) of the MCP server:
cargo build --release -p engram-mcp
```

When it finishes, the server is at:

```
engram/target/release/engram-mcp        # (engram-mcp.exe on Windows)
```

That single binary is the whole server — copy it anywhere you like.

---

## Step 3 — Pick a storage folder

Everything for a project lives in one folder (the MCP creates it):

```bash
mkdir -p ~/.engram/myproject
```

This is where `engram_data.db` (your graph) will be created.

---

## Step 4 — Ontology + taxonomy (use the SDLC starter)

The server classifies everything against an **ontology** (what kinds of things +
how they may relate) and a **taxonomy** (a concept tree). Two ready-made starters
ship in this folder:

- `examples/sdlc-ontology.json` — 3 layers: **code / process / team**, with
  `within` predicates (`calls`, `depends_on`, `tests`, …) and `across` predicates
  (`realizes`, `describes`, `owns`, `governs`, …).
- `examples/sdlc-taxonomy.json` — a small SKOS-style tree
  (`Software Development → Source Code / Process / …`).

Copy them next to your storage folder so they’re easy to find:

```bash
cp mcp/engram-mcp/examples/sdlc-ontology.json  ~/.engram/sdlc-ontology.json
cp mcp/engram-mcp/examples/sdlc-taxonomy.json  ~/.engram/sdlc-taxonomy.json
```

> **Heads-up on “classes” vs “kinds”.** The ontology `classes` (e.g. `Requirement`,
> `Sprint`) guide how the agent *thinks about* classification. The stored entity
> `kind` is still a built-in variant (`function`, `class`, `struct`, `concept`,
> `api`, …). Process/team things usually map to `concept`. You can edit both JSON
> files freely — they’re read at launch.

---

## Step 4b — Tune the scanner (optional)

`scan_repo` has two tuning lists that are hardcoded by default: which concept
names earn a cross-document `mentions` link, and which files/dirs are always
skipped. For a large or unusual codebase you can override both with a small
JSON file. A checked starter is at `examples/scan.json`. Drop it at your
repo root:

```bash
cp mcp/engram-mcp/examples/scan.json  /path/to/repo/.engram/scan.json
```

…then `scan_repo { "path": "/path/to/repo" }` picks it up automatically. Or
point at it explicitly: `scan_repo { "path": "...", "scan_config": "/abs/scan.json" }`.
A missing or malformed file **soft-fails** to the defaults — it never breaks a
scan; the result text says which source applied.

```json
{
  "concepts": {
    "min_name_length": 10,
    "blocklist":     ["KafkaConsumer", "EventDispatcher"],
    "allowlist":     ["api", "Auth"]
  },
  "deny": {
    "dirs":        ["generated", "vendor", "third_party"],
    "extensions":  ["map", "svg", "proto"]
  }
}
```

- **`concepts`** — cross-document linking. `blocklist` adds project-specific
  words that are common-but-meaningless in *your* repo (merged with the
  built-in ~50 generics). `allowlist` force-links a name even if it’s short or
  blocklisted (checked first; wins). `min_name_length` overrides the default
  8-char threshold. Both lists match case-insensitively.
- **`deny`** — files to skip on top of `.gitignore`/`.ignore` (already honored)
  and the built-in skip list (`node_modules`, `target`, `.db`, `.log`, …).
  `dirs` match path segments **case-sensitively**; `extensions` are
  **case-insensitive** and take the segment after the *last* dot — so use
  `svg`, not `min.js` (a file `app.min.js` has extension `js`).

All fields are optional and additive. With no file, behavior is unchanged.

---

## Step 5 — Wire it into Claude (`.mcp.json`)

In your **project folder** (the one you open with Claude Code), create a file named
`.mcp.json` (note the leading dot). Use **absolute paths** for the binary and files:

```json
{
  "mcpServers": {
    "engram": {
      "command": "/ABSOLUTE/PATH/TO/engram/target/release/engram-mcp",
      "args": [
        "--storage",  "/ABSOLUTE/PATH/TO/.engram/myproject",
        "--project",  "myproject",
        "--ontology", "/ABSOLUTE/PATH/TO/.engram/sdlc-ontology.json",
        "--taxonomy", "/ABSOLUTE/PATH/TO/.engram/sdlc-taxonomy.json"
      ]
    }
  }
}
```

Example (fill in your own paths):

```json
{
  "mcpServers": {
    "engram": {
      "command": "/home/you/engram/target/release/engram-mcp",
      "args": [
        "--storage",  "/home/you/.engram/myproject",
        "--project",  "myproject",
        "--ontology", "/home/you/.engram/sdlc-ontology.json",
        "--taxonomy", "/home/you/.engram/sdlc-taxonomy.json"
      ]
    }
  }
}
```

> **Several projects?** Swap `--project` for the org/domain model:
> `--org acme --domain backend --submodule payments`. `--project` still works as a
> simple workspace name. Data is isolated per workspace.

Now (re)start Claude Code in that folder — it will start the `engram` server. Approve
the MCP server when Claude asks (first run only).

---

## Step 6 — Your first run

Tell Claude something like:

> *“Use the engram tools. Run `scan_repo` on this repo, then `index_docs` on the
> README, and tell me `graph_neighbors` for the main entry function.”*

Typical first moves:

1. **`scan_repo { "path": "/home/you/myproject" }`** — indexes the code
   (functions, classes, call graph). One file comes out: `engram_data.db`.
2. **`index_docs { "content": "<paste a doc>", "path": "README.md" }`** — chunks a
   doc into the `docs` lane.
3. **`search { "query": "auth" }`** — find entities by name.
4. **`graph_neighbors { "name": "login" }`** — see what’s connected to `login`
   (calls, and any doc that `describes` it).
5. **`recall { "query": "how does login work?" }`** — fused answer from code + docs.

Want the agent to build the **semantic** doc↔code links (a *Login* doc → the
`login` function)? Run the **`engram-distill`** skill over your docs after scanning.

---

## Security check — `cargo audit`

Scan dependencies for known CVEs before you trust the build:

```bash
cargo install cargo-audit      # one-time
cargo audit                     # scans Cargo.lock; reports any advisories
```

Run it after `cargo update` or adding deps. (Optional but recommended for any
local tool that handles your code.)

---

## Launch + runtime arguments — the contract

The canonical reference for everything the server accepts. Flags come from
`mcp/engram-mcp/src/config.rs` (the parser is the source of truth; this
table must not drift from it).

### Launch flags (`engram-mcp [flags]`)

| Flag | Effect | Default |
|---|---|---|
| `--storage <path>` | Directory holding the SQLite store(s) | **required** |
| `--project <name>` | Project identity → `scope.workspace` (when no `--domain`) | `"default"` |
| `--org <name>` | Organization → `scope.tenant` | `"default"` |
| `--domain <name>` | Domain → overrides `--project` as workspace | — |
| `--subdomain <name>` | With `--domain`: workspace becomes `domain/subdomain` | — |
| `--ontology <path>` | Multi-layer ontology config (JSON) | baked-in default |
| `--taxonomy <path>` | Taxonomy config (JSON) | baked-in default |
| `--layout single\|multi` | SQLite file layout | `single` |
| `--db-file <name>` | DB filename override | — |
| `--tools core\|full\|all\|consolidated` | Tool profile (21 / 28 / all / 7) | `core` |
| `--backend sqlite\|pgvector` | Engine recipe | `sqlite` |
| `--pg-connection-string <url>` | Postgres URL (pgvector only; env `ENGRAM_PG_CONNECTION_STRING`) | — |
| `--no-vector` | Disable the embedding lane (fast, lexical+graph only) | on with fastembed builds |

**Scope rule** (`scope.rs`): `tenant = --org || "default"`;
`workspace = --domain/--subdomain || --domain || --project`. The scope
dimensions `subject`, `session`, and `environment` exist in the domain
model but are **not settable from the server** — reserved for future
per-request wiring, always `None` today.

### One-shot subcommands (no MCP handshake)

```
engram-mcp recall --storage <dir> --project <p> --query "<text>" [--json] [--limit N]
engram-mcp status --storage <dir> --project <p> [--json]
```

`recall --json` emits `{ok, hit_count, top_score, elapsed_ms, results:
[{kind, score, source_score, lanes, evidence, title, content, path}]}`
— the shape the launcher's recall hook consumes. Errors exit 0 with
`{ok:false, error}` so hooks fail open.

### Runtime environment variables

| Variable | Used by | Effect |
|---|---|---|
| `ENGRAM_STORAGE` | hooks / pi extension | Store dir override |
| `ENGRAM_PROJECT` | hooks / pi extension | Project scope override |
| `ENGRAM_HOOKS` | hooks | `0` = kill switch (every hook exits silently) |
| `ENGRAM_MCP_BIN` | hooks / extension | Explicit binary path override |
| `ENGRAM_RECALL_MIN_SEP` / `_MIN_OVERLAP` | recall hook | Gate thresholds (separation / term overlap) |
| `ENGRAM_RECALL_MAX_CHARS` / `_MAX_ITEMS` | recall hook | Injection budget caps |
| `ENGRAM_DEDUPE_TURNS` | recall hook | Suppress same items for N turns |
| `ENGRAM_STALE_THRESHOLD` | stale-index hook | Files-changed count that triggers the reminder |
| `ENGRAM_PG_CONNECTION_STRING` | server (pgvector build) | Connection-string env alias for the flag |

> **Known skew (tracked):** the hooks default `--project` to the *cwd
> basename* while the server defaults to the literal `"default"` — if you
> launch the server with `--project X` from a directory named `Y`, the
> recall hook queries scope `Y` while the server writes scope `X`. Set
> `ENGRAM_PROJECT` (or run from the matching directory) until the
> server-side env fallback lands.

---

## Tools at a glance (default surface: 21)

The default `--tools core` profile — the highest-leverage set (frequency ×
decision-value × non-redundancy):

| Group | Tools |
|---|---|
| Discover | `recall`, `search`, `explore`, `get_context` |
| Code intel | `symbol_context`, `change_impact`, `architecture`, `whats_changed` |
| Ingest | `scan_repo`, `index_docs`, `store_knowledge`, `write_memory` |
| Knowledge graph | `put_entity`, `put_relationship` |
| Beliefs | `belief_get`, `contradiction_list` |
| Procedures | `procedure_put`, `procedure_list` |
| Lifecycle / health | `forget`, `graph_health` |
| Source | `fetch_content` |

Larger surfaces: `--tools full` (28 — adds maintenance tools) and
`--tools all` (the complete granular set, including `graph_neighbors`,
`belief_put`, `hierarchy_path`, `consolidate`, `ontology_read`, and
friends). `--tools consolidated` is the 7-verb legacy surface. Ask your
agent “what tools does engram have?” to see what's wired.

---

## Troubleshooting

- **“command not found” / server won’t start** — check the `command` path in
  `.mcp.json` is **absolute** and points at `target/release/engram-mcp`.
- **Empty results after scan** — confirm `--storage` is the same folder across runs,
  and `--project`/`--org … --domain …` matches the scope you wrote under.
- **Two tools can’t read the DB at once** — they can now (WAL). Verify with
  `sqlite3 ~/.engram/myproject/engram_data.db "PRAGMA journal_mode;"` → `wal`.
- **Edit the ontology/taxonomy** — change the JSON and **restart** the server
  (they’re read at launch).
- **Start over** — delete `engram_data.db` (and any `-wal`/`-shm` sidecars) and
  re-scan. The data is regenerable.

---

## Learn more

- Design + roadmap: `docs/rfcs/0016-zbot-class-memory-kg-code-as-final-layer.md`
- Build a connected graph from docs: the **`engram-distill`** skill
- All commands/flags: run `engram-mcp` with no args (prints usage)

---

## Running on Postgres (pgvector backend)

The server can open a Postgres+pgvector store instead of SQLite (requires a
build with the feature and a running Postgres — `docs/how-to-pg/`):

```bash
cargo build --release -p engram-mcp --features pgvector
docker compose -f docs/how-to-pg/docker-compose.yaml up -d

target/release/engram-mcp \
  --storage ~/.engram/myproject \
  --project myproject \
  --backend pgvector \
  --pg-connection-string "postgres://engram:engram@localhost:5432/engram"
```

`--pg-connection-string` (or the `ENGRAM_PG_CONNECTION_STRING` env var) is
required with `--backend pgvector`. The schema is applied idempotently on
boot. The capability report (`engram_capability_report`) marks the lanes not
yet wired on Postgres — recall currently fuses memory + beliefs only (the
knowledge/vector/lexical lanes land with the pgvector-backend PS2 work), and
scan-side embedding is a no-op until an embedding provider is wired into the
recipe. Everything else (scans with cross-file `calls` edges, symbol context,
blast radius, memory/belief/procedure writes) works.
