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
- 28 tools the agent can call: scan, search, graph traversal, beliefs, recall, etc.

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

## Tools at a glance (28)

| Group | Tools |
|---|---|
| Ingest | `scan_repo`, `index_docs`, `store_knowledge`, `write_memory` |
| Recall / search | `recall`, `search`, `get_context` |
| Graph | `graph_neighbors`, `graph_subgraph`, `resolve_entity`, `put_entity`, `put_relationship` |
| Belief / hierarchy | `belief_get`, `belief_put`, `belief_retract`, `belief_stale_list`, `hierarchy_path` |
| Lifecycle | `forget`, `consolidate` |
| Code intel | `symbol_context`, `change_impact`, `code_health`, `architecture`, `whats_changed` |
| Config | `ping`, `ontology_read`, `taxonomy_read`, `capability_report` |

Ask Claude “what tools does engram have?” or call `capability_report` to see what’s
wired.

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
- **Rebuild embeddings** — back up the store, then use `maintain` with
  `{"op":"reindex","limit":1024}` in the `full` tool profile until the backlog
  drains. Retain `engram_data.db`, its history and existing vector namespaces.
  A vector rebuild does not require deleting canonical data.

## Configure natural-language code evidence

The `core` and `full` tool profiles expose `code` with `op: "evidence"`.
This operation returns structured implementation excerpts, source hashes/ranges,
supporting context and diagnostics. Its quality acceptance remains tracked in the
[reliable code intelligence spec](../../docs/specs/reliable-code-intelligence/spec.md).
An available operation or scorer does not establish a passing retrieval score.

Provision the admitted local assets explicitly before using vector retrieval or
the cross-encoder. These commands verify the installed manifest's pinned sizes
and hashes; normal model inference reads local files:

```bash
python3 scripts/dev/provision_code_models.py --model bge-small-en-v1.5 --download
python3 scripts/dev/provision_code_models.py --model jina-reranker-v1-turbo-en --download
```

Use `--from-cache /absolute/path/to/huggingface/hub` instead of `--download` to
import matching artifacts from a Hugging Face hub cache root containing
`models--.../snapshots/<revision>/...`. `ENGRAM_MODEL_CACHE` selects the operator-owned asset
directory; its default is `~/.cache/engram/models`. A build without the model
features reports the missing capability. The default MCP build includes
`fastembed` and `cross-encoder-rerank`.

Create an explicitly selected code configuration file, for example
`/absolute/path/to/code.json`:

```json
{
  "profile": "balanced-v1",
  "retrieval": { "result_limit": 8 },
  "evidence": { "token_budget": 4000 }
}
```

```bash
target/release/engram-mcp \
  --storage ~/.engram/myproject --project myproject --tools full \
  --config /absolute/path/to/code.json
```

Omitted fields retain the selected profile's defaults. Settings resolve once at
launch: profile defaults, selected JSON/TOML file, environment, then explicit SDK
fields. `ENGRAM_CODE_CONFIG` selects the file and `ENGRAM_CODE_PROFILE` selects
the profile. A scanned repository's config is not implicitly trusted as launch
configuration. Restart after changing settings.

| Setting | `balanced-v1` | `fast-v1` |
|---|---|---|
| Candidates per enabled lane | 50 | 20 |
| Final primary limit | 10 | 10 |
| Enabled lanes | vector, lexical | vector, lexical |
| Anchored graph hops / neighbor bound | 1 / 20 | 0 / 20 |
| Reranker | local cross-encoder | none |
| Passage body / context token targets | 384 / 64 | 384 / 64 |
| Query instruction | none | none |
| Evidence token budget / per-file limit | 6000 / 4 | 6000 / 4 |
| Supporting-unit limit | 20 | 20 |

The scorer model is `jinaai/jina-reranker-v1-turbo-en`, with query/pair limits
128/1024 tokens, at most 50 scored pairs and batches of 8. The default failure
policy is `degrade`; `error` returns an error instead. Fast mode avoids loading
the scorer. Both profiles share the same passage representation.

Nested environment settings use double underscores and the Rust field names:

```bash
export ENGRAM_CODE_PROFILE=fast-v1
export ENGRAM_CODE__RETRIEVAL__RESULT_LIMIT=8
export ENGRAM_CODE__EVIDENCE__TOKEN_BUDGET=4000
export ENGRAM_CODE__LOCAL_SCORER__ON_FAILURE=error
```

Unknown keys, invalid values and incompatible budgets fail validation. The
generated `CodeIntelligenceConfigOverlay` contract describes the complete field
set. Packets report profile, configuration hash and origins; the provider's
configuration snapshot contains the full effective settings. Index-setting
changes require building the compatible derived namespace. Complete canonical
methods and retained history stay intact.

Call the consolidated tool with a query only and optional allowed reductions:

```json
{"op":"evidence","query":"How does a failed import keep old source from becoming current?","resultLimit":5}
```

Scope and requester come from the trusted host. Query arguments cannot choose
models, index representation or authorization context. Rust callers use
`EngramProvider::code_evidence`; native TypeScript callers use
`provider.codeEvidence(query, trustedSourceContext)`. The HTTP runtime dispatches
the same `code` operation when its launch configuration supplies source context.

Install an independently calibrated artifact with
`--code-calibration /absolute/path/to/calibration.json`; native provider options
use `codeCalibrationPath`, and Rust configuration uses
`with_code_calibration_path`. Calibration binds the actual model, tokenizer,
representation, scoring strategy and configuration. Fast mode needs its own
artifact. Changing a bound setting invalidates the artifact.

Without a matching validated artifact, or after scorer fallback, the packet
reports `insufficient_evidence`. Authorized inspection candidates can still be
present; they are not validated implementation proof. RRF and cross-encoder
scores are ranking values, not confidence probabilities. Insufficiency does not
assert that a feature is absent. Check `diagnostics` for calibration, scorer,
coverage and reindex state, and `sourceFailures` for unavailable retrieval lanes.

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
