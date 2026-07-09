---
name: codegraph-impact
description: Analyze the blast radius of changing a symbol — who calls it, who calls them, how far the impact spreads. Use when a user says "what breaks if I change X", "impact of changing", "blast radius of", "is it safe to refactor X".
---

# Codegraph: Impact Analysis

Determine what breaks if you change a specific function, class, or method.

## When to use

- "What breaks if I change `parseRequest`?"
- "Is it safe to refactor the database layer?"
- "Blast radius of changing the auth handler"
- "Who depends on `UserService`?"

## Prerequisites

The repo must be indexed first. If `repository_stats` returns 0 nodes, call
`scan_repo` before continuing.

## Workflow

1. **Confirm the symbol exists.** Call `central_symbols` (limit 50) and scan the
   results for the target name. If not found, the symbol may be named differently
   — try a substring or ask the user for the exact name.

2. **Get the blast radius.** Call `blast_radius` with the symbol name and depth 5:
   ```
   blast_radius({ "target": "parseRequest", "depth": 5 })
   ```
   Returns: every symbol that transitively calls the target.

3. **Check for bridge connections.** Call `bridge_symbols` (limit 10).
   Cross-reference: if the target or any of its callers appears in the bridge
   list, the change is high-risk (it's a chokepoint).

4. **Trace a critical path.** Pick one of the callers and call `dependency_path`:
   ```
   dependency_path({ "from": "mainHandler", "to": "parseRequest" })
   ```
   Returns: the exact call chain from a caller to the target.

## How to synthesize

Present the risk assessment:

> **Blast radius: HIGH / MODERATE / LOW**
>
> Changing `parseRequest` affects **N callers** across **M communities**.
> Critical path: `mainHandler → router → parseRequest`.
> **Warning:** `router` is a bridge symbol — changes here ripple widely.

## Risk levels

- **LOW:** 0-2 callers, none are bridge symbols.
- **MODERATE:** 3-10 callers, or the target is in a single community.
- **HIGH:** >10 callers, or any caller (or the target) is a bridge symbol.
