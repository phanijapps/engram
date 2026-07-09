---
name: codegraph-first
description: Understand a codebase quickly — index it, then get the structural overview (central symbols, communities, bridge points, entry points). Use when a user says "understand this codebase", "what does this repo do", "give me an overview", or on first encounter with a new repository.
---

# Codegraph: First Look

Get a structural overview of a codebase in seconds — no file reading needed.

## When to use

- "Understand this codebase"
- "What does this repo do?"
- "Give me an overview of this project"
- First encounter with a new repository

## Workflow

1. **Index the repo.** Call `scan_repo` with the absolute path.
   ```
   scan_repo({ "path": "/absolute/path/to/repo" })
   ```
   Returns: file count, entity count, relationship count.

2. **Get headline stats.** Call `repository_stats`.
   Returns: node count, edge count (the scale of the call graph).

3. **Find the most important symbols.** Call `central_symbols` with limit 20.
   Returns: PageRank-ranked symbols — the functions/classes everything else depends on.

4. **Find architectural modules.** Call `call_communities` with maxPasses 20.
   Returns: community label per symbol — clusters of tightly-coupled code.

5. **Find chokepoints.** Call `bridge_symbols` with limit 5.
   Returns: highest-betweenness symbols — touching these has outsized blast radius.

6. **Find entry points.** Pass the source of `main.rs` or `index.ts` to `find_entry_points`.
   Returns: function names where execution begins.

## How to synthesize

After gathering the data, present a concise narrative:

> This codebase has **N symbols** across **M modules** (communities). The core
> abstractions are **X, Y, Z** (top PageRank). Entry points: **main, handler**.
> Be careful with **B** — it's a bridge symbol (high betweenness).

**Do NOT** read individual files unless the user asks about a specific symbol.
The graph tells you the structure; files are for detail.
