# Portable acquisition resource and custody design

This document describes the current candidate, not feature acceptance or supported-host runtime proof.
Owner choice B preserves Linux x86-64/ARM64, macOS Apple Silicon and Windows x86-64.
Its supplemental authority is `remote-storage-platform-decision.md`, SHA-256 `5b2c07e2cd196ca9f1d78272e7391a932195120d6f009ae7d6ed722aef508dab` in the job evidence directory.

## Owned boundaries

- Use HTTPS smart-Git v0 or an explicitly configured OpenSSH upload-pack RPC.
- Do not run local fetch, clone, index-pack, object import, credential helpers or filters for remote acquisition.
- Retain reqwest/rustls with normal hostname/SNI verification, explicit CA roots, no proxy and no redirect fallback.
- Own each HTTPS request's current-thread Tokio runtime and hickory resolver; apply its timeout to DNS/TLS/headers/body and drop the runtime before the joined owner thread returns.
- Bound advertisements, reference/capability counts, response bytes, pack objects, inflation, delta reconstruction, tree entries, scheduled traversal, depth and selected source bytes before their corresponding accumulation or scheduling.
- Validate SHA1DC or SHA256 pack checksums and object identities, directory references, exact names and supported file modes before cache persistence.
- Admit the entire one-file fetched-data write-set before creating `objects.pack`.
- Create no bare Git metadata, index, temporary pack copy, alternates or loose-object import.
- Bootstrap real owner-private directories and verify identities before source writes; use capability-relative child I/O and cleanup rather than unanchored Unix paths.
- Create Windows directory security descriptors with the current user owner and protected owner-only inheritable DACLs, including children whose token default owner might otherwise differ.
- Reject reparse/link roots, foreign owners and permissive preexisting managed roots rather than silently changing their permissions.
- Preserve exact path case and UTF-8 component identity; reject traversal, streams, device names, reserved punctuation and trailing dots/spaces rather than renaming them.
- Precreate every zero-byte retained file before content writes, so the actual destination filesystem detects case, normalization, short-name and file/directory aliases.
- Reopen retained file identities before writes and compare exact acquired bytes after retention in the loopback integration fixture.

## Limit semantics

`max_fetched_storage_bytes` bounds the sum of logical regular-file bytes written to one attempt-owned fetched cache.
Its complete fetched-data write-set is one admitted pack file.
The ceiling is not a physical-block quota, exact RSS ceiling, global retained-cache ceiling or bound on unrelated live stores.
Transport, cumulative inflated object/delta work, object count, advertised references, delta/tree depth, total tree entries, scheduled traversal, selected file count, per-file bytes and selected snapshot bytes have independent admissions.
Repeated subtree references consume traversal and entry work again.
Allocator/container, TLS and OS buffers add overhead; these logical admissions are not an exact whole-process memory measurement.
The retained snapshot is a separate operation with explicit default or caller-supplied finite file/byte/depth admissions.
No remote Git helper imports objects or writes cache data.

## Process and durability behavior

Pipe draining and protocol stdin use no I/O threads or EOF-dependent joins.
Unix uses nonblocking descriptors and keeps the group leader unreaped through the owned-group stop, preventing PID-reuse group signaling.
Windows uses nonblocking pipe modes and an assigned suspended-child kill-on-close job before any helper code resumes.
Assignment/resume failures explicitly stop and reap the child.
Windows cleanup polls boundedly for both reaped leader and zero active job processes.
A failed Windows cleanup retains the exact handles in a bounded registry shared with a sixteen-helper admission, reports `cleanup_pending`, and retries through those handles rather than an unfenced PID.
Active process deadlines permit a separate bounded 250 ms Windows cleanup grace.
Unix process-group termination does not grant custody over a process which deliberately creates a different session; the escaped-session test owns and stops that fixture separately.
Regular filesystem and OS configuration calls are not advertised as physically preemptible real-time operations.

Retained files are flushed before publication and made read-only.
Directory sync reopens a real read-capable handle rather than syncing a Unix O_PATH capability.
Windows directory opens use backup/reparse-aware flags and write access for flushing; no-replace publication uses MoveFileExW write-through semantics.
Linux/macOS use their no-replace rename primitives under the parent capability.
Actual Windows/macOS execution must validate these filesystem/durability semantics; compilation is not that proof.

## Failure and crash custody

Prepare the attempt guard before mkdir and establish custody immediately after creation, before verification or source writes.
Normal failures and caught operation unwinds attempt identity-checked, capability-relative cleanup and parent synchronization.
A removal, identity or synchronization failure reports typed local custody details: operation failure, `cleanup_pending`, exact retry owner, target, filesystem identity and permitted next action.
Native path units preserve non-UTF-8/UTF-16 targets in trusted diagnostic detail and use private markers so public error redaction cannot expose their encoding.
These diagnostic details are not deletion authority or a new frozen-v1 wire payload.
A retained tree already published before a final synchronization failure reports complete-retained-publication-no-delete and remains intact.
Successful caches are never automatically deleted; later reference/lease/reset authority remains mandatory.
Process-crash leftovers are unclassified and never silently reused or treated as deletion permission.
An independent subprocess exit fixture proves leftovers remain discoverable and cannot be overwritten by a same-name attempt.

## Proof boundary and required runtime CI

Deterministic native tests cover pending DNS cancellation, TLS/body stalls, hostname validation, published collision vectors, malformed packets and object formats, authenticated pack/delta boundaries, tree scheduling, reserved/aliased paths, blocked stdin, escaped-session pipe holders and storage/custody failures.
The positive integration case uses real loopback HTTPS and public-key SSH for SHA1 and SHA256, then acquires, retains, reopens and compares every selected byte.
Its AsyncSSH dependency is test-only, pinned in `tests/requirements-portable-acquisition.txt`, and installed into an owned venv by `scripts/ci/prepare-acquisition-fixtures.py`.
It requires no SSH daemon, service, OS account, privilege change or production connection.
Fixture descendants have a separate Unix process group or Windows job.

`.github/workflows/portable-acquisition.yml` requires native Linux x86-64/ARM64, macOS ARM64 and Windows x86-64 rows and checks their actual Rust host triples.
The widened source compile guard also checks actual storage, snapshot, object, job and process implementations and their Windows tests, without unrelated native parser cross-toolchains.
No macOS/Windows runtime PASS is asserted locally, no external workflow execution is fabricated and no source is pushed or uploaded by this builder.
TASK-008 remains NOT_DONE until required supported-host runtime rows and independent security review accept the final source.
