# Phase 08 Spec: Knowledge Ingestion

## Status

Done for deterministic text ingestion and in-memory repository baseline.

## Scope

Ingest code and documents as source-grounded knowledge, not as agent memory.

## Acceptance

- Sources, documents, and chunks preserve provenance.
- Re-ingestion is idempotent when hashes match.
- Retrieval distinguishes memory results from knowledge results.
