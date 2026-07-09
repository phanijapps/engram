---
name: codegraph-onboarding
description: Onboard a new developer to a codebase — trace execution flow from entry points, map the architecture into communities, identify key abstractions and critical paths. Use when a user says "I'm new to this codebase", "how does this work", "explain the architecture", "where do I start".
---

# Codegraph: Developer Onboarding

Trace the execution flow and map the architecture for a new developer.

## When to use

- "I'm new to this codebase"
- "How does this project work?"
- "Explain the architecture"
- "Where do I start reading code?"

## Prerequisites

The repo must be indexed first. If `repository_stats` returns 0 nodes, call
`scan_repo` before continuing.

## Workflow

1. **Get the scale.** Call `repository_stats` to understand how large the codebase is.

2. **Map the modules.** Call `call_communities` (maxPasses 20).
   Returns: community label per symbol. Group symbols by label to see the
   architectural modules. Count the labels to see how many modules exist.

3. **Find where execution starts.** Read the main entry file (e.g., `main.rs`,
   `index.ts`, `app.py`) and pass its source to `find_entry_points`:
   ```
   find_entry_points({ "source": "fn main() { ... }" })
   ```
   Returns: function names where execution begins.

4. **Trace the execution flow.** For each entry point, call `process_flow`:
   ```
   process_flow({ "entryPoint": "main", "maxDepth": 10 })
   ```
   Returns: the ordered list of symbols reachable from the entry point.
   This is the "happy path" — the call chain a new developer should follow.

5. **Identify the key abstractions.** Call `central_symbols` (limit 10).
   These are the functions/classes that everything else depends on — the
   "domain vocabulary" a new developer needs to learn first.

6. **Find the critical paths.** Call `bridge_symbols` (limit 5).
   These are chokepoints — code that connects modules. Understanding them
   early prevents architectural surprises later.

7. **Get a 360° view of a key symbol.** Pick the highest-ranked central symbol
   and call `symbol_context`:
   ```
   symbol_context({ "symbol": "DatabasePool", "depth": 3 })
   ```
   Returns: callers (who depends on it), callees (what it depends on), and
   its community. This is the "if you understand this, you understand the system"
   symbol.

## How to synthesize

Present a narrative onboarding guide:

> **Architecture Overview**
>
> This codebase has **N symbols** organized into **M modules**.
>
> **Execution flow:** `main → bootstrap → loadConfig → startServer → handleRequest`
>
> **Core abstractions** (learn these first):
> 1. `DatabasePool` — connection management (called by 47 symbols)
> 2. `Router` — request dispatch (called by 23 symbols)
> 3. `Config` — configuration loading (called by 31 symbols)
>
> **Critical paths:** `Router` is a bridge symbol — it connects the HTTP layer
> to the business logic. Changes here affect both communities.
>
> **Where to start reading:** Follow the execution flow above, then read
> `DatabasePool` (the most-depended-on symbol).

## Tips

- The first `process_flow` call is the "map" — share it with the developer.
- Central symbols are the vocabulary — name them explicitly.
- Bridge symbols are the risk areas — flag them for the developer.
- Communities are the module boundaries — use them to organize the narrative.
