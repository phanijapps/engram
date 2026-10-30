/**
 * Shared TASK-020 case battery (review r6 findings 1/2/5), executed PER
 * RECIPE in its own test file. One addon copy per vitest FILE is the point:
 * vitest isolates test files in separate processes, so the two recipe
 * artifacts are never dlopen'd into ONE process (two copies of the same
 * napi cdylib in one process interpose napi class symbols and crash — the
 * r6a-fix2 defect this split repairs).
 */
import { expect, it } from "vitest";
import { mkdirSync, mkdtempSync, readdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { execFileSync } from "node:child_process";

export const scope = { tenant: "task-020", workspace: "bound" };
export const foreignScope = { tenant: "task-020", workspace: "other" };

export function configJson(base, extra = {}) {
  return JSON.stringify({
    storage_path: join(base, "store"),
    trusted_root: base,
    scope_policy: "Strict",
    embedding_provider: {
      provider_type: "none",
      model: "none",
      dimensions: 384,
      prompt_profile: "query"
    },
    migration_mode: "Apply",
    capability_policy: "FailClosed",
    enable_vector: false,
    ...extra
  });
}

/** The HOST policy injection (r6 finding 1): code ops allowed for `of`. */
export function authorizedConfig(base, extra = {}, of = scope) {
  return configJson(base, { ...extra, codeAuthority: { launchScopes: [of] } });
}

/** Creates a real local git repository with one commit on `main`. */
export function gitFixture(tag) {
  const base = mkdtempSync(join(tmpdir(), `task020-git-${tag}-`));
  const run = (args) =>
    execFileSync("git", args, {
      cwd: base,
      env: { ...process.env, GIT_CONFIG_GLOBAL: "/dev/null", GIT_CONFIG_SYSTEM: "/dev/null" }
    });
  run(["init", "--initial-branch=main", "."]);
  run(["config", "user.email", "task020@engram.test"]);
  run(["config", "user.name", "task020"]);
  writeFileSync(join(base, "lib.rs"), "pub fn fixture() -> u32 { 1 }\n");
  writeFileSync(join(base, "notes.md"), "# Fixture notes\n");
  run(["add", "."]);
  run(["commit", "--no-gpg-sign", "-m", "fixture"]);
  const oid = run(["rev-parse", "HEAD"]).toString().trim();
  expect(oid).toHaveLength(40);
  return { path: base, oid };
}

/** Advances a fixture by one committed code revision. */
export function gitFixtureWithContent(tag, source) {
  const base = mkdtempSync(join(tmpdir(), `task020-git-${tag}-`));
  const run = (args) =>
    execFileSync("git", args, {
      cwd: base,
      env: { ...process.env, GIT_CONFIG_GLOBAL: "/dev/null", GIT_CONFIG_SYSTEM: "/dev/null" }
    });
  run(["init", "--initial-branch=main", "."]);
  run(["config", "user.email", "task020@engram.test"]);
  run(["config", "user.name", "task020"]);
  writeFileSync(join(base, "lib.rs"), source);
  run(["add", "."]);
  run(["commit", "--no-gpg-sign", "-m", "fixture"]);
  return { path: base, oid: run(["rev-parse", "HEAD"]).toString().trim() };
}

export function advanceGitFixture(path) {
  writeFileSync(join(path, "lib.rs"), "pub fn next_fixture() -> u32 { 2 }\n");
  const options = {
    cwd: path,
    env: { ...process.env, GIT_CONFIG_GLOBAL: "/dev/null", GIT_CONFIG_SYSTEM: "/dev/null" }
  };
  execFileSync("git", ["add", "lib.rs"], options);
  execFileSync("git", ["commit", "--no-gpg-sign", "-m", "next fixture"], options);
  return execFileSync("git", ["rev-parse", "HEAD"], options).toString().trim();
}

/** Runs the full per-recipe case battery over ONE loaded getAddon(). The addon
 * arrives as a GETTER: test files assign it in `beforeAll`, after the
 * describe body has already registered these cases. */
export function describeCodeBindingCases(getAddon, recipe) {
  it(`${recipe}: deny_by_default_and_authorized_surface`, () => {
    const base = mkdtempSync(join(tmpdir(), `task020-${recipe}-matrix-`));
    const caps = JSON.parse(new (getAddon().NativeProvider)(configJson(base)).capabilitiesJson());
    expect(caps.code_binding).toBe("Supported");
    expect(caps.code_source).toBe("Supported");
    expect(caps.code_reset).toBe("Supported");

    // DENY-BY-DEFAULT through the native provider (r6 finding 1): without a
    // host policy, EVERY code operation denies with the typed envelope.
    mkdirSync(base + "-deny");
    const provider = new (getAddon().NativeProvider)(configJson(base + "-deny"));
    const deniedActions = [
      () =>
        provider.codeBindJson(
          JSON.stringify({ scope, repo: "repo-matrix", branchRef: "refs/heads/main" })
        ),
      () =>
        provider.codeRegisterJson(
          JSON.stringify({ scope, repo: "repo-matrix", path: "/somewhere/repo" })
        ),
      () => provider.codeStatusJson(JSON.stringify({ scope, repo: "repo-matrix" })),
      () => provider.codeRefreshJson(JSON.stringify({ scope, repo: "repo-matrix" }))
    ];
    for (const action of deniedActions) {
      expect(action).toThrow(/CodeAuthorizationRequired/);
    }

    // AUTHORIZED registration + bind + refresh over a real git fixture (r6
    // findings 1/5) — the SAME shared seam stdio/one-shot use.
    const { path: repoPath, oid: headOid } = gitFixture(`${recipe}-matrix`);
    mkdirSync(base + "-auth");
    const authorized = new (getAddon().NativeProvider)(authorizedConfig(base + "-auth"));
    const registration = JSON.parse(
      authorized.codeRegisterJson(
        JSON.stringify({ scope, repo: "repo-matrix", path: repoPath, mode: "local-only" })
      )
    );
    expect(registration.repo_id).toBe("repo-matrix");
    expect(registration.locator_id).toBe(repoPath);
    expect(registration.acquisition).toBe("local_only");

    const providerBinding = JSON.parse(
      authorized.codeBindJson(
        JSON.stringify({ scope, repo: "repo-matrix", branchRef: "refs/heads/main" })
      )
    );
    expect(providerBinding.branchRef).toBe("refs/heads/main");
    expect(providerBinding.version).toBe(1);
    expect(providerBinding.activeGeneration).toBeNull();

    const observed = JSON.parse(
      authorized.codeRefreshJson(JSON.stringify({ scope, repo: "repo-matrix" }))
    );
    expect(observed.observedOid).toBe(headOid);
    expect(observed.policy).toBe("on_request");

    const after = JSON.parse(
      authorized.codeStatusJson(JSON.stringify({ scope, repo: "repo-matrix" }))
    );
    expect(after.freshness.observedOid).toBe(headOid);
    expect(after.freshness.state).toEqual({ observed: { matchesIndexed: null } });
    expect(after.checkpoint.repoId).toBe("repo-matrix");
    // Foreign scope isolation (the host policy allows only the launch scope).
    expect(() =>
      authorized.codeStatusJson(JSON.stringify({ scope: foreignScope, repo: "repo-matrix" }))
    ).toThrow(/CodeAuthorizationRequired/);
  });

  it(`${recipe}: bound_ingestion_pipeline`, () => {
    const { path: repoPath, oid: headOid } = gitFixture(`${recipe}-pipeline`);
    const base = mkdtempSync(join(tmpdir(), `task020-${recipe}-pipeline-`));
    const provider = new (getAddon().NativeProvider)(authorizedConfig(base));
    const repo = `repo-${recipe}-pipeline`;

    provider.codeRegisterJson(JSON.stringify({ scope, repo, path: repoPath }));
    provider.codeBindJson(JSON.stringify({ scope, repo, branchRef: "refs/heads/main" }));

    // Supplying a path alongside repo is a typed rejection.
    expect(() =>
      provider.scanRepositoryJson(
        JSON.stringify({ path: repoPath, scope, repo, branchRef: "refs/heads/main" })
      )
    ).toThrow(/path is not an input/);

    const ingested = JSON.parse(provider.scanRepositoryJson(JSON.stringify({ scope, repo })));
    expect(ingested.mode).toBe("bound");
    expect(ingested.generationId).toMatch(/^gen-/);
    expect(ingested.commitOid).toBe(headOid);
    expect(ingested.snapshotDigest).toHaveLength(64);
    expect(ingested.idempotent).toBe(false);
    expect(ingested.documents).toBeGreaterThan(0);
    expect(ingested.entities).toBeGreaterThan(0);

    const status = JSON.parse(provider.codeStatusJson(JSON.stringify({ scope, repo })));
    expect(status.indexedCommitOid).toBe(headOid);
    expect(status.binding.activeGeneration).toBe(ingested.generationId);

    // DIRTY checkout after the pinned commit: the mutable tree must not
    // change what a re-ingest publishes — same commit is idempotent.
    writeFileSync(join(repoPath, "lib.rs"), "pub fn evil() -> u32 { 2 }\n");
    writeFileSync(join(repoPath, "untracked.rs"), "pub fn rogue() -> u32 { 3 }\n");
    const again = JSON.parse(provider.scanRepositoryJson(JSON.stringify({ scope, repo })));
    expect(again.idempotent).toBe(true);
    expect(again.generationId).toBe(ingested.generationId);
    expect(again.commitOid).toBe(headOid);

    expect(() =>
      provider.scanRepositoryJson(JSON.stringify({ scope, repo, branchRef: "refs/heads/other" }))
    ).toThrow(/BindingConflict/);

    // A POPULATED store reopened without host policy cannot use either the
    // active-generation shortcut or a new commit to acquire or retain bytes.
    const retained = join(base, "store", "retained");
    const beforeDenied = readdirSync(retained).sort();
    const denied = new (getAddon().NativeProvider)(configJson(base));
    expect(() => denied.scanRepositoryJson(JSON.stringify({ scope, repo }))).toThrow(
      /CodeAuthorizationRequired/
    );
    expect(readdirSync(retained).sort()).toEqual(beforeDenied);

    const nextOid = advanceGitFixture(repoPath);
    expect(() => denied.scanRepositoryJson(JSON.stringify({ scope, repo }))).toThrow(
      /CodeAuthorizationRequired/
    );
    expect(readdirSync(retained).sort()).toEqual(beforeDenied);

    // The authorized coordinator can then acquire and retain the new commit.
    const afterDenied = new (getAddon().NativeProvider)(authorizedConfig(base));
    const next = JSON.parse(afterDenied.scanRepositoryJson(JSON.stringify({ scope, repo })));
    expect(next.idempotent).toBe(false);
    expect(next.commitOid).toBe(nextOid);
    expect(readdirSync(retained).sort()).not.toEqual(beforeDenied);
  });

  it(`${recipe}: bound_retrieval_eligible_results`, () => {
    const eligible = gitFixtureWithContent(
      `${recipe}-retrieval-eligible`,
      "pub fn eligible_fixture_record() -> u32 { 1 }\n"
    );
    const ineligible = gitFixtureWithContent(
      `${recipe}-retrieval-ineligible`,
      "pub fn ineligible_fixture_record() -> u32 { 2 }\n"
    );
    const base = mkdtempSync(join(tmpdir(), `task020-${recipe}-retrieval-`));
    const provider = new (getAddon().NativeProvider)(authorizedConfig(base));
    const repo = `repo-${recipe}-retrieval-eligible`;
    provider.codeRegisterJson(JSON.stringify({ scope, repo, path: eligible.path }));
    provider.codeBindJson(JSON.stringify({ scope, repo, branchRef: "refs/heads/main" }));
    provider.scanRepositoryJson(JSON.stringify({ scope, repo }));
    const foreign = new (getAddon().NativeProvider)(authorizedConfig(base, {}, foreignScope));
    const foreignRepo = `repo-${recipe}-retrieval-ineligible`;
    foreign.codeRegisterJson(JSON.stringify({ scope: foreignScope, repo: foreignRepo, path: ineligible.path }));
    foreign.codeBindJson(JSON.stringify({ scope: foreignScope, repo: foreignRepo, branchRef: "refs/heads/main" }));
    foreign.scanRepositoryJson(JSON.stringify({ scope: foreignScope, repo: foreignRepo }));

    const recalled = JSON.parse(
      provider.requireRecallApi().recallJson(
        JSON.stringify({
          query: "eligible_fixture_record",
          scope,
          requester: { actor: { id: "task-020", kind: "agent", displayName: "task-020" }, roles: [], permissions: [] },
          modes: ["graph"],
          cues: [],
          includeExplanations: true
        })
      )
    );
    expect(recalled).toHaveProperty("items");
    expect(Array.isArray(recalled.items)).toBe(true);
    expect(recalled.items.length).toBeGreaterThan(0);
    const resultText = JSON.stringify(recalled.items);
    expect(resultText).toContain("eligible_fixture_record");
    expect(resultText).not.toContain("ineligible_fixture_record");
    console.log("bound eligible results", recalled.items.length);
  });

  it(`${recipe}: bound_graph_traversal_answer`, () => {
    const eligible = gitFixtureWithContent(
      `${recipe}-graph-eligible`,
      "pub fn eligible_graph_record() -> u32 { 1 }\n"
    );
    const ineligible = gitFixtureWithContent(
      `${recipe}-graph-ineligible`,
      "pub fn ineligible_graph_record() -> u32 { 2 }\n"
    );
    const base = mkdtempSync(join(tmpdir(), `task020-${recipe}-graph-`));
    const provider = new (getAddon().NativeProvider)(authorizedConfig(base));
    const repo = `repo-${recipe}-graph-eligible`;
    provider.codeRegisterJson(JSON.stringify({ scope, repo, path: eligible.path }));
    provider.codeBindJson(JSON.stringify({ scope, repo, branchRef: "refs/heads/main" }));
    provider.scanRepositoryJson(JSON.stringify({ scope, repo }));
    const foreign = new (getAddon().NativeProvider)(authorizedConfig(base, {}, foreignScope));
    const foreignRepo = `repo-${recipe}-graph-ineligible`;
    foreign.codeRegisterJson(JSON.stringify({ scope: foreignScope, repo: foreignRepo, path: ineligible.path }));
    foreign.codeBindJson(JSON.stringify({ scope: foreignScope, repo: foreignRepo, branchRef: "refs/heads/main" }));
    foreign.scanRepositoryJson(JSON.stringify({ scope: foreignScope, repo: foreignRepo }));

    const answer = JSON.parse(
      provider.exploreJson(JSON.stringify({ scope, query: "eligible_graph_record", includeCodeArtifacts: true }))
    );
    expect(Array.isArray(answer)).toBe(true);
    expect(answer.length).toBeGreaterThan(0);
    const answerText = JSON.stringify(answer);
    expect(answerText).toContain("eligible_graph_record");
    expect(answerText).not.toContain("ineligible_graph_record");
    console.log("bound traversal answer", answer.length);
  });

  it(`${recipe}: bound_reset_preview_apply_outcome`, () => {
    const { path: repoPath } = gitFixture(`${recipe}-reset`);
    const base = mkdtempSync(join(tmpdir(), `task020-${recipe}-reset-`));
    const provider = new (getAddon().NativeProvider)(authorizedConfig(base));
    const repo = `repo-${recipe}-reset`;
    provider.codeRegisterJson(JSON.stringify({ scope, repo, path: repoPath }));
    provider.codeBindJson(JSON.stringify({ scope, repo, branchRef: "refs/heads/main" }));
    provider.scanRepositoryJson(JSON.stringify({ scope, repo }));

    const preview = JSON.parse(provider.codeResetPreviewJson(JSON.stringify({ scope, repo })));
    expect(preview.previewHash).toHaveLength(64);
    const journal = {
      resetId: `native-reset-${recipe}`,
      preview,
      previewHash: preview.previewHash,
      targetFingerprints: preview.targets.map((target) => [target.recordId, target.fingerprint]),
      authorizingDecisions: ["task-020 native reset"],
      checkpoints: [],
      preservationDigests: preview.preservationDigests
    };
    const applied = JSON.parse(
      provider.codeResetApplyJson(JSON.stringify({ journal, decision: "task-020 native reset" }))
    );
    expect(applied.previewHash).toBe(preview.previewHash);
    expect(applied.checkpoints).toEqual(expect.arrayContaining([expect.stringMatching(/done$/)]));
    console.log("preview/apply outcome", applied.resetId);
  });

  it(`${recipe}: bound_embedding_backfill_outcome`, () => {
    const { path: repoPath } = gitFixture(`${recipe}-backfill`);
    const base = mkdtempSync(join(tmpdir(), `task020-${recipe}-backfill-`));
    const provider = new (getAddon().NativeProvider)(authorizedConfig(base));
    const repo = `repo-${recipe}-backfill`;
    provider.codeRegisterJson(JSON.stringify({ scope, repo, path: repoPath }));
    provider.codeBindJson(JSON.stringify({ scope, repo, branchRef: "refs/heads/main" }));
    const before = provider.codeStatusJson(JSON.stringify({ scope, repo }));

    expect(() => provider.backfillEmbeddingsJson(JSON.stringify(scope))).toThrow(
      /no embedding provider wired/
    );
    expect(provider.codeStatusJson(JSON.stringify({ scope, repo }))).toBe(before);
    console.log("backfill outcome binding unchanged");
  });

  it(`${recipe}: bound_source_fetch_exact_bytes`, () => {
    const { path: repoPath } = gitFixture(`${recipe}-source`);
    const base = mkdtempSync(join(tmpdir(), `task020-${recipe}-source-`));
    const provider = new (getAddon().NativeProvider)(authorizedConfig(base));
    const repo = `repo-${recipe}-source`;
    provider.codeRegisterJson(JSON.stringify({ scope, repo, path: repoPath }));
    provider.codeBindJson(JSON.stringify({ scope, repo, branchRef: "refs/heads/main" }));
    provider.scanRepositoryJson(JSON.stringify({ scope, repo }));

    const fetched = JSON.parse(provider.codeFetchJson(JSON.stringify({ scope, path: "lib.rs" })));
    expect(fetched.repoId).toBe(repo);
    expect(fetched.generationId).toMatch(/^gen-/);
    expect(Buffer.from(fetched.bytes)).toEqual(Buffer.from("pub fn fixture() -> u32 { 1 }\n"));
    expect(fetched.servedDigest).toHaveLength(64);
    expect(fetched.participants).not.toHaveLength(0);
  });

  it(`${recipe}: legacy_lanes_never_ingest_code`, () => {
    const base = mkdtempSync(join(tmpdir(), `task020-${recipe}-legacy-`));
    const provider = new (getAddon().NativeProvider)(authorizedConfig(base));
    const repoDir = join(base, "repo");
    mkdirSync(repoDir, { recursive: true });
    writeFileSync(join(repoDir, "lib.rs"), "pub fn a() -> u32 { 1 }\n");
    writeFileSync(join(repoDir, "notes.md"), "# Notes\n");

    // Provider surface: neither lane selected → typed guidance.
    expect(() => provider.scanRepositoryJson(JSON.stringify({ scope }))).toThrow(
      /BindingRequired/
    );
    // Code intent without a registration → the pipeline's typed denial.
    expect(() =>
      provider.scanRepositoryJson(JSON.stringify({ scope, repo: "repo-legacy" }))
    ).toThrow(/no registered repository/);
    // The document-only lane over `path` never ingests code files.
    const docs = JSON.parse(
      provider.scanRepositoryJson(JSON.stringify({ path: repoDir, scope }))
    );
    expect(docs.mode).toBe("legacy-document-only");

    // Legacy engine: code intent is DENIED (with or without a binding); the
    // explicit document-only lane runs without one.
    const engine = new (getAddon().NativeIngestEngine)();
    expect(() =>
      engine.startScanJobJson(JSON.stringify({ root: repoDir, scope, repo: "repo-legacy" }))
    ).toThrow(/LegacyCodeScanDenied/);
    expect(() =>
      engine.startScanJobJson(JSON.stringify({ root: repoDir, scope }))
    ).toThrow(/BindingRequired/);
    // Legacy binding is unsupported because this engine has no host-policy
    // configuration seam. It must not mutate the durable binding cell.
    expect(() =>
      engine.codeBindJson(
        JSON.stringify({ scope, repo: "repo-legacy", branchRef: "refs/heads/main" })
      )
    ).toThrow(/LegacyCodeBindUnsupported/);
    expect(() =>
      engine.startScanJobJson(JSON.stringify({ root: repoDir, scope, repo: "repo-legacy" }))
    ).toThrow(/LegacyCodeScanDenied/);
    const job = JSON.parse(
      engine.startScanJobJson(JSON.stringify({ root: repoDir, scope, documentOnly: true }))
    );
    expect(typeof job.jobId).toBe("string");
  });

  it(`${recipe}: legacy_authority_validation`, () => {
    // Review r6 finding 9 regression: SUPPLIED malformed policy/actor values
    // are typed rejections (InvalidPolicy/InvalidActor) — never silent
    // permissive defaults; ABSENT values keep the documented defaults.
    const engine = new (getAddon().NativeIngestEngine)();
    const root = mkdtempSync(join(tmpdir(), `task020-${recipe}-auth-`));
    writeFileSync(join(root, "notes.md"), "# Notes\n");

    expect(() =>
      engine.startScanJobJson(
        JSON.stringify({
          root,
          scope,
          documentOnly: true,
          policy: { visibility: "not-a-visibility" }
        })
      )
    ).toThrow(/InvalidPolicy/);
    expect(() =>
      engine.startScanJobJson(
        JSON.stringify({
          root,
          scope,
          documentOnly: true,
          actor: { id: 42 }
        })
      )
    ).toThrow(/InvalidActor/);
    // Absent → defaults apply (the job starts; the typed path is proven by
    // the absence of a validation rejection).
    const job = JSON.parse(
      engine.startScanJobJson(JSON.stringify({ root, scope, documentOnly: true }))
    );
    expect(typeof job.jobId).toBe("string");
  });

  it(`${recipe}: direct_retrieval_denied`, () => {
    const engine = new (getAddon().NativeKnowledgeEngine)();
    expect(() => engine.deadCodeJson(JSON.stringify({ scope }))).toThrow(/BindingRequired/);
    expect(() => engine.fileDependenciesJson(JSON.stringify({ scope }))).toThrow(
      /BindingRequired/
    );
    expect(() => engine.blastRadiusJson(JSON.stringify({ scope, target: "a" }))).toThrow(
      /BindingRequired/
    );
  });

  it(`${recipe}: generated_type_shapes`, () => {
    // Compile-time literals live in the .ts files; the runtime proof here is
    // the authorized bind result satisfying the generated CodeBinding shape.
    const base = mkdtempSync(join(tmpdir(), `task020-${recipe}-shapes-`));
    const provider = new (getAddon().NativeProvider)(authorizedConfig(base));
    const runtime = JSON.parse(
      provider.codeBindJson(
        JSON.stringify({
          scope: { tenant: "task-020", workspace: "bound" },
          repo: `repo-${recipe}-shapes`,
          branchRef: "refs/heads/main"
        })
      )
    );
    expect(runtime.scope.tenant).toBe("task-020");
    expect(runtime.repoId).toBe(`repo-${recipe}-shapes`);
    expect(runtime.branchRef).toBe("refs/heads/main");
    expect(runtime.version).toBe(1);
    expect(runtime.activeGeneration).toBeNull();
    expect(runtime.epoch).toBe(0);
  });
}
