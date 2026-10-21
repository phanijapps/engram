# Engram MCP configuration

## Runtime modes

Every supported npm platform ships one executable with local and served embedding
support. There is no separate edition to install.

- Default: local BGE-small embeddings, unless an explicit provider overrides it.
- `--slim`: embeddings off, including model initialization and embedding requests.
- `--no-vector`: compatibility alias for `--slim`.
- `--vector`: enable the configured provider, not necessarily the local model.

The last mode switch wins. Enabled embedding settings resolve CLI > environment >
profile > defaults. Off mode ignores embedding environment/profile settings;
CLI syntax errors (unknown flags, missing values, malformed numbers) still fail.

`ENGRAM_NO_FULL=1` is deprecated: it now requests `--slim` on the same executable.
Explicit CLI mode switches override that environment default. Do not install a
second binary to disable embeddings. `ENGRAM_MCP_BIN` remains a development override.

Slim mode preserves non-vector data and existing vectors; it does not delete model
caches or reduce executable size. Optional embedding-based MMR reranking falls
back to relevance-only without an embedder. Ranking is not identical across modes.

## Local model cache

Local model assets may download from Hugging Face during startup (roughly 130 MB
for BGE-small). For disconnected operation, provision the cache beforehand and set
`FASTEMBED_CACHE_DIR` to its directory, or start with `--slim`.

An unavailable local model can leave non-vector capabilities running with a
warning on stderr. A running server alone does not prove embeddings are working;
check the MCP capability report and embedding diagnostics.

## Served embeddings

These examples use a separate store to avoid mixing existing local vectors with a
new model. Start Ollama and pull its model first if using that provider.

```sh
engram-mcp --storage ~/.engram-ollama \
  --embedding-provider ollama --embedding-model nomic-embed-text

engram-mcp --storage ~/.engram-openai \
  --embedding-provider openai-compatible \
  --embedding-model text-embedding-3-small \
  --embedding-endpoint https://api.openai.com/v1 \
  --embedding-api-key-env OPENAI_API_KEY
```

The OpenAI example reads credentials from the named environment variable; never
put API key values in client args. Ollama defaults to `http://localhost:11434`.
Automatic dimensions are resolved by the provider; explicit dimensions must match
its output. These served providers may send your content over the network.

Equivalent settings: `ENGRAM_EMBEDDING_PROVIDER`, `ENGRAM_EMBEDDING_MODEL`,
`ENGRAM_EMBEDDING_ENDPOINT`, `ENGRAM_EMBEDDING_API_KEY_ENV`,
`ENGRAM_EMBEDDING_DIMENSIONS`. Use `--embedding-config embedding.toml` or
`ENGRAM_EMBEDDING_CONFIG` for a TOML profile:

```toml
[embedding_provider]
provider_type = "ollama"
model = "nomic-embed-text"
```

## Switching and backfill

Keep the same storage path **and project/scope** when switching local → slim →
the same local model. Existing same-space vectors survive. Content ingested while
slim is not automatically embedded at startup: call the scoped MCP `reindex` tool
(or `maintain` with `op: "reindex"`) repeatedly until the backlog is drained.
Failures stay pending and are reported in server diagnostics. No graph rescan is
required for vector backfill.

### Changing embedding spaces or upgrading a legacy store (SQLite)

Different models/providers, dimensions or embedding instructions can make old
vectors incompatible. Legacy no-model stores may also carry a `none` space.
`RequiresReindex` means you need a **vector-only rebuild**, not ordinary backfill.
Never delete `engram_data.db` to repair vectors: it also holds canonical records.

1. Stop all processes using this store and back it up, including any SQLite WAL.
2. Launch once with your intended provider configuration and `--rebuild-vectors`.
   This explicitly discards **all derived vectors in the store**, across projects,
   and adopts the new space. Memories, documents, chunks, graphs and beliefs stay.
   Example for the default local model:
   `engram-mcp --storage ~/.engram --rebuild-vectors`.
3. Backfill **each project/scope** with the MCP `reindex` tool.
4. Remove `--rebuild-vectors` from launch arguments before the next restart,
   otherwise it resets vectors again. Confirm semantic coverage before relying on it.

Rebuild requires available embedding inference and cannot run with `--slim`.
The reset is transactional but backfill is not: failed batches can be retried.
This reset option is currently supported by SQLite only; the optional pgvector
recipe rejects it explicitly without resetting data. Its runtime off switch is
supported. No hot in-process mode or model switching is promised.
