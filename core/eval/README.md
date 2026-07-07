# engram-eval

`engram-eval` runs portable evaluation fixtures against any implementation of
the Engram `MemoryService` contract.

It also exposes accepted v1 example loaders and a small contract runner for
write/retrieval smoke behavior so store crates can reuse the same fixture path
without copying JSON parsing or service orchestration.

Report summary helpers convert executed fixture reports into serializable
aggregate and case-level output for CI or future CLIs.

Architecture coverage helpers map executed cases onto the research parity bars
they prove: accepted recall, forbidden recall, leakage, policy filtering,
ranking, hierarchy granularity, taxonomy drift, belief lifecycle, contradiction
review, consolidation gates, and adapter readiness. They summarize coverage and
failure status without making this crate execute every subsystem directly.

Current scope:

- load accepted portable contract examples
- run accepted write/retrieval examples through `MemoryService`
- seed memories through normal write behavior
- run retrieval cases
- summarize executed fixture reports
- summarize architecture capability coverage over executed cases
- report missing required targets
- report forbidden target leaks
- report missing explanations
- report score and max-result expectation failures

The crate does not own retrieval, storage, model providers, or adapter-specific
fixtures. It is the deterministic harness that future SQL, vector, native, and
TypeScript paths should share.
