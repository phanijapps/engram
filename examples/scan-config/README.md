# Sample scan config

A checked example of the scan-filter config consumed by the `scan_repo` MCP
tool. Copy this to `<repo>/.engram/scan.json` (or pass its path as the
`scan_config` arg to `scan_repo`) to tune how the scanner indexes a repository.

> The config is **optional**. With no file, the scanner uses its builtin
> defaults (the prior hardcoded behavior). A missing or malformed config
> soft-fails to builtin — it never aborts a scan.

## Discovery ladder

`scan_repo` resolves the filter in this order:

1. an explicit `scan_config` path arg, else
2. `<repo>/.engram/scan.json` if present, else
3. the builtin defaults.

The result text reports which source was applied (or that the builtin was
used).

## Fields

All fields are optional. List-shaped fields are **additive** — your entries
join the builtin sets; they do not replace them.

### `concepts` — cross-document concept linking

Controls which concept names earn a `mentions` edge across documents. Without
tuning, the scanner links specific, non-generic names >8 chars. These knobs
adjust that for your codebase.

| Field | Type | Effect |
| --- | --- | --- |
| `min_name_length` | number | Override the 8-char threshold. Names at or below this length are not linked. |
| `blocklist` | string[] | Additional generic terms to **not** link (merged with the builtin ~50 terms: `authentication`, `repository`, `middleware`, …). Use for project-specific words that are common-but-meaningless in *your* repo. |
| `allowlist` | string[] | Terms to **always** link, even if short or blocklisted. Checked first; overrides `blocklist` and `min_name_length`. Use for short domain terms you want connected (`api`, `Auth`). |

Both lists match **case-insensitively**.

### `deny` — file skip list

Extends the always-skip denylist (the builtin skips `node_modules`, `target`,
`dist`, `.git`, … and `.db`/`.log`/`.lock`/… extensions).

| Field | Type | Effect |
| --- | --- | --- |
| `dirs` | string[] | Directory names to skip anywhere in the tree. Matched against path segments **case-sensitively** (like the `ignore` crate's directory semantics). |
| `extensions` | string[] | File extensions to skip. **Case-insensitive** (`.SVG` matches `svg`). Give a single segment after the last dot — `svg`, `map`, `proto` — not `min.js` (a file `app.min.js` has extension `js`). |

> `.gitignore` and `.ignore` are already honored by the walker (the `ignore`
> crate). Use `deny` here for project-specific exclusions you want enforced
> regardless of gitignore. `.npmignore` is **not** recognized.

## Trying it out

```bash
# drop the sample into a repo and scan
cp examples/scan-config/scan.json /path/to/repo/.engram/scan.json
# then call scan_repo { "path": "/path/to/repo" } via the engram-mcp server
```

Or point at it explicitly:

```jsonc
// scan_repo args
{ "path": "/path/to/repo", "scan_config": "/abs/path/to/scan.json" }
```

The scan result ends with a line like `scan_config applied: …`,
`.engram/scan.json applied`, or `no scan config; builtin filter`.

## Where the logic lives

- Config shape: `ScanFilterConfig` in `adapters/ingest/src/scan_filter.rs`.
- Spec: `docs/specs/scan-filter-config/spec.md`.
