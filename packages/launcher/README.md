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

## Optional agent workflows

```sh
engram-mcp install --agent claude
```

Adds skills, hooks, review workflows, and the factory specialist subagents.
The skill set is derived from the package's authored tree at build time —
currently seven: `engram-index`, `engram-know`, `engram-mem`,
`engram-distill`, `outcome-reporting`, plus the software-factory pair
`engram-factory` and `engram-frame`. New skills flow to delivery with no
installer change; retired skills from older installs are removed
automatically unless you replaced them. `engram-factory` also delivers its
14 specialist subagents in each host's native dialect (claude, codex,
copilot, pi). Use `--scope project` for a repository installation. See the
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
