# Release scaffolding (RFC-0021)

- `platform-package.template.json` — the per-target package manifest the
  release workflow stamps (`__TARGET__/__OS__/__CPU__/__VERSION__`) next to
  each built binary. Full variants append `-full` to the name and carry the
  fastembed-enabled binary.
- Publishing is tag-driven, dry-run first (RFC rollout step 4): build
  matrix → scan gate → checksums/sign/SBOM → GitHub Release → npm publish
  with provenance. All-or-nothing: a scan failure stops before npm.
