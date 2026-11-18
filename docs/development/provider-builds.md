# Provider builds and bounded proof

Provider selection and inference are independent. The portable factory is
`engram-backend-provider`; its default is SQLite. Bare integration remains usable
without SQLite, PG, model SDKs or a provider recipe. Host crates expose optional
`sqlite`, `pgvector`, `helixdb-service`, `helixdb-embedded` and convenience `helixdb`
features. Inference features `fastembed` and `http-embeddings` are separate.

## Build selection

```sh
cargo +1.88.0 check -p engram-backend-provider --no-default-features
cargo +1.88.0 build -p engram-mcp --no-default-features --features helixdb-embedded
cargo +1.88.0 build -p engram-mcp --no-default-features --features helixdb-service
```

MCP defaults still include SQLite, FastEmbed and served embeddings. Node addon
defaults include SQLite and served embeddings; its production `build:native` script
also enables FastEmbed for SQLite. Normal addon publication uses release, named
Rust 1.88.0, a shared build lock and immutable copies. Tests use debug by default:

```sh
node packages/node/test/helpers/build-native-recipes.mjs --providers sqlite,pgvector,helixdb-service,helixdb-embedded
node packages/node/scripts/build-native.mjs helixdb-embedded
```

The helper's `profile` option accepts `debug` or `release`. Production explicitly
requests release and reads `target/release`; test recipes read `target/debug`.
Artifact directory, filename and JSON sidecar identify the profile. Debug cannot
satisfy release lookup. Sidecars also bind recipe, features/Cargo argv, source and
diff hashes, lock hash, platform, architecture and artifact digest. Publication
retains immutable files; a currently loaded inode is not overwritten. A nondefault
instrumented artifact has a separate namespace and includes `test-controls`.
Its default profile is debug. It is not a production dependency-closure receipt.

Run commands with the repository's pinned Node 22/pnpm 10 and named Rust toolchain.
Use disposable trusted roots and fixture credentials; never point proof at a live
user store. CI must report actual missing linker/runtime/addon/trace access as
blocked. Do not substitute a cross-compile, directory name or metadata declaration
for import, construction, operations and shutdown on the native target.

## Inventory versus execution

```sh
python3 tools/ci/run_provider_e2e.py --check-inventory --self-test-omissions
python3 tools/ci/check_provider_builds.py --matrix single --all-supported-targets --with-embeddings --output runs/provider-builds/single
```

The first command checks complete operation/host/mode/target inventory and deliberate
omission controls. It does not execute E2E. The build gate also retains its legacy
`--package` and `--features` entry points; matrix selection is `single` or
`explicit-combinations`. Report normal/build dependency trees and link inputs
separately from dev/test feature-unified graphs. Restoring conformance as a normal
provider dependency must fail production closure, as must forbidden SQLite/PG
leakage in a selected Helix-only artifact. Normal artifacts must omit test controls.

The artifact input schema is `engram.provider-artifacts.v1`. It binds source revision
and lock SHA-256, plus each artifact's host, provider, mode, target, path, digest,
features, profile, production dependency graph and link inputs. Those declarations
still require verification against actual consumed files and native observations.
Missing, stale, mismatched, skipped or omitted required cells are non-PASS, not
successful zero-work runs. Explicit one-shot N/A requires the operation inventory's
reason; an unknown capability or missing fixture is not N/A.

Execution requires `--expected-build-record FILE` and
`--expected-build-record-sha256 HASH`. The record is delivered separately, outside
the artifact collection, and its hash comes from trusted orchestration. A hash
read from the artifact bundle cannot supply this independent pin. The schema is
`engram.provider-build-record.v1`: producer repository/run/commit, complete tracked
source file types and byte hashes plus approved new source files, source-snapshot
digest, lock hash, actual Cargo target/kind/features and compiler/link-record hashes,
produced artifact hashes, and every operation/fault template hash. The consumer
checks actual checkout bytes and all selected plans before creating a store or
helper. This provides content integrity under a trusted pin; it does not authenticate
an author's identity or implement PKI. CI verifies real GitHub run/artifact delivery
identity and retains the original template hashes when it relocates paths.

The producer accepts `--request-plans FILE` with schema
`engram.provider-build-plans.v1`. Each plan names `package`, `target`, exact
`features`, a real `request_plan`, and `operation_plans` keyed by host. Runtime
commands are fixed repository consumers; caller-selected executables are rejected.
The bootstrap plan identifies `proof_obligations.sentinel_write_call_id` and
`sentinel_call_id`: an actual memory write and a read of that same ID, full scope
and value, repeated after owner release and reopen. It does not attest a held
native-close handle. Missing plans block runtime acceptance. Plan storage paths,
logical stores and matching scopes are rebound to fresh owned namespaces; `$fixture`
path references and `seed_files` describe portable input fixtures. Publication emits
`artifacts.json`; operation plans must contain all applicable calls for that host.

Selected E2E is **Helix only**: E2E-003 service, E2E-004 embedded. Ordinary SQLite/PG
compatibility, conformance and CI remain in place; this adds no SQLite/PG E2E campaign.
The target inventory matches release packages: `linux-x64-gnu`, `linux-arm64-gnu`,
`darwin-arm64`, `win32-x64`. Inventory membership does not assert execution on those
machines. Unsupported or unavailable target instrumentation stays blocked.

The workflow invocation contract is:

```sh
python3 tools/ci/run_provider_e2e.py --provider helixdb --mode embedded --all-supported-targets --expected-build-record "$ENGRAM_PROVIDER_E2E_EXPECTED_BUILD_RECORD" --expected-build-record-sha256 "$ENGRAM_PROVIDER_E2E_EXPECTED_BUILD_RECORD_SHA256" --output runs/provider-embedded
python3 tools/ci/run_provider_e2e.py --provider helixdb --mode service --all-supported-targets --expected-build-record "$ENGRAM_PROVIDER_E2E_EXPECTED_BUILD_RECORD" --expected-build-record-sha256 "$ENGRAM_PROVIDER_E2E_EXPECTED_BUILD_RECORD_SHA256" --output runs/provider-service
```

`--target` narrows to one declared target. `--artifacts` selects the artifact JSON;
its default is `runs/provider-builds/single/artifacts.json`. `--phase` is `operations`,
`faults` or `all` (default). The output is a compact `report.json`, `operations.jsonl`
and `raw.log`, bound to the actual invocation rather than one report per cell.
These commands are interfaces, not evidence that a workflow has run. Task-006
harness-control validation does not discharge final Task-007 native fault/network
proof or separate packed-install QA.

The explicit GATE-022 test adapters additionally read
`ENGRAM_PROVIDER_E2E_ARTIFACTS`, `ENGRAM_PROVIDER_E2E_EXPECTED_BUILD_RECORD` and
`ENGRAM_PROVIDER_E2E_EXPECTED_BUILD_RECORD_SHA256`. Missing input fails the gate;
it does not skip its actual cases. MCP's existing launch scope supports a tenant
and `Some(project)` workspace, including an explicit empty project. A missing/null
workspace or any non-null subject/session/environment (including empty strings)
is rejected before startup, rather than silently changing the declared Scope.

## Owned service and fault fixtures

The canonical local helper commands are:

```sh
/usr/bin/python3 tools/ci/helixdb_server.py provision --output runs/helix-c3-component-server
/usr/bin/python3 tools/ci/helixdb_server.py cleanup --output runs/helix-c3-component-server
```

Use the helper-emitted endpoint and credential environment-variable **name**; do not
print its secret value. Use distinct invocation-owned roots, retain failed data,
and record exact provision/cleanup commands. Cleanup requires exact pidfd identity,
not a numeric PID or process-group kill fallback. A failed or ambiguous close blocks
replacement; it is not fixed by deleting the data or starting another helper.
Service client close must never stop an externally managed server.

Current effective PLT-001 still denies service with vectors off. Early service
fixtures explicitly enable vectors with embedding provider `none`. Final ratified
storage/auth network-policy application and actual native tracing remain separate
proof obligations; do not claim the future exception is already active.

Fault controls must bind the executed production or instrumented artifact, host,
mode, logical store, full scope and original mutation UUID. Typed BeforeSend and
AfterAck observations must originate at real adapter dispatch/validated receipt
boundaries. A counter or recorder declaration is not such an event. Keep physical
reply/loss observations distinct from canonical acknowledgment and exact receipt
reconciliation. The service gateway and send recorder use owned endpoints/data;
only the final integrated protocol establishes actual landed writes, no later or
replay sends, definite-failure continuation and blocked admission with no new UUID.

Unknown outcome retains receipt custody. Do not replay the original logical write,
whole batch or source operation. Earlier successes and skipped later steps must
remain visible through Rust, native, stdio and applicable one-shot envelopes.
See [composition and archive limits](../architecture/helixdb-provider.md).
