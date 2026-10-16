# CI promotion readiness

Mode: focused CI repair; reduced review per owner instruction.

## Objective

Restore the contract, Rust, TypeScript and dependency-audit CI checks that failed on PR #123 before branch promotion.

## Acceptance Criteria

- CI explicitly installs tools required by contract and boundary scripts, including ripgrep.
- A clean TypeScript checkout builds workspace package exports before dependent typechecks and tests.
- Workspace and FastEmbed feature Clippy checks pass with warnings denied on the CI stable toolchain.
- The minimum supported Rust version is 1.88, required by the patched `time` dependency; current setup guides agree.
- The committed dependency graph has no cargo-audit vulnerabilities, with no advisory suppression or removal of supported backends.
- Existing portable contracts, generated types, retrieval behavior, model settings and evaluation reports remain unchanged.

## Testing Strategy

Captured PR #123 logs reproduce the four failures. Verify setup changes using clean package outputs and the same CI command order; verify lint changes with actual Clippy; verify dependencies with a freshly fetched RustSec database and workspace/feature compilation and tests. Run a real built MCP smoke after Rust dependency changes. Existing accuracy failures remain explicitly unresolved by this CI repair.

## Compatibility

Serialized contracts and generated TypeScript types are unchanged. Rust embedders constructing the pre-release `DocumentHeadWrite::Chunk` variant now wrap the chunk in `Box::new(...)`; this reduces the size of every queued document write and fixes the large-enum warning. Compatibility classification: breaking for this Rust constructor; compatible for serialized contracts and TypeScript APIs. The minimum supported compiler moves from 1.85 to 1.88 for patched dependencies and array-slice APIs.
