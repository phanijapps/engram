# Accepted behavior — retrieval evaluation fixtures

> Behavior authority for accepted retrieval fixtures, migrated from the
> `accepted-retrieval-fixtures` feature spec. The fixtures + runner below are
> normative.

Four accepted v1 retrieval fixtures cover the core retrieval contract — positive
recall, forbidden recall, budget omission, and no-result. All live under
`contracts/v1/examples/` and all deserialize as `EvaluationFixture`:

- **`evaluation-fixture.positive-recall.json`** (`eval-retrieval-positive-001`) —
  one fact; the query must `mustInclude` `memory-001` with an explanation.
- **`evaluation-fixture.forbidden-recall.json`** (`eval-retrieval-forbidden-001`) —
  two identical-body facts in different tenants; a query scoped to one tenant must
  `mustInclude` `memory-001` and `mustExclude` `memory-002` (cross-scope isolation).
- **`evaluation-fixture.budget-omission.json`** (`eval-retrieval-budget-001`) —
  `request.budget.maxItems = 1` must `mustInclude` `memory-001` and `mustExclude`
  `memory-002` (budget omission ≠ no-result).
- **`evaluation-fixture.no-result.json`** (`eval-retrieval-no-result-001`) — an
  unmatched query must `mustExclude` `memory-001` (empty required result set).

**`MemoryFixtureRunner`** (`core/eval/src/lib.rs`) seeds each setup memory via
`service.write_memory`, collects the returned ids as insertion-order aliases
(`memory-NNN` resolves to the (NNN-1)-th setup id), then calls `service.retrieve`
per case. Failure rules: `must_include` (missing required target), `must_exclude`
(forbidden target returned), `max_results` (too many results), `min_score` (below
minimum), `requires_explanation` (missing explanation for a `must_include` target).

**Invariants.** Fixtures are portable (no adapter-specific DB / in-memory state);
forbidden recall is never a pass; budget omission is never hidden behind a
no-result fixture; retrieval is exercised through `MemoryService`, not adapter
internals.

**Proof.** `core/eval/tests/fixture_runner.rs` — all four fixtures pass plus a
forced forbidden-leak negative test, run against `SqlMemoryService::open_in_memory()`.
