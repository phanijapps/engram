# CI promotion readiness plan

Repair CI tool installation in both continuous and release workflows, build exported TypeScript packages before consumer checks, and fix warning-producing Rust expressions. Upgrade vulnerable dependencies through their existing adapters while preserving the public contracts and backend capabilities. Verify the exact CI commands, a clean TypeScript build, the dependency audit and a live MCP invocation before pushing a focused PR.

Declining advisory ignores and disabled CI checks: these would conceal failures. Declining retrieval/scorer changes: accuracy remediation is separate from CI readiness.

## Tasks

- [x] Install required CI tools and validate contract checks.
- [x] Establish clean-checkout TypeScript build order and verify typechecks/tests/build.
- [x] Repair all workspace and feature Clippy errors on the CI toolchain.
- [x] Upgrade vulnerable dependency paths and run the complete audit.
- [x] Run gates, focused review and live MCP smoke.
- [ ] Commit, push and verify GitHub CI.
