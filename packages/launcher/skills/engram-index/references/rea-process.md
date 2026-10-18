# REA process for code indexing

The evidence-first discipline (adapted from the REA — Reverse Engineer
Anything — method) applied to understanding and memorizing a codebase.

## The process, stage by stage

### 1. Target scoping
Define what is being understood before touching anything:
- What kind of system is this? (service, library, app, agent tool)
- What will "understood" mean here? (can answer where/how questions,
  can plan changes safely, can recall invariants)
- What is already known? (check staleness — if a fresh index exists,
  only delta-memorize)

**Stop condition:** scope stated in one sentence before any tool call.

### 2. Surface mapping
Map the terrain without reading files:
- `scan_repo` — the structural pass (symbols, edges, communities)
- `architecture {limit: 20}` — central symbols, bridges, stats
- `graph_health` — is the graph trustworthy?

Surface mapping answers: how big, how coupled, where does complexity
live? It does NOT answer how anything works — that's tracing.

### 3. Feature tracing
Follow one behavior from trigger to outcome through the graph:
- Start from the entry point (top central symbol, or a named feature)
- `symbol_context` each hop: callers, callees, community
- `change_impact` on bridges to understand blast radius
- `fetch_content` the load-bearing files to verify the graph's story

Record every hop: file, symbol, what the edge means. A trace that
skips a hop is a guess, not a trace.

### 4. Evidence collection
Every claim gets its evidence AT COLLECTION TIME, not later:

| Label | Meaning | Evidence standard |
|---|---|---|
| `[proven]` | Verified against source or graph | tool + query + what it showed |
| `[likely]` | Strong signal, unverified | naming, structure, docs memory |
| `[guessed]` | Hypothesis only | must say so explicitly |

Rules:
- Never claim how something works without graph/source evidence.
- Separate facts from interpretation — the label IS the separation.
- Start with the simplest trace that proves the behavior.
- Pseudocode-level summaries of files you haven't fetched are `[likely]`
  at best.

### 5. Reconstruction (memorization)
Turn findings into durable memory — one concern per memory, written so a
future agent in a fresh session can act on it:
- Stack + layout invariants
- Sharp edges (bridges, god modules, temporal traps)
- Core abstractions (top PageRank, one line each)
- Build/test/verify procedures (exact commands, only ones seen working)

An unmemorized finding is a finding you'll pay for twice.

### 6. Verification (write → read-back)
- Recall 3 probe queries a future agent would actually ask.
- A probe that fails = a memory written badly = rewrite it.
- **An index that cannot be recalled is not an index.**

### 7. Final report
- N symbols, M modules, core abstractions, bridges to respect
- What was memorized + confidence split (proven/likely/guessed counts)
- Unknowns worth investigating — honesty about holes

## The final check (run before claiming done)

- What am I missing in the overview?
- Is each claim proven or just plausible?
- Did I mistake graph structure for verified behavior?
- Would these memories survive the next refactor?
- What evidence would change my mind?
