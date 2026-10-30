# Bound Code Operations Runbook

## Reader and purpose

This runbook is for an operator who administers project-bound code indexing and its disposable verification fixtures.

## Bound ingestion

1. Authorize the scope in the host before any code operation.
2. Register the approved locator with an opaque `RepoId`.
3. Bind exactly one full `refs/heads/...` ref to that repository and scope.
4. Run the bound scan or rebuild through the registered-locator pipeline.
5. Inspect status and refresh only when an on-request ref observation is needed.

The required order is register, bind, then scan.
The registered locator is the acquisition authority.
A caller path, a current checkout, a checkout `HEAD`, and a caller-supplied object ID never authorize bound ingestion.
Bound ingestion reads retained snapshot bytes and never checks out, switches, fetches into, or otherwise mutates a developer working tree.
A same-ref bind retry is idempotent.
A different ref for an existing binding returns `BindingConflict` and is not a branch-switch workflow.
`code_status` reports the indexed OID separately from the last observed ref OID.
`code_refresh` observes the configured ref from the registered locator and does not fetch or change a checkout.

## Host authorization

Recipe defaults deny every code operation.
For native and TypeScript hosts, inject the allowed launch scopes through `codeAuthority.launchScopes` when creating `NativeProvider`.
For example, configure `{"codeAuthority":{"launchScopes":[{"tenant":"acme","workspace":"payments"}]}}` alongside the normal provider configuration.
An omitted authority returns the typed `CodeAuthorizationRequired` denial.
The legacy `NativeIngestEngine.codeBindJson` surface is unavailable and returns `LegacyCodeBindUnsupported`.
Use `NativeProvider` or the TypeScript provider/client path with host `codeAuthority` injection instead.

## Reset lifecycle

1. Create an exact reset preview for the intended binding.
2. Review the preview hash, exact targets, ambiguous-ownership entries, and preservation digests.
3. Obtain the owner gate that names that preview hash.
4. Apply only the owner-approved preview.
5. Resume the durable journal after an interruption with the same authorization reference.

Do not apply a preview with ambiguous ownership.
Do not substitute a target, a subset, a binding version, an epoch, or a preview hash after owner approval.
The journal records exact target fingerprints, authorization decisions, checkpoints, and preservation digests.
A reset is targeted and resumable, not a scope wipe and not a cross-store rollback.

## Matrix runner

Run the matrix only against disposable fixtures.
Source PostgreSQL credentials from the absolute, owner-only `ENGRAM_TEST_PG_ENV_FILE`.
The file and its parent directory must be owned by the invoking user, the file mode must be `0600`, and the file may contain only the expected URL and fixture metadata keys.
Set `ENGRAM_TEST_REQUIRE_PG=1` only after the runner can preflight the pinned disposable database.

```bash
set -a
. "$ENGRAM_TEST_PG_ENV_FILE"
set +a
ENGRAM_TEST_REQUIRE_PG=1 python3 scripts/ci/check-bound-code-matrix.py \
  --sqlite-temp --pg-url-env ENGRAM_TEST_PG_URL --require-native --require-mcp \
  --fail-on-skip --receipt-dir /secure/evidence/bound-code-matrix
```

`--sqlite-temp` is mandatory and creates file-backed temporary SQLite stores.
`--require-native` and `--require-mcp` make their rows required.
`--fail-on-skip` rejects missing required rows instead of accepting a green skip.
Each row writes a redacted receipt and `manifest.json` records every receipt path, digest, exit value, requirement flag, and status.
Store receipts outside the repository and do not place credentials in argv, logs, or the manifest.

## Fixture rotation

The runner pins the disposable PostgreSQL container ID, labels, loopback port, tmpfs-only data mount, database name, and database comment marker.
Re-pin only during an approved fixture rotation.

1. Stop and remove the old disposable fixture after preserving required evidence.
2. Create the replacement with the same disposable labels, loopback-only port mapping, tmpfs data mount, and marker comment.
3. Record the new full container ID, database name, port, rotation time, and password-state metadata in the owner-only environment file.
4. Update `PINNED_CONTAINER_ID`, `PINNED_DATABASE`, and the expected labels in `scripts/ci/check-bound-code-matrix.py` in one reviewed change.
5. Run the matrix control tests and a preflight matrix invocation before accepting the new pin.

Never edit a running fixture into compliance or reuse a production database.
A failed identity probe is a refusal, not permission to bypass the pin.
