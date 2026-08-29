# Research: Agent Memory Needs & MCP Tool Consolidation

## What memory layers do AI agents actually need?

### The research consensus

**MemGPT / Letta** (Packer et al., 2024; arXiv:2310.08560)
- Pioneered OS-style tiered memory for LLM agents
- Core insight: agents need ~5-8 operations, not dozens — read, write,
  update, delete, and memory management (the OS analogy)
- Three tiers: core (working), archival (persistent), recall (retrieval)
- **Implication**: the tool surface should mirror OS syscalls — few, rich,
  composable

**Generative Agents** (Park et al., 2023; Stanford)
- Memory stream + retrieval scoring: `recency × importance × relevance`
- Reflection: agents periodically synthesize higher-level observations
- **Implication**: ONE retrieval interface with multi-signal scoring, not
  separate tools per signal. Reflection is a background process, not a
  tool the agent calls manually.

**Voyager** (Wang et al., 2023)
- Skill library built incrementally — procedural memory as first-class
- **Implication**: procedures are stored, verified, and incremented —
  engram already has this (`procedure_put/increment`)

**Cognitive architectures (CoALA alignment)**
- Working / episodic / semantic / procedural memory types
- Engram's `MemoryRole` derives these from kind + policy + scope — correct

### What engram already has (and research validates)

| Research concept | Engram implementation | Status |
|---|---|---|
| Episodic memory | `kind: episode` + lifecycle events | ✓ |
| Semantic memory | Beliefs + knowledge chunks | ✓ |
| Procedural memory | Procedures (runbooks with success/failure counters) | ✓ |
| Source-grounded knowledge | Source → Document → Chunk → Entity chain | ✓ |
| Reflection | Consolidation (reflection + decay pipeline) | ✓ |
| Multi-signal retrieval | 6 modes fused via RRF with FusionTrace | ✓ |
| Bi-temporal validity | `valid_from/until` + `as_of` retrieval | ✓ (unique) |
| Governance | Policy + provenance + scope on every record | ✓ (unique) |

**Verdict: engram's DOMAIN MODEL is research-aligned and complete. The
problem is the SURFACE — 44 tools is an anti-pattern.**

---

## The tool overload problem

### Token cost (the invisible tax)

Each MCP tool definition (name + description + JSON schema) costs
~200-500 tokens in the agent's context window BEFORE any work starts.

| Surface | Tools | Est. tokens consumed | % of 128K window | % of 32K window |
|---|---|---|---|---|
| Current (44 tools) | 44 | ~13,200 | 10.3% | 41.3% |
| Proposed (10 tools) | 10 | ~4,000 | 3.1% | 12.5% |
| **Savings** | **−77%** | **~9,200 freed** | **7.2% freed** | **28.8% freed** |

### Accuracy degradation

Research on tool selection consistently shows agent accuracy degrades with
tool count. The agent must CHOOSE which tool to call; with 44 options:
- Choice paralysis: more wrong tool selections
- Schema confusion: similar tools with subtle differences
- Context waste: 41% of a 32K window consumed by definitions alone

### What agents ACTUALLY call (from usage patterns)

Heavy use: `scan_repo`, `search`, `recall`, `write_memory`, `symbol_context`
Moderate: `get_context`, `change_impact`, `store_knowledge`, `forget`
Rare/Never called directly by agents: the other ~35 tools

---

## The consolidation: 44 → 10 tools

### Design principle: **fewer verbs, richer nouns**

Agents don't need 44 verbs. They need ~10 rich verbs with discriminators.
Everything else is implementation detail the agent shouldn't see.

### CORE (6 tools) — the 80/20, always loaded

| # | Tool | Consolidates | Discriminator |
|---|---|---|---|
| 1 | `remember` | write_memory, put_entity, put_relationship, belief_put, procedure_put, store_knowledge | `kind` |
| 2 | `recall` | recall, search, get_context, predict_context | `mode` |
| 3 | `code` | symbol_context, change_impact, code_health, architecture, whats_changed, explore, file_dependencies | `op` |
| 4 | `scan` | scan_repo, scan_protocols, scan_dependencies, scan_ownership, index_docs | `target` |
| 5 | `graph` | graph_neighbors, graph_subgraph, resolve_entity | `op` |
| 6 | `forget` | forget, belief_retract | `kind` |

### SPECIALIST (4 tools) — loaded via `--tools full`

| # | Tool | Consolidates | Discriminator |
|---|---|---|---|
| 7 | `maintain` | consolidate, reindex, graph_health, list_maintenance_candidates, build_maintenance_plan, apply_maintenance_plan | `op` |
| 8 | `beliefs` | belief_get, belief_stale_list, contradiction_list | `op` |
| 9 | `procedures` | procedure_list, procedure_increment | `op` |
| 10 | `hierarchy` | hierarchy_build, hierarchy_path | `op` |

### Removed from the surface

- `ping` — the server responded; that IS the ping
- `capability_report` — embedded as a response header on any tool call
- `ontology_read` / `taxonomy_read` — config detail agents don't query

### Profile changes

```
--tools core    → 6 tools (DEFAULT — the daily driver surface)
--tools full    → 10 tools (specialist work)
--tools all     → 44 tools (backward compat, deprecated)
```

### Token impact

| | Before | After |
|---|---|---|
| Tool definitions | ~13,200 tokens | ~4,000 tokens |
| Freed for context | — | ~9,200 tokens (70%) |
| On 32K window | 41% consumed by tools | 12.5% consumed |

---

## Implementation approach

Phase 1: Add consolidated tools as ALIASES that dispatch to existing
handlers (zero behavior change, backward compat preserved).

Phase 2: Default `--tools core` for new launches; `all` still works.

Phase 3: The 44 granular tools become `--tools all` (deprecated but
functional). The N-API binding and TS server add the same consolidated
surface (surface parity maintained).
