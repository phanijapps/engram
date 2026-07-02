# Repository Scripts

This directory holds repository-owned automation that is shared by hooks,
package scripts, and release checks.

- `generate_ts_contracts.py`: generates the TypeScript contract projection from
  accepted v1 schemas.
- `validate_contracts.py`: validates accepted examples and invalid fixtures
  against the v1 schema package.
- `update_roadmap_phase.py`: updates roadmap phase metadata.

Keep tool-specific wrappers in `tools/hooks/` or `.codex/hooks/`; keep reusable
Python automation here.
