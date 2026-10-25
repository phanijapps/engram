# Memorization catalog — what engram-index writes, exactly

The reference for phase 2 (memorize) of engram-index. One concern per
memory; every claim labeled; every procedure seen working.

## Memory formats (write_memory)

### Stack + layout
```
write_memory({ "content": "[proven] Rust workspace, 30+ crates; core/
  adapters/backends split enforced by scripts/ci/check-engine-neutrality.sh
  — evidence: architecture tool showed core crates; CI script read via
  fetch_content" })
```

### Invariants (the rules this repo lives by)
```
write_memory({ "content": "[proven] engram-domain must not depend on SQL,
  vector stores, or async runtimes — evidence: check-engine-neutrality.sh
  gates CI; violation names engine types in core crates" })
```
Label `[likely]` when inferred from structure alone ("naming suggests
layering" is not proof); upgrade to `[proven]` only when a check, test,
or doc enforces it.

### Sharp edges (what bites)
```
write_memory({ "content": "[proven] bridge symbol: resolve_symbol in
  core/code — 14 dependents across 5 crates; change_impact shows ripple
  into scanner, mcp tools, and tests. Touch last, test wide." })
```
Sources of sharp edges: `architecture` bridge list, `change_impact`
output on symbols you expect to touch, temporal traps from
`whats_changed`.

### Core abstractions (the "understand this = understand the system" set)
```
write_memory({ "content": "[proven] MemoryService trait is the central
  abstraction (top PageRank) — every write path flows through it;
  adapters implement, orchestration composes. evidence: architecture
  tool, symbol_context depth 2" })
```

## Procedure formats (procedure_put)

Only procedures whose steps were SEEN working (CI configs, READMEs,
this session). Untested steps must say so in `trigger`.

```
procedure_put({ "name": "verify",
  "trigger": "before handoff",
  "steps": ["cargo fmt --all", "cargo check --workspace",
            "pnpm run typecheck", "pnpm run test",
            "scripts/ci/check-contracts.sh"] })

procedure_put({ "name": "scan-repo",
  "trigger": "index refresh",
  "steps": ["scan_repo {path: <repo>}", "architecture {limit: 20}",
            "graph_health {}"] })
```

## Concept extraction (NOT here — engram-distill)

The graph already knows every symbol; concepts the graph can't infer
(business rules, domain terms, design principles) are extracted by an
**engram-distill** pass over the repo's docs, keyed by source. Do not
call `put_entity`/`put_relationship` from this skill — a concept not
source-keyed and bridged by distill is an orphan waiting to happen.

## What NOT to memorize

- File counts, line counts, dependency version trivia
- Boilerplate names, test fixture names
- Anything `architecture`/`symbol_context` answers on demand
- Session play-by-play (that's engram-mem's job, and only the durable parts)
