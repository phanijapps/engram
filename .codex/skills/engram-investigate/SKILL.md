---
name: engram-investigate
description: Use when investigating a code, architecture, or design question that requires finding specific implementation evidence across indexed repositories. Structures a multi-step evidence-gathering workflow: discover scope, retrieve compact landscape, extract anchors, expand graph neighborhoods, fetch targeted evidence, and assess sufficiency — all via engram's MCP tools, without filesystem search.
---

# engram-investigate

Investigate a question using engram's retrieval tools. Designed to find
decisive implementation evidence (function definitions, call chains,
credential flows, request construction) **without searching the filesystem
directly**. Optimized for accuracy + minimal context usage.

## When to use

- "How does X implement Y?"
- "Where is Z defined and who calls it?"
- "Trace the flow from A to B"
- "Why does X happen after Y?"
- Any question where `grep` / filesystem search would be your fallback

## Prerequisites

Engram's MCP tools must be available (`recall`, `search`, `get_context`,
`symbol_context`, `change_impact`). The target repositories must be
indexed via `scan_repo`.

## Workflow

### Step 1 — Discover scope (broad recall)

Call `recall` with the question as the query:

```json
{"query": "the question in natural language", "limit": 15}
```

`recall` fuses vector + lexical + graph + temporal + beliefs lanes over ALL
entity kinds (code, docs, concepts, memories, beliefs). It finds relevant
evidence regardless of whether it's code, documentation, decisions, or
prior evaluations. Check the results' provenance — which repository/corpus
dominates?

Use `search` only for exact-identifier follow-ups (when you know a specific
function/class name like `anthropicOAuth` and want its symbol definition +
file path).

### Step 2 — Compact landscape (discovery mode)

Call `get_context` in **discovery mode** for a cheap overview — entity
names + file paths + scores only, no content bodies (~80 chars per item
vs ~2000 in evidence mode):

```json
{
  "focus": "the question in natural language",
  "repository": "org/repo from step 1",
  "mode": "discovery",
  "limit": 20
}
```

Read the landscape. Look for **distinctive identifiers** — function names,
class names, constants, file paths that are unique to the implementation
you need. These become your **anchors** for targeted follow-up.

### Step 3 — Expand graph neighborhoods

For each distinctive anchor, call `symbol_context` to get its call graph
neighborhood (callers + callees + community):

```json
{"symbol": "loginAnthropic", "depth": 2}
```

If you need blast radius (who is affected by changing this symbol):

```json
{"target": "loginAnthropic", "depth": 2}
```

This reveals the **structural relationships**: who calls it, what it
calls, the chain from entry point to implementation.

### Step 4 — Fetch targeted evidence (evidence mode)

For the key anchors discovered in steps 2–3, call `get_context` in
**evidence mode** to get smart excerpts (bounded content, not whole
files):

```json
{
  "focus": "loginAnthropic resolveStoredOAuth",
  "repository": "org/repo",
  "mode": "evidence",
  "limit": 10
}
```

This returns the specific code lines you need — function signatures,
key statements, relevant excerpts — capped at ~2000 chars per item.

### Step 5 — Assess sufficiency

Ask yourself:

- Did I find the **decisive identifiers**? (function names, constants,
  values like `sk-ant-oat`, `Authorization: Bearer`, `oauth-token`)
- Did I find the **primary source files**? (the paths where the
  implementation lives — shown in each result)
- Did I trace the **full flow**? (entry → processing → storage → output)

If any answer is **no**, do a targeted follow-up:

```json
{"query": "the missing identifier or concept", "repository": "org/repo"}
```

Then repeat steps 3–4 for the new anchor. Limit yourself to 2–3
follow-up rounds — if engram can't find it after that, the evidence
may not be indexed.

### Step 6 — Synthesize

Combine the evidence into an answer. Cite each claim with:

- The **entity name** (e.g., `loginAnthropic`)
- The **file path** (e.g., `packages/ai/src/auth/oauth/anthropic.ts`)
- The **score** (e.g., `[1.00]` for exact match, `[0.04]` for semantic)

If evidence is incomplete, say so — **"Found X but could not locate Y"**
is more useful than a confident guess.

## Tool reference

| Tool | Purpose | Context cost |
| --- | --- | --- |
| `recall` | Broad evidence discovery across ALL kinds (code, docs, memories, beliefs) | Low (~content per item) |
| `search` | Find code symbols + chunks by keyword (with exact-match boost) | Low (~50 chars/result, chunks +500) |
| `get_context` (`mode: "discovery"`) | Compact landscape: names + paths + scores | Very low (~80 chars/item) |
| `get_context` (`mode: "evidence"`) | Targeted excerpts with content | Medium (~2000 chars/item) |
| `symbol_context` | Call graph neighborhood (callers + callees) | Low |
| `change_impact` | Blast radius (who is affected by a change) | Low |

## Key behaviors (built into the tools)

- **Exact-match injection:** Searching for `anthropicOAuth` finds it even
  if vector similarity is low. Exact identifier matches are boosted to
  score 1.0.
- **Repository filtering:** Pass `repository: "org/repo"` to eliminate
  cross-repo contamination. Results only from the target repo.
- **File paths:** Every result includes the source file path — you know
  WHERE the code lives without reading the file.
- **Per-file cap:** Max 2 results per source file (diversity over depth).
  Use `symbol_context` for depth from a single file.
- **Score threshold:** Items below 0.01 are dropped as noise. Pass
  `min_score: 0` for maximum breadth.
- **Lane budgets:** Code-shaped queries prioritize code entities (60%)
  over memory/docs. The query shape is detected automatically.
- **NL → symbol anchor:** `get_context` resolves natural-language focus
  to the top-scoring entity → populates [Code]/[Graph] sections
  automatically.

## Anti-patterns

- **Don't** call `get_context` with `mode: "evidence"` + `limit: 50` as
  your first step — that's 50k chars of untargeted context. Use discovery
  mode first.
- **Don't** skip the `repository` filter — without it, unrelated repos
  contaminate results.
- **Don't** do more than 3 follow-up rounds without finding the evidence —
  if engram can't find it, it may not be indexed. Use `scan_repo` to
  index the repo first.
- **Don't** read source files directly unless engram's results are
  insufficient after the full workflow. The tools are designed to replace
  filesystem search.
