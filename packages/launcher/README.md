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
The skill set is derived from the package's authored tree at build time —
currently eight: `engram-index`, `engram-know`, `engram-mem`,
`engram-distill`, `engram-bound` (branch-bound index lifecycle),
`outcome-reporting`, plus the software-factory pair `engram-factory` and
`engram-frame`. New skills flow to delivery with no installer change;
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

Data stays in your chosen storage directory. No telemetry. Local inference
requires no embedding service; configuring a remote provider sends embedding
inputs to that service.

[GitHub](https://github.com/phanijapps/engram) · MIT
