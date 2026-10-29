/**
 * TASK-020 focused proof (@engram/node): the native surface exposes bound
 * code operations over the SAME neutral coordinator the recipes use; the
 * provider AND legacy paths agree on binding denial (BindingRequired /
 * BindingConflict); direct code-graph retrieval fails closed without
 * published bound code; and the generated contract type shapes match the
 * runtime serialization.
 *
 * Real artifacts: this file loads BOTH recipe-isolated addons built by the
 * task-owned helper (`./helpers/build-native-recipes.mjs`) — no port doubles.
 */
import { describe, expect, it, beforeAll } from "vitest";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { loadRecipe } from "./helpers/build-native-recipes.mjs";
import type { CanonicalSpan, CodeBinding, CodeGeneration, IndexedSource } from "@engram/contracts";

/** Recipe addons loaded from isolated artifact directories. */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
let sqliteAddon: any;
// eslint-disable-next-line @typescript-eslint/no-explicit-any
let pgvectorAddon: any;

// Long timeout: the helper BUILDS both recipe artifacts on first run (or
// after Rust source changes) — cargo compile time, not test time.
beforeAll(
  () => {
    sqliteAddon = loadRecipe("sqlite");
    pgvectorAddon = loadRecipe("pgvector");
  },
  15 * 60 * 1000
);

function configJson(base: string, extra: Record<string, unknown> = {}): string {
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

const scope = { tenant: "task-020", workspace: "bound" };
const foreignScope = { tenant: "task-020", workspace: "other" };

describe("task_020", () => {
  it("backend_surface_matrix", () => {
    const base = mkdtempSync(join(tmpdir(), "task020-matrix-"));
    // Both recipes expose the SAME code capability states (Supported) and
    // agree with each other on the surface matrix.
    const sqliteProvider = new sqliteAddon.NativeProvider(configJson(base));
    const sqliteCaps = JSON.parse(sqliteProvider.capabilitiesJson());
    expect(sqliteCaps.code_binding).toBe("Supported");
    expect(sqliteCaps.code_source).toBe("Supported");
    expect(sqliteCaps.code_reset).toBe("Supported");

    const pgBase = mkdtempSync(join(tmpdir(), "task020-matrix-pg-"));
    const pgProvider = new pgvectorAddon.NativeProvider(configJson(pgBase));
    const pgCaps = JSON.parse(pgProvider.capabilitiesJson());
    expect(pgCaps.code_binding).toBe("Supported");
    expect(pgCaps.code_source).toBe("Supported");
    expect(pgCaps.code_reset).toBe("Supported");

    // Selected-recipe assertion (behavior, not directory name): the sqlite
    // artifact REFUSES a pgvector config with the exact feature-gate message;
    // the pgvector artifact gets PAST that gate (its failure, if any, is a
    // connection error — never the feature gate).
    const pgConfig = configJson(mkdtempSync(join(tmpdir(), "task020-pgcfg-")), {
      pgvector_connection_string: "postgresql://engram:nobody@127.0.0.1:1/none"
    });
    expect(() => new sqliteAddon.NativeProvider(pgConfig)).toThrow(/requires the `pgvector` feature/);
    let pgvectorGatePassed = false;
    try {
      new pgvectorAddon.NativeProvider(pgConfig);
      pgvectorGatePassed = true; // connected (nothing listening is fine too)
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      expect(message).not.toMatch(/requires the `pgvector` feature/);
      pgvectorGatePassed = true;
    }
    expect(pgvectorGatePassed).toBe(true);

    // Provider and legacy surfaces AGREE on the serialized bind result
    // (normalized branch/version facts).
    const providerBinding = JSON.parse(
      sqliteProvider.codeBindJson(
        JSON.stringify({ scope, repo: "repo-matrix", branchRef: "refs/heads/main" })
      )
    );
    const legacyEngine = new sqliteAddon.NativeIngestEngine();
    const legacyBinding = JSON.parse(
      legacyEngine.codeBindJson(
        JSON.stringify({ scope, repo: "repo-matrix-legacy", branchRef: "refs/heads/main" })
      )
    );
    expect(providerBinding.branchRef).toBe("refs/heads/main");
    expect(legacyBinding.branchRef).toBe("refs/heads/main");
    expect(providerBinding.version).toBe(legacyBinding.version);
    expect(providerBinding.activeGeneration).toBeNull();
    expect(legacyBinding.activeGeneration).toBeNull();

    // Status matrix on the provider surface: freshness starts unknown, the
    // observation is recorded on request, and the checkpoint is RepoId-keyed.
    const before = JSON.parse(
      sqliteProvider.codeStatusJson(JSON.stringify({ scope, repo: "repo-matrix" }))
    );
    expect(before.freshness.state).toBe("unknown");
    expect(before.freshness.observedOid).toBeNull();
    const observed = JSON.parse(
      sqliteProvider.codeObserveRefJson(
        JSON.stringify({ scope, repo: "repo-matrix", observedOid: "b".repeat(40) })
      )
    );
    expect(observed.policy).toBe("on_request");
    expect(observed.observedOid).toBe("b".repeat(40));
    const after = JSON.parse(
      sqliteProvider.codeStatusJson(JSON.stringify({ scope, repo: "repo-matrix" }))
    );
    expect(after.freshness.observedOid).toBe("b".repeat(40));
    expect(after.freshness.state).toEqual({ observed: { matchesIndexed: null } });
    expect(after.checkpoint.repoId).toBe("repo-matrix");
    // Foreign scope isolation on the same surface.
    expect(() =>
      sqliteProvider.codeStatusJson(JSON.stringify({ scope: foreignScope, repo: "repo-matrix" }))
    ).toThrow(/BindingRequired/);
  });

  it("legacy_unbound_denied", () => {
    const base = mkdtempSync(join(tmpdir(), "task020-legacy-"));
    const provider = new sqliteAddon.NativeProvider(configJson(base));
    const repoDir = join(base, "repo");
    mkdirSync(repoDir, { recursive: true });
    writeFileSync(join(repoDir, "lib.rs"), "pub fn a() -> u32 { 1 }\n");

    // Provider surface: an unbound scope denies the code scan.
    expect(() =>
      provider.scanRepositoryJson(
        JSON.stringify({ path: repoDir, scope, repo: "repo-legacy" })
      )
    ).toThrow(/BindingRequired/);

    // Legacy engine: the same denial, before any binding exists.
    const engine = new sqliteAddon.NativeIngestEngine();
    expect(() =>
      engine.startScanJobJson(
        JSON.stringify({
          root: repoDir,
          scope,
          repo: "repo-legacy",
          source_name: "legacy-scan"
        })
      )
    ).toThrow(/BindingRequired/);

    // Wrong-branch assertion is a BindingConflict on the provider surface
    // once a binding exists.
    provider.codeBindJson(
      JSON.stringify({ scope, repo: "repo-legacy", branchRef: "refs/heads/main" })
    );
    expect(() =>
      provider.scanRepositoryJson(
        JSON.stringify({
          path: repoDir,
          scope,
          repo: "repo-legacy",
          branchRef: "refs/heads/other"
        })
      )
    ).toThrow(/BindingConflict/);

    // After binding through the LEGACY engine's own durable cell, the legacy
    // scan is accepted (binding unlocks; ordinary document ingest stays
    // separate).
    engine.codeBindJson(
      JSON.stringify({ scope, repo: "repo-legacy", branchRef: "refs/heads/main" })
    );
    const job = JSON.parse(
      engine.startScanJobJson(
        JSON.stringify({
          root: repoDir,
          scope,
          repo: "repo-legacy",
          sourceName: "legacy-scan"
        })
      )
    );
    expect(typeof job.jobId).toBe("string");
  });

  it("direct_retrieval_denied", () => {
    const engine = new sqliteAddon.NativeKnowledgeEngine();
    // Direct code-graph retrieval fails CLOSED without published bound code:
    // legacy unbound records are never served through this seam.
    expect(() => engine.deadCodeJson(JSON.stringify({ scope }))).toThrow(/BindingRequired/);
    expect(() => engine.fileDependenciesJson(JSON.stringify({ scope }))).toThrow(/BindingRequired/);
    expect(() =>
      engine.blastRadiusJson(JSON.stringify({ scope, target: "a" }))
    ).toThrow(/BindingRequired/);
  });

  it("generated_type_shapes", () => {
    // Compile-time: the runtime serialized shapes satisfy the GENERATED
    // contract types (field names, nullability, camelCase).
    const binding: CodeBinding = {
      scope: { tenant: "task-020", workspace: "bound" },
      repoId: "repo-shapes",
      branchRef: "refs/heads/main",
      version: 1,
      activeGeneration: null,
      epoch: 0
    };
    const generation: CodeGeneration = {
      id: "gen-shapes",
      bindingVersion: 1,
      commitOid: "a".repeat(40),
      snapshotDigest: "b".repeat(64),
      parserFingerprint: "parser",
      filterFingerprint: "filter"
    };
    const source: IndexedSource = {
      repoId: "repo-shapes",
      generationId: "gen-shapes",
      commitOid: "a".repeat(40),
      path: "src/lib.rs",
      contentDigest: "c".repeat(64)
    };
    const span: CanonicalSpan = { startLine: 1, endLine: 3 };
    expect(span.startLine).toBe(1);
    expect(span.endLine).toBe(3);
    expect(source.contentDigest).toHaveLength(64);
    expect(generation.commitOid).toHaveLength(40);

    // Runtime: the native bind result satisfies the generated CodeBinding
    // shape field-for-field.
    const base = mkdtempSync(join(tmpdir(), "task020-shapes-"));
    const provider = new sqliteAddon.NativeProvider(configJson(base));
    const runtime = JSON.parse(
      provider.codeBindJson(
        JSON.stringify({ scope, repo: binding.repoId, branchRef: binding.branchRef })
      )
    ) as CodeBinding;
    expect(runtime.scope.tenant).toBe(binding.scope.tenant);
    expect(runtime.repoId).toBe(binding.repoId);
    expect(runtime.branchRef).toBe(binding.branchRef);
    expect(runtime.version).toBe(binding.version);
    expect(runtime.activeGeneration).toBeNull();
    expect(runtime.epoch).toBe(binding.epoch);
  });
});
