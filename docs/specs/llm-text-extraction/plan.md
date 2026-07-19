# Plan: llm-text-extraction

## Tasks

### T1 — Enable LLM enhancement for text docs in the scan
- **Tests:** goal-based — typecheck; curl smoke with creds on a markdown file.
- **Depends on:** none
- **Approach:** In `scanner.rs`, after deterministic extraction of a text document, if `getLLMConfig()` (passed via ScanOptions as a new optional field), call `extractGraph(text, "text", config)` → `parseLLMGraph` → persist via the knowledge transport. Reuse `enhance.ts`'s pattern. The scan's per-file progress reports whether LLM enhancement ran. For `/ingest/extract`, the existing LLM-enhance toggle already covers text; this adds it to the scan path.

### T2 — Validate Q&A grounding on text entities
- **Tests:** goal-based — ingest a skill doc; ask "how does X work?" → answer cites the doc's entities.
- **Depends on:** T1
