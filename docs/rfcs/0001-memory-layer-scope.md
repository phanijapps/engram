# RFC 0001: Memory Layer Scope

## Status

Draft

## Problem

Agent systems need memory that is reliable, inspectable, permission-aware, and
composable across runtimes. A useful memory layer should not be tightly coupled
to a single framework, model provider, or storage engine.

## In Scope

- Memory write and retrieval contracts.
- Provenance and confidence tracking.
- Policy-aware retention, redaction, and forgetting.
- Hybrid retrieval across semantic, structured, temporal, and relational data.
- Consolidation from raw traces into durable facts.
- Evaluation fixtures for recall quality and safety behavior.

## Out of Scope Initially

- Full hosted control plane.
- Multi-tenant billing and organization management.
- Framework-specific agent orchestration.
- Model training or fine-tuning pipelines.

## Open Questions

- Should the first runtime be TypeScript, Python, Rust, or a split design?
- Should event sourcing be mandatory from the first vertical slice?
- Which storage backend should be the local development default?
- What is the minimum useful evaluation dataset?
- How should memory policy be expressed: code, config, or declarative rules?

## Proposed Initial Capabilities

### Write Memory

Accept an observation or explicit fact, attach provenance and policy metadata,
deduplicate where practical, and persist it.

### Retrieve Memory

Given a task, query, actor, and scope, return ranked memories with explanations
and policy filtering.

### Consolidate Memory

Convert noisy event history into durable records with confidence changes and
links to source evidence.

### Forget Memory

Delete, redact, or tombstone memory records while preserving audit requirements
where allowed.
