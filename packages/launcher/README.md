# @engramlabs/mcp

Local-first memory, knowledge and code retrieval for AI agents, via MCP.
Remember decisions, search documents, and navigate code relationships in one store.

## Install

```sh
npm install -g @engramlabs/mcp
engram-mcp install --client claude
```

Other clients: `claude-desktop`, `cursor`, `codex`, `pi`, `copilot`.
The installer registers the installed package version and defaults storage to
`~/.engram`. Restart your MCP client after registration.

## Choose embeddings at startup

One install, no editions or extra packages:

```sh
engram-mcp --storage ~/.engram          # Local embeddings enabled
engram-mcp --storage ~/.engram --slim   # Embeddings off
```

Local embeddings may download model assets on the first launch; inference then
runs on your machine. `--slim` skips embedding setup and downloads. Memory,
knowledge, lexical and graph features remain available, but search ranking can differ.
It changes runtime behavior, not installation size.

To configure a client without embeddings:

```sh
engram-mcp install --client claude --slim
```

Restart to switch modes, keeping the same storage path and project. Content
added while embeddings are off needs vector backfill using the MCP `reindex`
tool after re-enabling them.

## Selected storage and recall

SQLite remains the default. A binary compiled with the appropriate optional
provider supports `--backend pgvector` or `--backend helixdb`. Helix requires an
explicit service/embedded mode and logical store. Selection errors never open a
substitute SQLite store. Installing flags does not add a missing binary feature.

```sh
engram-mcp install --client claude --backend helixdb --helix-mode embedded \
  --helix-store my-store --trusted-root /absolute/owned-root \
  --storage /absolute/owned-root/cache --helix-path /absolute/owned-root/native \
  --project myproject --no-vector
```

For service, configure an HTTP(S) origin and `--helix-credential-env` variable
**name**, never a secret value in arguments. Current effective PLT-001 still denies
service with vectors off; early service use explicitly chooses `--vector
--embedding-provider none`. The future ratified storage/auth exception is not yet
an active permission. Embedded vectors-off storage remains offline.

Installed hooks forward provider/mode/store and launch scope. Their default stays
vectors off; `ENGRAM_ENABLE_VECTOR=true|false` selects the explicit choice. Preserve
org/domain/subdomain/project with the logical store to avoid wrong-store recall.
Prompt hooks fail open with distinct failure/empty/gated/deduplicated diagnostics;
code authority stays fail closed. See [configuration](CONFIGURATION.md) for flags,
environment precedence, trusted paths and read-only recall.

Archive v1 is a projected export. Dry-run validates without writes; import apply
is explicitly unsupported. Unknown write outcomes retain the original UUID and
receipt: stop, preserve data and reconcile; never blindly replay a batch or reset
a store to clear the error.

## Branch-bound code indexing

Code indexes are pinned to a **canonical branch** (default `refs/heads/main`),
not your checkout. Indexing reads committed bytes of the bound branch; a
dirty working tree or a colleague's feature branch never contaminates the
index. One branch per `(project, repository)`.

```sh
engram-mcp code --op register --repo myrepo --path /abs/path/to/repo --json
engram-mcp code --op bind    --repo myrepo --branch-ref refs/heads/main --json
engram-mcp code --op rebuild --repo myrepo --request-id <unique-key> --json
engram-mcp code --op status  --repo myrepo --json
engram-mcp code --op refresh --repo myrepo --json
```

Read tools (`search`, `recall`, `fetch_content`, `symbol_context`) honor the
binding automatically — never pass a branch to read tools. `fetch_content`
returns byte-exact committed content (sha256-verified against the bound
revision), even if the file is dirty or deleted locally.

After every merge to main: `refresh` then `rebuild` to keep the index current.

### Code reset (destructive — owner gate required)

```sh
engram-mcp code --op reset-preview --repo myrepo --json > preview.json
# Show the owner: target count, ambiguity list, preservation inventory.
# Get an explicit go-ahead, then:
engram-mcp code --op reset-apply --journal "$(cat preview.json)" --decision "<owner-gate>" --json
```

Ambiguous legacy targets block the preview. Ordinary memory and knowledge are
never touched by a code reset.

## Ontology packs

Classify knowledge under a **standing domain vocabulary** — not ad-hoc tags.
Pin an ontology and taxonomy at launch, or install a standard-domain pack:

```sh
engram-mcp --storage ~/.engram --project myproject \
  --ontology ~/.engram/myproject-ontology.json \
  --taxonomy ~/.engram/myproject-taxonomy.json
```

Standard-domain packs (33 pinned: FIBO for banking, schema.org for travel,
healthcare, logistics, and 29 more):

```sh
engram-mcp pack list                          # browse the catalogue
engram-mcp pack install fibo --storage ~/.engram --project banking
engram-mcp pack status --storage ~/.engram
```

Installed packs add a vocabulary layer for the `engram-distill` skill's
classification — extracted entities, relationships, and facts are typed
against the pack's real-world vocabulary (e.g. a banking `Loan`, not a
generic `Concept`). No pack is required; the default technical/domain/business
ontology ships built-in.

## Optional agent workflows

```sh
engram-mcp install --agent claude
```

Adds skills, hooks, review workflows, and the factory specialist subagents.
The skill payload is generated from the authored `.agents/skills` tree at
build time. Only `engram-*` skills and Engram's `outcome-reporting` companion
ship; unrelated local skills such as `better-*` remain local. Generation,
parity checks and conformance tests share `scripts/skill-scope.cjs` for this
boundary. Every selected skill's full folder is copied, including the factory
eval dataset. The installer reads this packaged payload, not the developer's
local skills directory. New Engram skills flow to delivery with no installer change;
retired skills from older installs are removed automatically unless you
replaced them. Uninstall with `engram-mcp install --agent <host> --remove`
— removal is name-tagged, so files you replaced or added yourself are left
untouched. `engram-factory` also delivers its 14 specialist subagents in
each host's native dialect (claude, codex, copilot, pi). Use `--scope
project` for a repository installation. Copilot is the exception at user
scope: that platform discovers agents only per repository, so `install
--agent copilot` writes its 15 agent files (14 specialists plus
`red-forman.md`, refreshing copies carrying our name tag) into the
**current repository's** `.github/agents/` even with `--scope user` — run
it from the repo that should carry them, and expect that side effect in any
repo you are standing in. See the
[setup guide](https://github.com/phanijapps/engram/blob/main/mcp/engram-mcp/HOW-TO.md).

## Platforms and configuration

Prebuilt binaries: Linux x64/ARM64 (glibc), macOS Apple Silicon, Windows x64.
Other platforms require a source build.

For remote embeddings, model caching and safe model changes, see
[Configuration](https://github.com/phanijapps/engram/blob/main/packages/launcher/CONFIGURATION.md).

Canonical data stays in the selected local store or configured storage service.
Derived local caches stay under the trusted root. No telemetry. Local inference
requires no embedding service; configuring a remote provider sends embedding
inputs to that service.

[GitHub](https://github.com/phanijapps/engram) · MIT
