# Engram MCP configuration

## Select one storage provider

Storage selection does not select a model. SQLite is the unchanged default.
`--backend pgvector` requires a binary compiled with that optional provider;
`ENGRAM_PG_CONNECTION_STRING` supplies its connection string at runtime. The
installer does not save raw PG connection credentials. Helix likewise requires
its packaged mode feature; installing client configuration cannot add a missing
binary feature. `BuildFeatureMissing` is an error, never a fallback to SQLite.

```sh
# Embedded: explicit local storage mode, inference off.
engram-mcp --backend helixdb --helix-mode embedded --helix-store my-store \
  --trusted-root /absolute/owned-root --storage /absolute/owned-root/cache \
  --helix-path /absolute/owned-root/native --project myproject --no-vector

# Service: the named variable is provisioned outside args/config/logs.
engram-mcp --backend helixdb --helix-mode service --helix-store my-store \
  --helix-endpoint http://127.0.0.1:6969 --helix-credential-env HELIX_TOKEN \
  --trusted-root /absolute/owned-root --storage /absolute/owned-root/cache \
  --project myproject --vector --embedding-provider none
```

**Current effective PLT-001 rejects vectors-off service before credentials or
network access.** The service example explicitly enables vectors with provider
`none`, so storage HTTP is used without model initialization. The ratified future
storage/auth exception is not active until its final implementation and proof.
Embedded off mode remains offline. Do not bypass a rejection by switching stores.

| CLI value | Environment fallback |
| --- | --- |
| `--backend sqlite\|pgvector\|helixdb` | `ENGRAM_BACKEND` |
| `--helix-mode service\|embedded` | `ENGRAM_HELIX_MODE` |
| `--helix-store` | `ENGRAM_HELIX_STORE` |
| `--helix-endpoint` | `ENGRAM_HELIX_ENDPOINT` |
| `--helix-credential-env` | `ENGRAM_HELIX_CREDENTIAL_ENV` |
| `--helix-path` | `ENGRAM_HELIX_PATH` |
| `--trusted-root` | `ENGRAM_TRUSTED_ROOT` |
| `--storage`, `--project` | `ENGRAM_STORAGE`, `ENGRAM_PROJECT` |

Flags override environment values. Helix mode and logical store are required.
The service endpoint must be an HTTP(S) origin: host and optional port, root path,
no user/password/query/fragment, including empty `?` or `#`. Embedded paths cannot
be mixed with service settings. Explicit settings for another selected provider
and conflicting repeated backend/mode declarations fail before config writes or
storage opens. Unselected ambient Helix/PG references remain inactive. Native
paths and derived caches are confined under the validated trusted root.

The installer preserves provider/mode/store, endpoint/reference, paths, launch
scope, embedding-provider selection and explicit vector switches:

```sh
engram-mcp install --client claude --backend helixdb --helix-mode embedded \
  --helix-store my-store --trusted-root /absolute/owned-root \
  --storage /absolute/owned-root/cache --helix-path /absolute/owned-root/native \
  --org myorg --domain engineering --subdomain platform --project myproject --no-vector
```

MCP `--org` resolves tenant; domain/subdomain or project resolves workspace. These
flags are scope grants, not remote-user authentication. Preserve them, the logical
store and paths for recall; a different store/scope must not return the old sentinel.
The native SDK additionally accepts the full explicit scope tuple. Public operator
selection does not enable the private instrumented fixture protocol.

## Installed recall hooks

Both recall hooks preserve provider settings through the environment names above.
They also forward `ENGRAM_ORG`, `ENGRAM_DOMAIN` and `ENGRAM_SUBDOMAIN` as scope flags.
The Pi repo file `.pi/engram.json` can supply `storage`, `project`, `backend`,
`helixMode`, `helixStore`, `helixEndpoint`, `helixCredentialEnv`, `helixPath`,
`trustedRoot`, `org`, `domain`, `subdomain`, `embeddingProvider`, and `enableVector`;
environment values win. Store only credential variable names, never their values.

Hooks preserve their existing default of vectors **off**. Set
`ENGRAM_ENABLE_VECTOR=true` or `false` explicitly; Pi also accepts boolean
`enableVector`. True maps to `--vector`, false to `--no-vector`. The ordinary
server's embedding default is separate. Under the current strict service guard,
service hooks need explicit true and `ENGRAM_EMBEDDING_PROVIDER=none`.

Prompts fail open: failure does not inject context or block the coding session.
Claude/Codex heartbeat counters distinguish `failed`, `empty`, `gated`,
`deduplicated` and `injected`; Pi emits these outcomes through
`engram-recall:diagnostic`. Diagnostics omit prompts, raw errors and credentials.
Dedupe is scoped to the selected configuration and project. Code authorization
still fails closed; recall success never grants publication/reset authority.

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
and Helix recipes reject it explicitly without resetting data. Its runtime off switch is
supported. No hot in-process mode or model switching is promised.


## Read-only recall, export and recovery

One-shot Helix recall opens native and derived lexical state read-only, permitting
an overlapping writer without taking its writer lock. Missing/unusable lexical
state reports a lane failure, not a successful empty substitute. Closing a service
client never stops the server; embedded ownership ends only after all retained
handles close and the runtime drains. Uncertain closure blocks replacement.

Archive v1 exports existing projected fields, with no vectors or lossless journal
backup. Documents/concepts inherit source/scheme scope. Dry-run import is
nonmutating validation, not restoration; apply always returns unsupported before
writes. See [archive omissions and exact recovery](../../docs/architecture/helixdb-provider.md).

On `OutcomeUnknown`, preserve the original UUID/receipt and owned data, stop the
original logical mutation and do not replay it or the whole batch. Earlier
acknowledged BestEffort steps remain durable; skipped later steps remain skipped.
A prior-unknown rejection performs no new write. Reconcile the exact durable
receipt and confirm cleanup before replacement; do not delete or reset the store.
