# Provenance: vendored tantivy-sstable 0.3.0

## Upstream identity

Crate `tantivy-sstable` version 0.3.0, published to crates.io (registry `https://github.com/rust-lang/crates.io-index`).
Upstream repository: https://github.com/quickwit-oss/tantivy (published from the `sstable` subdirectory, `path_in_vcs: "sstable"`).
Upstream git revision recorded by the archive: `2fb381be70c03d5f84dadcaf89ae2fcb4170c8c1` (from `.cargo_vcs_info.json`).
Registry artifact: `tantivy-sstable-0.3.0.crate`, sha256 `c69578242e8e9fc989119f522ba5b49a38ac20f576fc778035b96cc94f41f98e` — the same checksum Cargo.lock pins at base.
This directory is a byte-for-byte copy of the local registry extraction (`~/.cargo/registry/src/index.crates.io-*/tantivy-sstable-0.3.0`), except the single manifest line diffed below and this note.

## The one local change

`Cargo.toml`, under `[dependencies.zstd]`, exactly one line changed:

```diff
-version = "0.13"
+version = ">=0.13, <0.15"
```

`features = ["experimental"]` is untouched; every other byte of the upstream tree is unchanged.
Verify anytime with `REG=$(ls -d ~/.cargo/registry/src/*/tantivy-sstable-0.3.0) && diff -r -x PROVENANCE.md "$REG" vendor/tantivy-sstable | grep -cE '^[<>]'` (expect 2).

## Why this override exists

The workspace uses tantivy 0.22.1, whose chain `tantivy-columnar 0.3.0 → tantivy-sstable 0.3.0 → zstd ^0.13` pins zstd 0.13.3 / zstd-safe 7.2.4 / zstd-sys 2.0.16+zstd.1.5.7.
Four memory-safety-class advisories — RUSTSEC-2026-0391, RUSTSEC-2026-0398, RUSTSEC-2026-0403, RUSTSEC-2026-0410 — are fixed only at zstd ≥0.14.3 / zstd-safe ≥8.2.0 / zstd-sys ≥2.1.2, and no fixed 0.13.x/7.x release exists.
The semver caret on `^0.13` blocks the 0.14 line, so this vendored copy widens the requirement by one line (`>=0.13, <0.15`) and a `[patch.crates-io]` stanza in the workspace root Cargo.toml redirects resolution to it (job 006-zstd-advisory-remediation, decision DEC-001).
Policy honored: vulnerabilities are FIXED in the lockfile, never ignored (deny.toml ignore stays `[]`).

## Licensing

The 0.3.0 archive ships no LICENSE file (archive contents: Cargo.toml, Cargo.toml.orig, README.md, benches, src).
Licensing travels as the manifest-declared expression `license = "MIT"` (Cargo.toml of this archive) — MIT is allow-listed in deny.toml.
License text: https://github.com/quickwit-oss/tantivy (see the repository root; no license file was fabricated here).

## Un-vendor trigger

Remove this directory and the `[patch.crates-io]` stanza when any released tantivy line's dependency graph takes zstd ≥0.14.3 on its own, or when upstream ships a tantivy-sstable 0.3.x backport with the widened requirement (job 006 follow-up F-1; decision DEC-004).
The first tantivy line taking zstd ^0.14 is 0.27.0, which declares rust 1.88 against this workspace's 1.85 — adopting it is a separate migration job, not this remediation.
