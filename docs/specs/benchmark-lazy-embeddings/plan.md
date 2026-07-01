# Plan: benchmark-lazy-embeddings

### T1 — Research hypothesis draft
- **Tests:** N/A (research).
- **Approach:** Use the research-pack to draft the lazy-embeddings hypothesis: why runtime embedding should work (knowledge graph provides structural retrieval, embeddings add semantic), what the tradeoffs are (latency vs index speed), what prior art says.

### T2 — Index Microsoft Terminal
- **Tests:** goal-based — successful index of 500K+ lines.
- **Approach:** Clone microsoft/terminal, scan via `/ingest/jobs` with force. Record: wall-clock time, entities, relationships, chunks, file count. No embeddings during indexing.

### T3 — Build eval suite (10+ Q&A questions)
- **Tests:** TDD — eval fixtures.
- **Approach:** Write 10+ questions about the Terminal codebase with known answers (e.g., "What does TerminalSet do?", "How does the rendering pipeline work?"). Score each answer: correct/partial/wrong + citation accuracy.

### T4 — Run evals (no embeddings vs lazy embeddings)
- **Depends on:** T2, T3
- **Approach:** Run the agentic Q&A on all eval questions (no embeddings). Then enable FastEmbed at Q&A time (embed query + top-K chunks, re-rank). Re-run. Record scores.

### T5 — Write PERFORMANCE.md + charts
- **Depends on:** T4
- **Approach:** Generate charts (matplotlib/quickchart): indexing time bar, eval score comparison, entity/relationship counts. Write `docs/perf/PERFORMANCE.md` with machine specs, methodology, results table, charts, honest conclusion.
