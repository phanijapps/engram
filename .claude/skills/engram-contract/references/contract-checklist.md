# Contract Checklist

Use this checklist before changing contract-affecting files.

## Compatibility

- Is the domain model still storage-neutral?
- Are identifiers still opaque?
- Are existing field names, enum values, and meanings preserved?
- If a field is new, is it optional unless a new contract version is being created?
- If a new enum value is added, can consumers tolerate unknown values?
- Is the change reflected consistently in JSON Schema and planned generated types?

## Required Domain Traits

- Durable records include `id`, `scope`, `provenance`, and timestamps where applicable.
- Retrieval outputs include scores, explanations, omissions, and source failures where applicable.
- Memory, source-grounded knowledge, beliefs, and hierarchy nodes remain separate entities.
- Evidence references point to source memory, knowledge chunks, or derivations without embedding provider-specific payloads.
- Policy inputs are typed fields, not metadata-only conventions.

## Rejection Triggers

- SQL table names, vector-store namespaces, file paths, Node.js objects, or Rust implementation details in the portable model.
- Embedding bytes in durable domain records instead of `EmbeddingRef`.
- AgentZero-specific concepts such as `ward_id` without a portable equivalent.
- LLM-generated beliefs without evidence, confidence, status, and contradiction handling.
- Silent retrieval failures without `RetrievalSourceFailure` or equivalent visibility.
