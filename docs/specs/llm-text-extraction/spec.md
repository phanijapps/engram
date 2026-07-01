# Spec: llm-text-extraction (LLM entity extraction for text/markdown docs)

- **Status:** Draft
- **Shape:** service
- **Constrained by:** RFC-0004 D2 (pi SDK for extraction); existing `parseLLMGraph` validator; security controls (output validation, call bounds, credential isolation)
- **Contract:** none

## Objective

Markdown/text documents (RFCs, ADRs, skill docs, READMEs) get the same quality
entity + relationship extraction as code files. Currently the deterministic
extractor uses `concept_name(first_line)` — it creates one concept per chunk with
no relationships. LLM extraction on text docs produces entities (person,
organization, project, concept, value_stream, requirement, api) + relationships
(mentions, relates_to, satisfies, implements) with provenance + confidence.

## Decision

When a text/markdown document is ingested (via `/ingest/extract` or the scan's
`/ingest/jobs` with LLM creds), the system runs `extractGraph(text, "text",
config)` from `llm.ts` alongside the deterministic pass. The LLM-extracted
entities + relationships are persisted into the same graph via `enhance.ts`'s
`enhanceWithLLM`. This reuses the existing Slice-2 enhancement pipeline — the
only change is enabling it for text documents during the scan (not just the
manual `/llm/extract` route).

## Boundaries

**Always do** — reuse `enhanceWithLLM`; validate output via `parseLLMGraph`; bound calls; creds server-side.
**Never do** — change Rust; add a second LLM path; skip deterministic extraction.

## Acceptance Criteria

- [ ] Scanned markdown/text files produce LLM-extracted entities + relationships when creds present.
- [ ] `/chat` "Do you know when and how to use the new-rfc skill?" finds the skill doc entities + relationships, not just "concept" fragments.
- [ ] Deterministic fallback when no creds; no regression on code files.
