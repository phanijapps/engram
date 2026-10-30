# engram-mcp

One stdio JSON-RPC 2.0 MCP server that exposes engram's **generic memory +
multi-layer knowledge graph + doc ingestion** to AI agents over a single
`EngramProvider`, fused per project. (RFC-0015, Phase 1 — the generic core. Code
intelligence `scan_repo` + consolidated code tools arrive in Phase 2; the
`get_context` packet + deprecation of the two older servers in Phase 3.)

## Launch

```
engram-mcp --storage <db> [--project <name>] [--ontology <layers.json>] [--taxonomy <concepts.json>]
```

- `--storage` (required) — SQLite store path.
- `--project` — the fused-per-project workspace (default `default`). All writes
  + recalls for one project share one searchable space; unrelated projects never
  blend.
- `--ontology` / `--taxonomy` — JSON config (multi-layer ontology + concept
  taxonomy). Absent → a baked-in generic default runs zero-config. See
  `docs/specs/engram-mcp-core/spec.md`.

Embeddings default to `none` (recall fuses lexical + graph + memory + beliefs;
no vector lane). FastEmbed is a compile-time feature toggle, not enabled here.

## Tools (Phase 1)

| Tool | Purpose |
| --- | --- |
| `ontology_read` / `taxonomy_read` | Active multi-layer config (layers, classes, predicates / concept scheme). |
| `write_memory` / `forget` | Persist / remove a memory observation. |
| `put_entity` / `put_relationship` | Granular KG write (entity upsert by name; kind honored). |
| `store_knowledge` | Bulk distill-write: facts + entities + relationships in one best-effort batch. |
| `recall` | Fused retrieval across memory + knowledge + beliefs + docs; optional `lanes` filter + `limit`. |
| `consolidate` | Reflection + decay → derived beliefs. |
| `index_docs` | Chunk a Markdown/text doc into retrievable sections (docs lane). |

Every tool routes through `EngramProvider` (`engram-integration`); the server
never calls an LLM — extraction is the calling agent's job (see the
`engram-distill` skill in `skills/`).

## Bound code operations

The granular MCP surface exposes `code_repo_register`, `code_bind`, `code_status`, and `code_refresh`.
Register an approved locator, bind one full `refs/heads/...` ref, then use the bound ingestion surface.
The server does not treat a caller path, checkout `HEAD`, or caller-supplied OID as code authority.
`code_refresh` records an on-request observation from the registered locator without fetching or changing a checkout.
`code_status` keeps that observation separate from the indexed generation OID.

The same operations are available without an MCP handshake through `engram-mcp code --op register|bind|status|refresh`.
`engram-mcp backfill-embeddings` drains unembedded document chunks and never rescans or rebuilds bound code.
The ambiguous one-shot `reindex` alias exits with a deprecation diagnostic rather than choosing either operation.

Bound code operations are deny-by-default in native and TypeScript hosts.
Inject the allowed scopes through `codeAuthority.launchScopes` when constructing the provider.
The legacy native `codeBindJson` entry point is removed for callers and returns the typed `LegacyCodeBindUnsupported` denial.
Use the provider path with host authority injection instead.

See [`HOW-TO.md`](HOW-TO.md#bound-code-operation-flow) for tool arguments and [`docs/operator-bound-code-runbook.md`](../../docs/operator-bound-code-runbook.md) for reset and matrix operations.

## Status

The MCP server ships generic memory, knowledge graph, document ingestion, and bound-code transport operations.
SQLite and pgvector recipes expose the supported capability set; unavailable optional lanes report their capability state rather than falling back.
