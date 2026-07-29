---
name: engram-distill
description: Distill knowledge from docs, transcripts, or code notes into the engram knowledge graph via the engram-mcp tools. Use after reading material worth remembering for a project — extract entities, relationships, and facts, classify them against the project's configured ontology, and persist them so later `recall` surfaces them. Extraction is the agent's own reasoning; the server never calls an LLM.
---

# engram-distill

Turn unstructured material (Markdown docs, transcripts, design notes) plus code
into a queryable, multi-layer knowledge graph by writing it through the
**engram-mcp** server. The server is a deterministic store — *you* (the agent)
do the extraction and classification; the server persists and retrieves.

## When to run

- You just read a doc / transcript / RFC / README that the project should remember.
- You are building up a project's knowledge graph (technical + business + domain).
- Before a task, to ground yourself: run `recall` (see `engram-recall` if present,
  else call `recall` directly).

## Prerequisites

The `engram` MCP server is configured (stdio JSON-RPC). Its launch flags fix the
**project** scope and the **ontology + taxonomy**:

```
engram-mcp --storage <db> --project <name> [--ontology <layers.json>] [--taxonomy <concepts.json>]
```

All writes land in one fused-per-project workspace; `recall` sees them together.

## Workflow

1. **Read the active ontology** — `ontology_read`. This returns the layers
   (technical / business / domain / …), their classes, and the allowed
   `within` / `across` predicates. Classify every entity you extract into one of
   these classes; use only these predicates for relationships. `taxonomy_read`
   gives the broader/narrower concept hierarchy.

2. **Index raw docs** (optional) — for material you want retrievable verbatim,
   call `index_docs` with the Markdown text. The server chunks it by structure
   (headers / code / prose) into the `docs` lane.

3. **Extract** (your reasoning) — from the material, produce:
   - **facts** — `{ "content": "…" }` free-text observations worth remembering.
   - **entities** — `{ "name", "kind" }` where `kind` is a real entity kind
     (`concept`, `api`, `function`, `organization`, …) AND maps to an ontology
     class. Unknown kinds are rejected.
   - **relationships** — `{ "subject", "predicate", "object" }` using only the
     ontology's predicates. Use **across** predicates (`realized_by`,
     `describes`, `governs`, …) to bridge layers — e.g. a business concept
     `realized_by` a technical artifact, or a domain concept `governs` a process.

4. **Write the batch** — `store_knowledge` with `{ facts, entities,
   relationships, idempotency_key }`. Supply a **stable caller-chosen
   `idempotency_key`** so re-sending the same batch dedups instead of doubling.
   The write is **best-effort, not ACID** — the result surfaces per-step status;
   malformed entries are skipped and reported.

5. **Verify** — `recall` with a query drawn from what you wrote; confirm it
   returns. Use `lanes` (`memory` / `knowledge` / `docs` / `beliefs`) to scope.

## Rules

- **You classify, the server stores.** Decide each entity's ontology class and
  each relationship's predicate from `ontology_read`; do not invent predicates.
- **One project, one graph.** Writes are scoped to the launch `--project`; do
  not try to cross projects from one skill run.
- **Ground concepts in artifacts.** Prefer `across` predicates that link a
  concept to the code/doc that realizes it — that cross-layer linkage is the
  point of the graph.
- **Idempotency.** Re-running the same distillation with the same
  `idempotency_key` should converge, not duplicate.

## Example call sequence

```
ontology_read                         → learn layers + predicates
index_docs      {content, path}       → persist the raw doc (docs lane)
store_knowledge {facts, entities,     → persist extracted KG (best-effort batch)
                 relationships,
                 idempotency_key}
recall          {query}               → verify it's retrievable
```
