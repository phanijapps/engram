# Position B — modularize knowledge for progressive loading

## Decision

Split the two large descriptive registers into topic files under `knowledge/`,
with `knowledge/INDEX.md` as the required manifest and routing entry point.
This is not a claim that a 304-line document is unreadable. It is a claim that
always loading it is the wrong default for agents whose task concerns one seam.

## Measured baseline and context cost

I ran `wc -l -w -c` on the current files:

| File | Lines | Words | Bytes | Brief token estimate |
|---|---:|---:|---:|---:|
| `architecture/knowledge.md` | 304 | 2,809 | 24,112 | ~6,000 |
| `product/knowledge.md` | 297 | 2,177 | 17,580 | ~4,400 |

`wc` does not measure model tokens, so the final column remains the brief's
approximation rather than a false precision claim. At a 100k-token window,
6,000 tokens are 6% of the window before an architecture task has read its
brief, spec, plan, tasks, source, contracts, and validation gates; product
knowledge adds another 4.4% when it is also relevant. On 10k windows the
architecture file alone is about 60%; on 5k windows it does not fit. Those
small-window tail cases make unconditional loading particularly costly.

The relevant unit is therefore the *job context*, not this file in isolation.
Factory analyzers and planners must hold the job artifacts and repository
material at the same time. Spending a fixed 6k tokens on launcher, demo, and
layout detail before a retrieval-only analysis reduces room for the evidence
that distinguishes a correct plan from a generic one.

## Why selective loading is the better default

A retrieval spec needs the `RetrievalIndex → RetrievalFusion → ContextComposer`
seam, RRF, `ContextPayload`, policy filtering, failure behavior, and the
read-path constraint. It does not need the installation or launcher story, the
full repository tree, or every product capability status. A code-index task
likewise needs binding authority, generation, and transport parity, not the
belief-synthesis narrative. Topic files let the router load the required
invariants plus the one relevant topic, while the manifest makes the selection
explicit and reviewable.

This improves more than token count: narrower context reduces accidental
coupling. An agent is less likely to turn an unrelated status paragraph into a
requirement or to overlook the one paragraph that defines its seam amid a long
absorbed overview. The proposal preserves a coherent full read: loading every
manifest entry reconstructs the current register.

## Concrete proposed layout

Change each pack's `00-INDEX.md` knowledge pointer to `knowledge/INDEX.md`.
The old `knowledge.md` is removed after inbound links are updated, or is only a
short compatibility redirect during migration; it must not become a duplicate
second register. The following are initial payload budgets for the manifest,
not measured current-file sizes. The migration acceptance check records actual
`wc -l -w -c` results and the same token-estimation method for each fragment.

### `docs/constitution/architecture/knowledge/`

| File | Initial budget | Purpose |
|---|---:|---|
| `INDEX.md` | ~200 tokens | Manifest, required prerequisites, and task-to-topic routing. |
| `orientation.md` | ~450 | Product-independent map and write/store-to-recall pipeline. |
| `retrieval-composition.md` | ~800 | Retrieval lanes, RRF, composition, degradation, and read-path boundary. |
| `core-layers-and-engine-cells.md` | ~1,000 | Core ownership, ports, adapters, and engine-neutrality constraints. |
| `provider-and-transport-surfaces.md` | ~650 | `EngramProvider`, capability reporting, N-API/TS, and MCP parity. |
| `repository-map-and-entrypoints.md` | ~1,000 | Layout, crate/package catalogue, conventions, and where to start. |
| `reference-foundations.md` | ~1,100 | Normative constraints, strategy, component stereotypes, and dependency direction. |
| `crosscutting-standards.md` | ~850 | Errors, security, configuration, observability, and testing standards. |

The dedicated retrieval fragment gathers the currently dispersed retrieval
statements rather than copying them. Its manifest prerequisites are
`orientation.md` and `reference-foundations.md`; this makes the seam complete
without loading repository onboarding or transport detail.

### `docs/constitution/product/knowledge/`

| File | Initial budget | Purpose |
|---|---:|---|
| `INDEX.md` | ~150 tokens | Manifest, scope statement, and capability-task routing. |
| `orientation.md` | ~450 | Mission, audience, four substrates, and explicit non-goals. |
| `memory-and-source-grounded-knowledge.md` | ~900 | Memory lifecycle, ingestion/graph, and taxonomy/ontology. |
| `derived-context-and-governance.md` | ~1,150 | Belief, hierarchy, retrieval, consolidation, policy, provenance, and scope. |
| `code-and-delivery-surfaces.md` | ~1,100 | Bound code, codegraph, storage, SDK, N-API, MCP, evaluation, and demo. |
| `maturity-and-direction.md` | ~800 | Capability table, spec-lag note, roadmap, maintenance, and links. |

A retrieval task would normally load the product manifest, `orientation.md`,
and `derived-context-and-governance.md`; a MCP task loads the same manifest and
`code-and-delivery-surfaces.md`. The manifest declares these prerequisites so
selection is not left to guesswork.

## Preventing fragment incoherence

Splitting has real failure modes: link-chasing adds latency, a task can load an
incomplete view, and cross-topic edits can drift. Prevent them by making the
manifest a contract, not a directory listing:

1. Every entry states purpose, load-when signals, required companions,
   canonical topics it owns, outbound cross-links, and measured size.
2. Every fragment starts with its required companions and ends with only the
   related-topic links needed to act; shared assertions have one canonical home
   and are linked, never copied.
3. `INDEX.md` has named routes for retrieval, storage/engine work, product
   capability/status work, code-index work, and surface-parity work, plus a
   `full-read` route for broad changes.
4. A documentation gate verifies that every `knowledge/*.md` file is listed
   exactly once, every manifest target exists, all declared prerequisites and
   cross-links resolve, and recorded `wc` sizes match regenerated values.
5. Reviewers require a fragment's declared companions in the job context when
   a changed rule crosses their boundary; broad refactors use `full-read`.

This discipline costs some manifest maintenance and cannot guarantee that an
agent follows routing. It makes omissions detectable, whereas a monolith makes
irrelevant loading mandatory and missing a buried paragraph easy.

## Concession and boundary

Monolithic knowledge wins when the register is small enough that it is routinely
loaded in full with the task's other evidence, its concepts change together,
and no meaningful task can select a smaller coherent subset. It also wins if
routing evidence shows agents regularly need most fragments and fragment gates
or maintenance outweigh saved context. In that condition, an index would add
indirection without selective-loading benefit. The present two files already
contain separable capability, seam, surface, layout, and status topics, and the
owner's stated goal is progressive loading; they meet the case for modularizing
now, provided the manifest and completeness checks ship with the split.
