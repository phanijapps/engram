# Platform sub-constitution

SUBORDINATE to the root `00-INDEX.md` — one constitution, one root; this pack
narrows it locally with ENGRAM-GROUNDED law. Reference grammar:
`governs` / `refines` / `enforced-by` / `see`.

## Charter

Where engram runs: storage, MCP server deployment, embedding providers,
CI environments, ontology pack distribution.

## Structure (one home per concern)

| Concern | Lives at |
|---|---|
| Knowledge | `knowledge.md` (STE descriptive register) |
| Rules | `rules/<ID>.md` — one rule per file (STE procedural register) |
| Deviations | below; global: root `DEVIATIONS.md` / `DECAY-WINDOWS.md` |

## Rule index

| ID | Rule (one line, STE) | File |
|---|---|---|
| PLT-001 | Disabled-vector mode MUST NOT initialize models, read credentials, or make HTTP calls | `rules/PLT-001.md` |
| PLT-002 | MCP server storage MUST resolve per-project under the declared storage root | `rules/PLT-002.md` |
| PLT-003 | Ontology pack installs MUST distinguish full structural vocabulary from the bounded launch slice | `rules/PLT-003.md` |

## Deviations & decay (domain)

- none open

## Notes

- Enforcement: PLT-001 by `core/integration/src/embedding_runtime.rs` mode
  checks; PLT-002 by `mcp/engram-mcp/src/config.rs` path resolution; PLT-003
  by `core/packs/src/slice.rs` structural slicing.
