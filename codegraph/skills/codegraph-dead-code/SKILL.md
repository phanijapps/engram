---
name: codegraph-dead-code
description: Find dead code and refactoring candidates — symbols with zero callers, ranked by complexity. Use when a user says "find dead code", "unused functions", "refactoring candidates", "what can I remove", "cleanup targets".
---

# Codegraph: Dead Code Detection

Find symbols that are defined but never called — refactoring candidates.

## When to use

- "Find dead code in this repo"
- "What functions are unused?"
- "Refactoring candidates"
- "What can I safely remove?"

## Prerequisites

The repo must be indexed first. If `repository_stats` returns 0 nodes, call
`scan_repo` before continuing.

## Workflow

1. **Get the dead-code list.** Call `dead_code`:
   ```
   dead_code({})
   ```
   Returns: entity IDs for symbols with zero incoming `calls` edges.

2. **Cross-reference with central symbols.** Call `central_symbols` (limit 50).
   If a "dead" symbol also appears in the central-symbols list, it's likely a
   false positive — it's important but called via dynamic dispatch, traits,
   or framework conventions (not static `calls` edges).

3. **Rank by complexity.** For each dead-code symbol you want to investigate,
   pass its source to `cyclomatic_complexity`:
   ```
   cyclomatic_complexity({ "source": "fn complex_dead(x: i32) -> i32 { ... }" })
   ```
   High-complexity dead code is the best refactoring target — it's expensive
   to maintain AND nobody calls it.

4. **Check for entry points.** Pass the file's source to `find_entry_points`.
   A dead-code symbol that IS an entry point (main, handler, __main__) is
   NOT dead — it's the starting point of execution.

## How to synthesize

Present the candidates ranked by confidence:

> **Found N potentially dead symbols.**
>
> **High confidence (safe to remove):**
> - `unusedHelper` (complexity 2, zero callers, not central, not an entry point)
>
> **Low confidence (verify first):**
> - `handleRequest` (zero callers BUT is an entry point — likely framework-driven)
> - `processData` (zero callers BUT appears in central symbols — may be dynamic)

## Caveats

- Dynamic dispatch (trait objects, callbacks, event handlers) won't appear as
  `calls` edges. A symbol called only via dynamic dispatch will look "dead."
- Framework entry points (HTTP handlers, CLI commands, event listeners) often
  have zero static callers. Cross-reference with `find_entry_points`.
- Cross-language calls (FFI, subprocess) won't appear in the graph.
