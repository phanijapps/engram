---
name: engram-bound
description: Manage the project-bound branch index — bind a repository to ONE canonical branch (register → bind → rebuild), keep it current after merges to main (refresh + rebuild), and reset code data through the owner-gated preview/apply flow when needed. Includes keeping the installed ontology pack classification current after reindexing. Trigger on "reindex the repo", "index this branch", "after merging to main" (standing rule), "bind the project to main", "which branch is indexed", "reset/rebuild the code index", or when code answers seem stale relative to main.
---

# engram-bound

The code index is **(project, repository, branch)** — pinned to one canonical
branch per project, independent of any developer's checkout. The indexed bytes
are always the **committed bytes of the bound branch**, never the working tree.

## Contract

1. One canonical branch per `(project, repository)` — default `refs/heads/main`.
   Never bind a feature branch; never switch a developer checkout to index.
2. Rebuilds index the bound ref's pinned commit only. Uncommitted/dirty bytes
   are never indexed. Rebinding to a different ref on the same repo is a
   deliberate act (`code bind` again) and retires the old generation's view.
3. The reset flow is destructive and owner-gated: ALWAYS preview, get an
   explicit human go-ahead, then apply. Never run `reset-apply` unprompted.
4. After any merge to main, refresh + rebuild so the index and knowledge stay
   current (standing project rule). Then re-distill drift (below).
5. Read tools (`search`, `recall`, `fetch_content`, `symbol_context`) honor the
   binding automatically — you never pass a branch to read tools.

## The store and binary

The live store for this agent is `~/.engram/agentzero` (project `agentzero`),
served by the repo's own binary. All one-shot commands accept
`--storage <dir> --project <name>`. If `.mcp.json` wires an `engram` server,
the stdio tools (`code_repo_register`, `code_bind`, `code_rebuild`,
`code_status`, `code_refresh`, `code_reset_*`) are the same operations.

## Lifecycle (one-shot forms — run from any checkout of the repo)

```bash
ENGRAM="path/to/engram-mcp --storage ~/.engram/agentzero --project agentzero"

# 1. REGISTER the repository (local mode; the path is a locator, display only)
$ENGRAM code --op register --repo <repo-id> --path /abs/path/to/repo --json

# 2. BIND to the canonical branch (idempotent for the same ref)
$ENGRAM code --op bind --repo <repo-id> --branch-ref refs/heads/main --json

# 3. REBUILD — acquires the branch's committed bytes, publishes a generation.
#    --request-id is an idempotency key: reuse it to safely retry.
$ENGRAM code --op rebuild --repo <repo-id> --request-id <unique-key> --json

# 4. STATUS / FRESHNESS (durable, never from checkout HEAD)
$ENGRAM code --op status --repo <repo-id> --json
$ENGRAM code --op refresh --repo <repo-id> --json   # observes the configured ref
```

Rebuild output reports `files / documents / entities / relationships`,
`commitOid` (what got indexed), and `snapshotDigest`. Oversized vendored blobs
are skipped with a stderr notice — expected for generated parser sources.

## After every merge to main (standing rule)

```bash
cd /path/to/repo && git pull --ff-only
$ENGRAM code --op refresh --repo <repo-id> --json     # observe new HEAD
$ENGRAM code --op rebuild --repo <repo-id> --request-id post-merge-<sha> --json
$ENGRAM code --op status  --repo <repo-id> --json     # confirm INDEXED == main HEAD
```

Then keep knowledge current (hand off to the sibling skills):
- **Code drift** → the rebuild above covers it; verify with a `search` for a
  symbol the merge touched.
- **Docs/decisions drift** → `engram-distill` for the changed docs, under the
  installed **ontology pack** vocabulary (see below).

## Reset flow (destructive — owner gate required)

Use when code data must be removed (wrong branch indexed historically, poisoned
generation, or a deliberate wipe before re-binding):

```bash
# 1. PREVIEW — hash-bound, lists every code-owned target + preserved ordinary data
$ENGRAM code --op reset-preview --repo <repo-id> --json > preview.json
# 2. STOP. Show the owner: target count, ambiguity list (must be empty),
#    preservation inventory. Get an explicit go-ahead tied to this preview.
# 3. APPLY with the owner's decision string (idempotent per preview hash)
$ENGRAM code --op reset-apply --journal "$(cat preview.json)" --decision "<owner-gate>" --json
# 4. REBIND + REBUILD from step 1 of the lifecycle.
```

Rules: `ambiguity` non-empty → BLOCK, ask the owner, do not apply. Never edit
the journal between preview and apply (hash-bound). Partial retries are safe
(`reset-resume` with the journal). Ordinary knowledge/memory is never touched.

## Ontology packs (classification during distillation)

The store's ontology/taxonomy files (`~/.engram/<project>-ontology.json`,
`-taxonomy.json`) pin the standing vocabulary for ALL knowledge writes. After a
rebuild or doc distillation:

- Route entity/relationship classification through `engram-distill`, which
  classifies against the installed pack (for this repo: the SDLC/agent
  vocabulary; other repos may carry FIBO, schema.org, etc. via `engram-domain`).
- Never invent new top-level classes during distillation — find the closest
  pack class; if none fits, flag it for the owner instead of forking vocabulary.
- New pack needed for a domain-heavy repo → run `engram-domain` (install pack →
  reindex → distill under it). Packs are per-store; switching packs is a
  re-distillation, not a rewrite of stored facts.

## Verifying the binding is healthy

```
"which branch is indexed?"      → code --op status: BOUND ref + INDEXED sha
"did the merge get indexed?"    → status INDEXED == git rev-parse main
"why is this symbol stale?"     → refresh, then rebuild if OBSERVED != INDEXED
"wrong branch got indexed"      → reset flow (owner gate), then rebind
```

## When it fails

- `BindingRequired` on a code tool → no binding in this scope; run register+bind.
- `BindingConflict` on bind → repo already bound to a different ref; deliberate
  rebind or reset flow (owner gate) — never silently rebind.
- `CodeAuthorizationRequired` → the store's host policy denies the op; surface
  to the owner (this is the deny-by-default guard working).
- Rebuild says "skipped N oversized files" → informational; vendored/generated
  blobs are excluded by design.
- Read tools fine but `search` misses new symbols → the lexical index needs the
  rebuild (step 3), not just refresh.

## Anti-patterns

- Indexing a feature branch or "whatever is checked out" — the binding is the
  product contract; feature branches are never the index source.
- Running `reset-apply` without a shown preview and explicit owner go-ahead.
- Editing the preview journal to "make it apply".
- Distilling with ad-hoc vocabulary instead of the installed ontology pack.
- Rebuilding so often you churn generations without merging — the index tracks
  the canonical branch, not your working pace.
