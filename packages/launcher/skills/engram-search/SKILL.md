---
name: engram-search
description: Agentic search over the code graph and memory to find or solve problems — "where is X", "how does Y work", "why did Z break", "find the code that...". An iterative hypothesis→retrieve→verify loop, not a one-shot lookup. Trigger on any codebase question where the answer should be evidenced.
---

# engram-search

Answer codebase questions with evidence. A loop: **hypothesis → retrieve
→ verify → refine**. Stop when the claim is `[proven]` — or honestly
unproven.

## Contract

1. Never claim file contents from memory alone — verify with `fetch_content`.
2. Every claim in the answer carries its evidence: the tool + query that proved it.
3. Two fruitless iterations → say what you looked for and stop.
4. Labels: `[proven]` (verified against source/graph), `[likely]` (strong signal, unverified), `[guessed]` (hypothesis only — say so).

## Workflow

### 1. State the hypothesis

Before any tool call, write one line: *"I expect X lives in Y because Z."*
This anchors the search and makes refinement measurable.

### 2. Retrieve from two angles

```
recall({ "query": "<the question's key terms>" })
search({ "query": "<the likely symbol/term name>" })
```

`recall` covers memories + knowledge + docs (the "why"); `search` covers
the code graph (the "where").

### 3. Drill the neighborhood

For the best candidate symbol:

```
symbol_context({ "symbol": "<name>", "depth": 2 })
```

For an area rather than a symbol:

```
explore({ "query": "<area terms>", "depth": 2 })
```

### 4. Verify against source

Before asserting what code does:

```
fetch_content({ "path": "<file path from the graph>" })
```

Graph and memory say where; the file says what. If they disagree, the
index is stale — say so (`[likely]` at best).

### 5. Refine or stop

- Found and verified → answer.
- Found but unverified → answer with `[likely]`, offer to fetch.
- Nothing → refine the query with terms the graph revealed (community
  names, related symbols), one more iteration. After two empty
  iterations, report the queries tried and stop.

## Output format

```
<answer, one paragraph>

Evidence:
- [proven] <claim> — symbol_context(X): called by A, B; fetch_content(file.rs)
- [likely] <claim> — recall hit "…" (indexed Oct 2026; may be stale)
- [guessed] <claim> — naming convention only
```

## When it fails

- Empty store (no index yet) → say "not indexed; run engram-index or
  scan_repo first" and stop. Do not fall back to grepping as if indexed.
- Stale-index signals (symbols that no longer exist per `fetch_content`) →
  flag the staleness in the answer; suggest `scan_repo`.
- Ambiguous symbol (many same-name entities) → disambiguate with
  `symbol_context` per candidate, or ask; don't average.

## Anti-patterns

- Claiming behavior from memory alone (no `fetch_content`).
- One query, one give-up.
- Answering from general language knowledge instead of this repo's graph.
- Skipping the hypothesis line.

## Final check

- Is every claim labeled?
- Did I read the actual file for the load-bearing claims?
- Would the evidence chain convince a skeptical reviewer?
