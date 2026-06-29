# engram-eval

`engram-eval` runs portable evaluation fixtures against any implementation of
the Engram `MemoryService` contract.

Current scope:

- seed memories through normal write behavior
- run retrieval cases
- report missing required targets
- report forbidden target leaks
- report missing explanations
- report score and max-result expectation failures

The crate does not own retrieval, storage, model providers, or adapter-specific
fixtures. It is the deterministic harness that future SQL, vector, native, and
TypeScript paths should share.
