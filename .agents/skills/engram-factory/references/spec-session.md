# Shared spec state and author-led submission

Use controller operations, not shared editing of board.json. Direct JSON sharing
makes concurrency/authority ambiguous. A copied context packet alone becomes stale.
The selected interface combines a read-only live packet with revision-bound
submission and existing immutable review evidence. No scheduler/service is added.

## Orchestrator setup

For an active spec phase BEFORE the first review candidate, select the author
invocation and required independent reviewer roster, then run:

`factory.py --repo REPO spec-session-open --job ID --author AUTHOR --reviewers-file ROSTER`

The roster file is job-relative and uses the existing role-to-invocation mapping.
The controller requires Red Forman, distinct reviewer invocations and no author
as reviewer. It stores an immutable cycle-bound session as accepted evidence.
It cannot be replaced, narrowed, or retrofitted over an existing review. Existing
jobs without a session retain their prior workflow; do not migrate them silently.

## Analyzer loop

1. Run `spec-context --job ID`. Read the brief/decisions and accepted source inputs
   by the returned paths/hashes; inspect source in the assigned worktree. The
   packet reports job/cycle/base, board revision, context identity, fixed roster,
   outstanding findings by reviewer, immutable report references, request refs
   and whether a prior verdict applies to current bytes. It is not owner authority.
2. Author only spec.md. Run `spec-check --job ID` without preparing a receipt.
   This read-only check shares canonical draft validators with spec-prepare.
   Fix structural findings locally. Neither check consumes a review candidate.
3. Submit the actual spec using the hash returned by spec-check, the context
   identity/revision from spec-context, and one stable operation ID:

   `factory.py --repo REPO --expected-revision N spec-submit --job ID --author AUTHOR --operation-id OP --context-sha256 CONTEXT --spec-sha256 SPEC`

   The controller checks session/cycle/stage, accepted inputs, revision, context
   and spec bytes, prepares the receipt, rechecks the handoff, freezes exact inputs
   with the fixed roster, and writes one immutable review request. It does NOT
   record verdicts, accept the stage, grant Start, or start a remote process.
4. The analyzer spawns the pinned independent reviewers for a fresh request,
   against exactly its candidate, packet and roster, then waits for actual results.
   Use native nested delegation when available and authorized; otherwise return the
   durable request to the orchestrator/runtime. Never substitute self-review or
   invent invocation capabilities. Stop editing frozen inputs while review is
   pending. The actual reviewer reports are ingested by the existing trusted
   orchestrator review-record path, not by the author.
5. Inspect spec-context for results. CHANGES returns actual role-scoped findings:
   revise and submit a NEW operation within the existing candidate limit. ERROR,
   exhaustion or input drift requires existing recovery/supervisor handling.
   PASS means return the candidate for orchestrator acceptance; the author never
   calls complete, review-record, approve, Start, Finish, merge or rework.

Exact same-operation retries return the original immutable request, even if the
board revision advanced, only when the original invocation/input/candidate/archive
identities still hold. Replayed requests explicitly forbid redispatch; consult
current context. Different arguments under one operation ID, stale new requests,
changed spec/inputs, or changed candidate metadata fail. Request existence is NOT
proof a reviewer launched. The host must deduplicate launches by request identity;
there is no exactly-once remote launch guarantee in this local controller.

## Boundaries and recovery

Context/check do not create locks, receipts, evidence or board upgrades. Submission
uses the existing board lock and review freezer. A filesystem/Git/board transaction
is not atomic across all files: a crash after freeze but before request/board
commit can leave inspectable orphan/pending evidence. Fail closed and preserve it;
no automatic adoption, candidate reset, inferred approval or force repair is added.
Timeout is not invocation quiescence. The existing job lease remains intact.

Invocation strings and session creation are caller assertions, not authentication.
The local controller shares the current user's filesystem trust boundary. Prompts
and wrapper sandbox modes are not fine-grained API authorization. A trusted host
must restrict role tool access; the analyzer has no new owner powers. Codex author
workspace-write permits its authored file and guarded submission; role instructions
forbid arbitrary board/source/evidence writes. Independent reviewers remain separate.

No speedup or token saving is assumed. Record actual dispatch/finish/tool/provider
measurements when available; a needs-dispatch request has unknown execution time.

Send periodic and milestone parent messages under shared.md for parent progress
updates throughout drafting, review waits and correction. Messages carry compact
status/evidence references; persisted controller evidence still governs acceptance.
