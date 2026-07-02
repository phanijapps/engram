# Repository Scripts

This directory holds repository-owned automation that is shared by hooks,
package scripts, and release checks.

- `generate_ts_contracts.py`: generates the TypeScript contract projection from
  accepted v1 schemas.
- `check_research_parity_docs.py`: scans research and architecture docs using
  `tools/research-parity/doc-drift-registry.json`; run
  `python3 tools/scripts/test_check_research_parity_docs.py` for its focused
  regression tests.
- `validate_contracts.py`: validates accepted examples and invalid fixtures
  against the v1 schema package.
- `update_roadmap_phase.py`: updates roadmap phase metadata.

Keep tool-specific wrappers in `tools/hooks/` or `.codex/hooks/`; keep reusable
Python automation here.
