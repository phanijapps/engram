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
