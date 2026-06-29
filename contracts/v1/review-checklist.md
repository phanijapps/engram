# v1 Contract Review Checklist

Use this checklist for every change to `contracts/v1/`, `specs/v1/`, or
accepted v1 sections of `docs/domain-data-model.md`.

## Wire Compatibility

- Field names are unchanged.
- Field meanings are unchanged.
- Required fields are not added to existing accepted shapes.
- Existing enum values are not removed or redefined.
- Identifiers remain opaque strings.

## Policy And Provenance

- Durable records include policy and provenance where required.
- Retrieval specs apply policy before context composition.
- Redacted and forgotten content cannot leak through explanations, metadata, or
  evaluation output.
- `training_export` remains deferred.

## Schema And Example Alignment

- Every accepted operation has a schema.
- Every accepted operation has at least one valid example.
- Invalid examples fail validation for the intended reason.
- `scripts/validate_contracts.py` passes.

## Extension Boundaries

- Belief, contradiction, hierarchy, taxonomy evolution, consolidation, and
  ingestion execution are not silently promoted into v1.
- Storage, provider, gateway, and binding details do not enter portable
  contracts.

## Generation Assumptions

- TypeScript types can be generated from accepted schemas.
- Rust projections can serialize to the accepted wire shapes.
- Generated artifacts are reproducible and not hand-edited.
