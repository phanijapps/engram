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

/** Selects the actual backend recipe for every fixture, never the SDK default. */
export function recipeConfig(recipe, base, extra = {}) {
  if (recipe !== "pgvector") return configJson(base, extra);
  const connection = process.env.ENGRAM_TEST_PG_URL;
  if (!connection) throw new Error("ENGRAM_TEST_PG_URL is required for the pgvector recipe");
  return configJson(base, { ...extra, pgvector_connection_string: connection });
}

export function authorizedRecipeConfig(recipe, base, extra = {}, of = scope) {
  return recipeConfig(recipe, base, { ...extra, codeAuthority: { launchScopes: [of] } });
}

/** Reads the registration written by a Pg fixture from the approved Pg database. */
export function assertPgDurableBinding(repo) {
  const container = process.env.PG_FIXTURE_CONTAINER_ID;
  const url = process.env.ENGRAM_TEST_PG_URL;
  if (!container || !url) throw new Error("approved Pg fixture identity is required for durable binding assertion");
  const parsed = new URL(url);
  const result = execFileSync(
    "docker",
    [
      "exec",
      container,
      "psql",
      "-U",
      decodeURIComponent(parsed.username),
      "-d",
      decodeURIComponent(parsed.pathname.slice(1)),
      "-tAc",
      `SELECT repo_id FROM code_repository_registrations WHERE repo_id = '${repo.replaceAll("'", "''")}'`
    ],
    { encoding: "utf8" }
  ).trim();
  expect(result).toBe(repo);
}

/** Makes durable Pg bindings collision-free across cases and matrix invocations. */
export function recipeCase(recipe, operation) {
  const nonce = `${process.pid}-${Date.now()}-${Math.random().toString(16).slice(2)}`;
  return {
    scope: { tenant: "task-020", workspace: `${recipe}-${operation}-${nonce}` },
    foreignScope: { tenant: "task-020", workspace: `${recipe}-${operation}-foreign-${nonce}` },
    repo: `repo-${recipe}-${operation}-${nonce}`
  };
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
    const fixture = recipeCase(recipe, "deny_by_default_and_authorized_surface");
    const caseScope = fixture.scope;
    const caseForeignScope = fixture.foreignScope;
    const base = mkdtempSync(join(tmpdir(), `task020-${recipe}-matrix-`));
    const caps = JSON.parse(new (getAddon().NativeProvider)(recipeConfig(recipe, base)).capabilitiesJson());
    expect(caps.code_binding).toBe("Supported");
    expect(caps.code_source).toBe("Supported");
    expect(caps.code_reset).toBe("Supported");

    // DENY-BY-DEFAULT through the native provider (r6 finding 1): without a
    // host policy, EVERY code operation denies with the typed envelope.
    mkdirSync(base + "-deny");
    const provider = new (getAddon().NativeProvider)(recipeConfig(recipe, base + "-deny"));
    const deniedActions = [
      () =>
        provider.codeBindJson(
          JSON.stringify({ scope: caseScope, repo: fixture.repo, branchRef: "refs/heads/main" })
        ),
      () =>
        provider.codeRegisterJson(
          JSON.stringify({ scope: caseScope, repo: fixture.repo, path: "/somewhere/repo" })
        ),
      () => provider.codeStatusJson(JSON.stringify({ scope: caseScope, repo: fixture.repo })),
      () => provider.codeRefreshJson(JSON.stringify({ scope: caseScope, repo: fixture.repo }))
    ];
    for (const action of deniedActions) {
      expect(action).toThrow(/CodeAuthorizationRequired/);
    }

    // AUTHORIZED registration + bind + refresh over a real git fixture (r6
    // findings 1/5) — the SAME shared seam stdio/one-shot use.
    const { path: repoPath, oid: headOid } = gitFixture(`${recipe}-matrix`);
    mkdirSync(base + "-auth");
    const authorized = new (getAddon().NativeProvider)(authorizedRecipeConfig(recipe, base + "-auth", {}, caseScope));
    const registration = JSON.parse(
      authorized.codeRegisterJson(
        JSON.stringify({ scope: caseScope, repo: fixture.repo, path: repoPath, mode: "local-only" })
      )
    );
    expect(registration.repo_id).toBe(fixture.repo);
    expect(registration.locator_id).toBe(repoPath);
    expect(registration.acquisition).toBe("local_only");

    const providerBinding = JSON.parse(
      authorized.codeBindJson(
        JSON.stringify({ scope: caseScope, repo: fixture.repo, branchRef: "refs/heads/main" })
      )
    );
    expect(providerBinding.branchRef).toBe("refs/heads/main");
    if (recipe === "pgvector") assertPgDurableBinding(fixture.repo);
    expect(providerBinding.version).toBe(1);
    expect(providerBinding.activeGeneration).toBeNull();

    const observed = JSON.parse(
      authorized.codeRefreshJson(JSON.stringify({ scope: caseScope, repo: fixture.repo }))
    );
    expect(observed.observedOid).toBe(headOid);
    expect(observed.policy).toBe("on_request");

    const after = JSON.parse(
      authorized.codeStatusJson(JSON.stringify({ scope: caseScope, repo: fixture.repo }))
    );
    expect(after.freshness.observedOid).toBe(headOid);
    expect(after.freshness.state).toEqual({ observed: { matchesIndexed: null } });
    expect(after.checkpoint.repoId).toBe(fixture.repo);
    // Foreign scope isolation (the host policy allows only the launch scope).
    expect(() =>
      authorized.codeStatusJson(JSON.stringify({ scope: caseForeignScope, repo: fixture.repo }))
    ).toThrow(/CodeAuthorizationRequired/);
  });

  it(`${recipe}: bound_ingestion_pipeline`, () => {
    const fixture = recipeCase(recipe, "bound_ingestion_pipeline");
    const caseScope = fixture.scope;
    const caseForeignScope = fixture.foreignScope;
    const { path: repoPath, oid: headOid } = gitFixture(`${recipe}-pipeline`);
    const base = mkdtempSync(join(tmpdir(), `task020-${recipe}-pipeline-`));
    const provider = new (getAddon().NativeProvider)(authorizedRecipeConfig(recipe, base, {}, caseScope));
    const repo = fixture.repo;

    provider.codeRegisterJson(JSON.stringify({ scope: caseScope, repo, path: repoPath }));
    provider.codeBindJson(JSON.stringify({ scope: caseScope, repo, branchRef: "refs/heads/main" }));
    if (recipe === "pgvector") assertPgDurableBinding(repo);

    // Supplying a path alongside repo is a typed rejection.
    expect(() =>
      provider.scanRepositoryJson(
        JSON.stringify({ path: repoPath, scope: caseScope, repo, branchRef: "refs/heads/main" })
      )
    ).toThrow(/path is not an input/);

    const ingested = JSON.parse(provider.scanRepositoryJson(JSON.stringify({ scope: caseScope, repo })));
    expect(ingested.mode).toBe("bound");
    expect(ingested.generationId).toMatch(/^gen-/);
    expect(ingested.commitOid).toBe(headOid);
    expect(ingested.snapshotDigest).toHaveLength(64);
    expect(ingested.idempotent).toBe(false);
    expect(ingested.documents).toBeGreaterThan(0);
    expect(ingested.entities).toBeGreaterThan(0);

    const status = JSON.parse(provider.codeStatusJson(JSON.stringify({ scope: caseScope, repo })));
    expect(status.indexedCommitOid).toBe(headOid);
    expect(status.binding.activeGeneration).toBe(ingested.generationId);

    // DIRTY checkout after the pinned commit: the mutable tree must not
    // change what a re-ingest publishes — same commit is idempotent.
    writeFileSync(join(repoPath, "lib.rs"), "pub fn evil() -> u32 { 2 }\n");
    writeFileSync(join(repoPath, "untracked.rs"), "pub fn rogue() -> u32 { 3 }\n");
    const again = JSON.parse(provider.scanRepositoryJson(JSON.stringify({ scope: caseScope, repo })));
    expect(again.idempotent).toBe(true);
    expect(again.generationId).toBe(ingested.generationId);
    expect(again.commitOid).toBe(headOid);

    expect(() =>
      provider.scanRepositoryJson(JSON.stringify({ scope: caseScope, repo, branchRef: "refs/heads/other" }))
    ).toThrow(/BindingConflict/);

    // A POPULATED store reopened without host policy cannot use either the
    // active-generation shortcut or a new commit to acquire or retain bytes.
    const retained = join(base, "store", "retained");
    const beforeDenied = readdirSync(retained).sort();
    const denied = new (getAddon().NativeProvider)(recipeConfig(recipe, base));
    expect(() => denied.scanRepositoryJson(JSON.stringify({ scope: caseScope, repo }))).toThrow(
      /CodeAuthorizationRequired/
    );
    expect(readdirSync(retained).sort()).toEqual(beforeDenied);

    const nextOid = advanceGitFixture(repoPath);
    expect(() => denied.scanRepositoryJson(JSON.stringify({ scope: caseScope, repo }))).toThrow(
      /CodeAuthorizationRequired/
    );
    expect(readdirSync(retained).sort()).toEqual(beforeDenied);

    // The authorized coordinator can then acquire and retain the new commit.
    const afterDenied = new (getAddon().NativeProvider)(authorizedRecipeConfig(recipe, base, {}, caseScope));
    const next = JSON.parse(afterDenied.scanRepositoryJson(JSON.stringify({ scope: caseScope, repo })));
    expect(next.idempotent).toBe(false);
    expect(next.commitOid).toBe(nextOid);
    expect(readdirSync(retained).sort()).not.toEqual(beforeDenied);
  });

  it(`${recipe}: bound_retrieval_eligible_results`, () => {
    const fixture = recipeCase(recipe, "bound_retrieval_eligible_results");
    const caseScope = fixture.scope;
    const caseForeignScope = fixture.foreignScope;
    const eligible = gitFixtureWithContent(
      `${recipe}-retrieval-eligible`,
      "pub fn eligible_fixture_record() -> u32 { 1 }\n"
    );
    const ineligible = gitFixtureWithContent(
      `${recipe}-retrieval-ineligible`,
      "pub fn ineligible_fixture_record() -> u32 { 2 }\n"
    );
    const base = mkdtempSync(join(tmpdir(), `task020-${recipe}-retrieval-`));
    const provider = new (getAddon().NativeProvider)(authorizedRecipeConfig(recipe, base, {}, caseScope));
    const repo = fixture.repo;
    provider.codeRegisterJson(JSON.stringify({ scope: caseScope, repo, path: eligible.path }));
    provider.codeBindJson(JSON.stringify({ scope: caseScope, repo, branchRef: "refs/heads/main" }));
    if (recipe === "pgvector") assertPgDurableBinding(repo);
    provider.scanRepositoryJson(JSON.stringify({ scope: caseScope, repo }));
    const foreign = new (getAddon().NativeProvider)(authorizedRecipeConfig(recipe, base, {}, caseForeignScope));
    const foreignRepo = `${fixture.repo}-foreign-retrieval`;
    foreign.codeRegisterJson(JSON.stringify({ scope: caseForeignScope, repo: foreignRepo, path: ineligible.path }));
    foreign.codeBindJson(JSON.stringify({ scope: caseForeignScope, repo: foreignRepo, branchRef: "refs/heads/main" }));
    if (recipe === "pgvector") assertPgDurableBinding(foreignRepo);
    foreign.scanRepositoryJson(JSON.stringify({ scope: caseForeignScope, repo: foreignRepo }));

    const recalled = JSON.parse(
      provider.requireRecallApi().recallJson(
        JSON.stringify({
          query: "eligible_fixture_record",
          scope: caseScope,
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
    const fixture = recipeCase(recipe, "bound_graph_traversal_answer");
    const caseScope = fixture.scope;
    const caseForeignScope = fixture.foreignScope;
    const eligible = gitFixtureWithContent(
      `${recipe}-graph-eligible`,
      "pub fn eligible_graph_record() -> u32 { 1 }\n"
    );
    const ineligible = gitFixtureWithContent(
      `${recipe}-graph-ineligible`,
      "pub fn ineligible_graph_record() -> u32 { 2 }\n"
    );
    const base = mkdtempSync(join(tmpdir(), `task020-${recipe}-graph-`));
    const provider = new (getAddon().NativeProvider)(authorizedRecipeConfig(recipe, base, {}, caseScope));
    const repo = fixture.repo;
    provider.codeRegisterJson(JSON.stringify({ scope: caseScope, repo, path: eligible.path }));
    provider.codeBindJson(JSON.stringify({ scope: caseScope, repo, branchRef: "refs/heads/main" }));
    if (recipe === "pgvector") assertPgDurableBinding(repo);
    provider.scanRepositoryJson(JSON.stringify({ scope: caseScope, repo }));
    const foreign = new (getAddon().NativeProvider)(authorizedRecipeConfig(recipe, base, {}, caseForeignScope));
    const foreignRepo = `${fixture.repo}-foreign-graph`;
    foreign.codeRegisterJson(JSON.stringify({ scope: caseForeignScope, repo: foreignRepo, path: ineligible.path }));
    foreign.codeBindJson(JSON.stringify({ scope: caseForeignScope, repo: foreignRepo, branchRef: "refs/heads/main" }));
    if (recipe === "pgvector") assertPgDurableBinding(foreignRepo);
    foreign.scanRepositoryJson(JSON.stringify({ scope: caseForeignScope, repo: foreignRepo }));

    const answer = JSON.parse(
      provider.exploreJson(JSON.stringify({ scope: caseScope, query: "eligible_graph_record", includeCodeArtifacts: true }))
    );
    expect(Array.isArray(answer)).toBe(true);
    expect(answer.length).toBeGreaterThan(0);
    const answerText = JSON.stringify(answer);
    expect(answerText).toContain("eligible_graph_record");
    expect(answerText).not.toContain("ineligible_graph_record");
    console.log("bound traversal answer", answer.length);
  });

  it(`${recipe}: bound_reset_preview_apply_outcome`, () => {
    const fixture = recipeCase(recipe, "bound_reset_preview_apply_outcome");
    const caseScope = fixture.scope;
    const caseForeignScope = fixture.foreignScope;
    const { path: repoPath } = gitFixture(`${recipe}-reset`);
    const base = mkdtempSync(join(tmpdir(), `task020-${recipe}-reset-`));
    const provider = new (getAddon().NativeProvider)(authorizedRecipeConfig(recipe, base, {}, caseScope));
    const repo = fixture.repo;
    provider.codeRegisterJson(JSON.stringify({ scope: caseScope, repo, path: repoPath }));
    provider.codeBindJson(JSON.stringify({ scope: caseScope, repo, branchRef: "refs/heads/main" }));
    if (recipe === "pgvector") assertPgDurableBinding(repo);
    provider.scanRepositoryJson(JSON.stringify({ scope: caseScope, repo }));

    const preview = JSON.parse(provider.codeResetPreviewJson(JSON.stringify({ scope: caseScope, repo })));
    expect(preview.previewHash).toHaveLength(64);
    const journal = {
      resetId: `${fixture.repo}-reset`, 
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
    const fixture = recipeCase(recipe, "bound_embedding_backfill_outcome");
    const caseScope = fixture.scope;
    const caseForeignScope = fixture.foreignScope;
    const { path: repoPath } = gitFixture(`${recipe}-backfill`);
    const base = mkdtempSync(join(tmpdir(), `task020-${recipe}-backfill-`));
    const provider = new (getAddon().NativeProvider)(authorizedRecipeConfig(recipe, base, {}, caseScope));
    const repo = fixture.repo;
    provider.codeRegisterJson(JSON.stringify({ scope: caseScope, repo, path: repoPath }));
    provider.codeBindJson(JSON.stringify({ scope: caseScope, repo, branchRef: "refs/heads/main" }));
    if (recipe === "pgvector") assertPgDurableBinding(repo);
    const before = provider.codeStatusJson(JSON.stringify({ scope: caseScope, repo }));

    expect(() => provider.backfillEmbeddingsJson(JSON.stringify(caseScope))).toThrow(
      /no embedding provider wired/
    );
    expect(provider.codeStatusJson(JSON.stringify({ scope: caseScope, repo }))).toBe(before);
    console.log("backfill outcome binding unchanged");
  });

  it(`${recipe}: bound_source_fetch_exact_bytes`, () => {
    const fixture = recipeCase(recipe, "bound_source_fetch_exact_bytes");
    const caseScope = fixture.scope;
    const caseForeignScope = fixture.foreignScope;
    const { path: repoPath } = gitFixture(`${recipe}-source`);
    const base = mkdtempSync(join(tmpdir(), `task020-${recipe}-source-`));
    const provider = new (getAddon().NativeProvider)(authorizedRecipeConfig(recipe, base, {}, caseScope));
    const repo = fixture.repo;
    provider.codeRegisterJson(JSON.stringify({ scope: caseScope, repo, path: repoPath }));
    provider.codeBindJson(JSON.stringify({ scope: caseScope, repo, branchRef: "refs/heads/main" }));
    if (recipe === "pgvector") assertPgDurableBinding(repo);
    provider.scanRepositoryJson(JSON.stringify({ scope: caseScope, repo }));

    const fetched = JSON.parse(provider.codeFetchJson(JSON.stringify({ scope: caseScope, path: "lib.rs" })));
    expect(fetched.repoId).toBe(repo);
    expect(fetched.generationId).toMatch(/^gen-/);
    expect(Buffer.from(fetched.bytes)).toEqual(Buffer.from("pub fn fixture() -> u32 { 1 }\n"));
    expect(fetched.servedDigest).toHaveLength(64);
    expect(fetched.participants).not.toHaveLength(0);
  });

  it(`${recipe}: legacy_lanes_never_ingest_code`, () => {
    const base = mkdtempSync(join(tmpdir(), `task020-${recipe}-legacy-`));
    const provider = new (getAddon().NativeProvider)(authorizedRecipeConfig(recipe, base));
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
    const provider = new (getAddon().NativeProvider)(authorizedRecipeConfig(recipe, base));
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
