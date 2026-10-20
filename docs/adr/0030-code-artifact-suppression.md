# ADR-0030: Code-artifact suppression at retrieval

- Status: Accepted
- Date: 2026-10-20
- Deciders: owner (phanijapps), planner session with red-forman + gpt-6.1-sol adversarial review

## Context

Code scans persist structural entities alongside real symbols and
knowledge-graph nodes. In the reference store, import-path stubs
(`vitest`, `@testing-library/react`, `./FileEditor`) land as `Module`
(6,295 of 6,340), file stubs as `File` (1,058) — **37.5% of the graph**.
Retrieval (graph-RAG lanes, search) surfaces them constantly while almost
never being what a query wants.

## Decision

1. **Suppression is result eligibility only.** Suppressed entities keep
   seeding PPR walks, carrying edges (concept↔code bridges such as
   `describes`/`realized_by`), and shaping community structure (Louvain
   labels + shared snapshots stay full-graph and flag-independent). They are
   simply not emitted as ranked results.
2. **The class is kind-based**: `Variable`, `Module`, `File`, `Repository`
   (`engram_retrieval::is_code_artifact`). No name heuristics — knowledge
   kinds can never be misclassified. The temporal-analytics kind set
   (`whats_changed`) stays deliberately separate.
3. **Opt-in**: `QueryFilter.includeCodeArtifacts` (absent/false =
   suppressed) on every surface — MCP `search`/`recall`/`explore`/
   `resolve_entity`, consolidated wrappers, oneshot CLI
   (`--include-code-artifacts`), facade DTO builder, N-API, TS HTTP MCP,
   TS client. Default is uniform everywhere.
4. **Filtering runs before truncation** in every lane; the lexical lane
   over-fetches (4×, floor 32) because Tantivy truncates before the
   resolver can classify.
5. **Exempt surfaces (explicit target / structural / analytics views)**:
   `symbol_context`, `graph_neighbors`, `graph_subgraph`, `fetch_content`,
   `whats_changed`, `code_health`, `dead_code`, `change_impact`,
   `architecture`, `file_dependencies`, raw repository reads, export.
6. **Forbidden seams**: `KnowledgeRelationshipSource` adapters (scope-keyed
   shared snapshot cache would bake the filter in for all lanes and all
   requests), `entity_lookup` (shared with rendering + anchor derivation),
   `list_entities` itself (analytics and file-dependency views need every
   entity).

## Consequences

- Default recall/search get materially cleaner on code-heavy scopes; users
  who need the artifacts pass the flag (empty-result packets carry an
  opt-in hint when suppressed matches exist).
- Indexing, stores, embeddings, and existing stores are untouched — no
  re-scan, no migration; the change is read-side only.
- The hand-maintained `engram-v1.schema.json` QueryFilter now carries
  `includeCodeArtifacts` (and the previously drifted `asOf`, ADR-0021);
  schema-conformance tests guard both directions.
