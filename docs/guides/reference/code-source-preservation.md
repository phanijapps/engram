# Code source preservation reference

Code source preservation keeps the admitted source bytes for each indexed file
and records which semantic chunks cover which source ranges. Use `source_read`
when an agent needs the original file text; use normal code search or recall
when it needs indexed semantic results.

## What is stored

The scanner applies its existing admission, ignore, secret, path, and size
checks before accepting a file. Accepted code must decode as UTF-8. The stored
snapshot reproduces the accepted text exactly, including whitespace, BOM and
line endings. Invalid UTF-8, filtered files, and oversized files are not
snapshot-covered. Empty accepted files have a valid empty snapshot.

Each source view is identified by exact scope, repository key, and normalized
relative path. The active manifest records a generation, source digest,
extraction fingerprint, state, policy, provenance, and the records contributed
by that file. Reads return only an active generation. Semantic coverage is
returned separately from the exact snapshot text; parse quality (`clean`,
`recovered`, `fallback`, or `failed`) does not by itself say whether the source
snapshot is complete. Missing descriptors on older rows mean coverage is
unknown.

Snapshot chunks have role `source_snapshot`. They do not participate in normal
recall, code-search candidate selection, or model-document feeds. A source read
authorizes access to source bytes only; it does not grant permission to send
those bytes to a model. Code-source model feeds remain disabled until a
separate per-source and destination authorization contract exists.

## Reading source through MCP

`source_read` accepts `repository`, `path`, and optional `generation` and
`maxBytes` (default 1 MiB). `code_sources` lists source generations visible to the
same trusted context. Neither tool accepts scope or requester fields.

For HTTP MCP, provide the trusted context in a JSON file and pass its path with
`--source-context`:

```json
{
  "scope": { "tenant": "acme", "workspace": "payments" },
  "requester": {
    "actor": { "id": "repo-reader", "kind": "agent" },
    "permissions": ["code:source:read"]
  },
  "readerVersion": 1
}
```

```sh
engram-mcp-http --config ./engram.json --source-context ./source-context.json
```

Treat this file as trusted host configuration: the MCP client cannot replace
its scope or requester in a tool call. The dimensions in scope are matched
exactly, including absent/null dimensions. A private source can be read by its
recorded owner. Another requester needs `code:source:read` and still must pass
the source policy, including its allowed-use and expiry checks.

If HTTP MCP starts without `--source-context`, source operations report
Unsupported. Rust `engram-mcp` instead derives context from its launch-bound
scope and requester; it does not expose a separate `--source-context` flag.
Native Node callers create a bound source API by supplying a
`CodeSourceContext`; the API rejects reader versions other than 1.

## Repository identity and rescans

Git sources use a normalized remote identity (`host/org/repo`) that remains
stable across commits. For non-Git sources, the key falls back to the caller's
`sourceName`, normalized to lowercase. Supply a unique, stable `sourceName` for
each non-Git repository: the host's default source name is fixed, so multiple
unrelated non-Git roots using that default can collide within the same scope.

The scanner fingerprints its parser, mapping, context, and snapshot chunking
configuration. Parser/grammar or mapping upgrades must also update the source
assembly fingerprint; dependency updates alone do not change that constant. A fingerprint change invalidates the versioned host sidecar as a
scan shortcut and causes files to be reconsidered even when their bytes have
not changed. The durable generation manifest is authoritative; the sidecar is
only a versioned optimization. Failed writes leave the file generation
incomplete and retryable. Already successful writes are retained and retried
idempotently; this is BestEffort across persistence cells, not cross-store
rollback.

## Export and backend support

Authorized transfer uses a versioned `CodeSourceBundle` inside the existing
`ImportData` format. Export requires `code:source:export` in addition to source
policy. Import rechecks source-export and source-policy authority for every input and
requires `code:source:write` and `code:source:activate`, checked
again when applying. The current transfer supports preserving the exact source
scope and policies; scope translation and policy widening are rejected.

Source preservation is available only where the active provider advertises a
conforming code-source capability. Unsupported stores fail explicitly; they do
not turn an unsupported read into an empty source result. Consult
`capability_report` for the provider currently in use.

## Related contracts

- [Code source preservation spec](../../specs/code-source-preservation/spec.md)
- [Domain data model](../../domain-data-model.md#code-source-preservation-rfc-0021)
- [V1 compatibility rules](../../../contracts/v1/compatibility.md#code-source-preservation-rfc-0021)

Low-level `NativeKnowledge.searchCodeJson` callers must supply a requester when
searching source-owned code. Unbound legacy calls retain legacy-record behavior
and omit source-owned results; use the provider/MCP retrieval path for a bound
request. This does not grant raw snapshot or model-enrichment access.
