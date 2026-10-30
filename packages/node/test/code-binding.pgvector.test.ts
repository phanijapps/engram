/**
 * TASK-020 focused proof — PGVECTOR recipe artifact (review r6 split +
 * finding 8): one recipe addon per test FILE (vitest process isolation), and
 * the REAL disposable-Pg matrix when ENGRAM_TEST_PG_URL is present — the
 * pgvector addon opens through the backends/pgvector recipe against the
 * marked disposable database (run-unique RepoIds; the database persists
 * across runs).
 */
import { describe, expect, it, beforeAll } from "vitest";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { loadRecipe } from "./helpers/build-native-recipes.mjs";
import { configJson, describeCodeBindingCases } from "./helpers/code-binding-cases.mjs";

// eslint-disable-next-line @typescript-eslint/no-explicit-any
let pgvectorAddon: any;

beforeAll(
  () => {
    pgvectorAddon = loadRecipe("pgvector");
  },
  15 * 60 * 1000
);

describe("task_020 pgvector recipe", () => {
  describeCodeBindingCases(() => pgvectorAddon, "pgvector");

  it("pgvector: real_disposable_pg_matrix", () => {
    const url = process.env.ENGRAM_TEST_PG_URL;
    if (!url) {
      console.warn("[task_020] ENGRAM_TEST_PG_URL absent — real-Pg matrix skipped");
      return;
    }
    const base = mkdtempSync(join(tmpdir(), "task020-realpg-"));
    const unique = `${process.pid}-${Date.now()}`;
    const scope = { tenant: "task-020", workspace: "realpg" };
    // No codeAuthority first: the REAL Pg recipe ALSO denies by default.
    const denying = new pgvectorAddon.NativeProvider(
      configJson(base, { pgvector_connection_string: url })
    );
    expect(() =>
      denying.codeBindJson(
        JSON.stringify({ scope, repo: `pg-${unique}`, branchRef: "refs/heads/main" })
      )
    ).toThrow(/CodeAuthorizationRequired/);

    const provider = new pgvectorAddon.NativeProvider(
      configJson(base + "-auth", {
        pgvector_connection_string: url,
        codeAuthority: { launchScopes: [scope] }
      })
    );
    const binding = JSON.parse(
      provider.codeBindJson(
        JSON.stringify({ scope, repo: `pg-${unique}`, branchRef: "refs/heads/main" })
      )
    );
    expect(binding.repoId).toBe(`pg-${unique}`);
    const status = JSON.parse(
      provider.codeStatusJson(JSON.stringify({ scope, repo: `pg-${unique}` }))
    );
    expect(status.checkpoint.repoId).toBe(`pg-${unique}`);
    expect(status.freshness.state).toBe("unknown");
  });
});
