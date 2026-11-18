# Release scaffolding (RFC-0021)

`stamp-platform-package.py <binary> <target> <version>` creates one internal npm
platform package per supported target. Every release executable includes embedding
support; `--slim` disables it at runtime. There are no full/slim artifact variants.

The only user-facing install is `@engramlabs/mcp`. Its optional dependencies and
the release target matrix must match. Release stamping updates their versions in
the packed artifact; it must not change the dependency names.

Before tagging, verify CI and Security are green on the exact commit. Use a fresh
immutable version: cancelling a workflow does not undo an npm publication, and
idempotent publication deliberately skips versions already on the registry.
Never move a release tag to retry different content.

The tag-driven workflow builds native binaries, exercises cold-cache `--slim`
startup/scan/search, stamps packages and checksums, creates a GitHub release, and
publishes npm packages with provenance. A dispatched dry run does not publish.
Confirm registry manifests and exercise a clean launcher installation after
publication; workflow success alone does not establish correct installed behavior.


## Provider artifact closure

The portable selected-provider factory lives in `backends/provider`; Helix service
and embedded builds are independent feature choices. Default SQLite release
behavior is retained. Do not infer that a published platform executable contains
Helix merely because the source supports it: record the actual feature set, target,
profile and consumed artifact hash. A missing packaged feature must remain an
explicit failure, not a fallback to a different store.

Node `packages/node/scripts/build-native.mjs` uses release and preserves SQLite's
FastEmbed production default. The shared recipe helper uses debug for focused tests
unless release is explicit. Profile-specific immutable paths and JSON sidecars
prevent a debug artifact from satisfying a release lookup. Instrumented
`test-controls` artifacts are separate and must not satisfy production closure.
See [provider build protocols](../../docs/development/provider-builds.md).

Inventory and omission controls are not release execution. The four release target
labels are linux-x64-gnu, linux-arm64-gnu, darwin-arm64 and win32-x64. Missing native
runners, syscall/DNS instrumentation, linker/runtime/addon, configured storage
service or credentials are blocked cells, never passing skips or cross-compile
substitutes. Retain normal/build dependency trees and link inputs separately from
feature-unified test graphs. Required operation families must be exercised through
the actual Rust/native/stdio/applicable-one-shot consumer, not just listed.

Only Helix service E2E-003 and embedded E2E-004 are selected here; existing SQLite/PG
compatibility CI remains. Current strict PLT-001 and future ratified application
must remain distinct until final integrated proof. Preserve invocation-owned
receipts and data after uncertain writes or cleanup; do not replay mutations or
relabel a successful inventory check as successful workflow, network or publication
proof. Packed-install QA and authoritative owner approval remain release gates.
