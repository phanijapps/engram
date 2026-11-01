# How engram is built (summary)

<!-- prerequisite: none -->

## Knowledge — how engram is built

**The map:** ~35 Rust crates + 6 TS packages. `core/*` = storage-neutral
behavior (domain, runtime, memory, knowledge, belief, hierarchy,
consolidation, reflection, retrieval, procedures, code, integration...).
`adapters/*` = engine cells (sqlite, pgvector, retrieval/*, embedding/http).
`backends/*` = recipes composing cells into an EngramProvider (ADR-0022:
swap the backend by config, not rewrite). `codegraph/*` = on-top queries.
`bindings/node` = N-API. `packages/*` = TS workspace.

**The seams that matter:**
- Recall composes through the retrieval composer (RRF + lanes); no lane
  bypasses the service layer; failed lanes degrade via `source_failures`
- The capability×engine grid: engines are interchangeable cells behind ports;
  engram-domain and friends never name an engine type (gate-enforced)
- Surface parity: every capability reachable via Rust facade AND N-API AND
  MCP, reflected in CapabilityReport (ADR-0022)
- Domain SSOT: `docs/architecture/domain-data-model.md` until Rust types
  become the generated-contract source
The map and the golden path are absorbed below.

