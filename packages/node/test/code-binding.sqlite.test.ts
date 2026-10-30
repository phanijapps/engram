/**
 * TASK-020 focused proof — SQLITE recipe artifact (review r6 split): one
 * recipe addon per test FILE. Vitest isolates files in separate processes,
 * so the sqlite and pgvector artifacts are never dlopen'd into the same
 * process (two copies of one napi cdylib interpose class symbols and
 * crash). The shared case battery is `./helpers/code-binding-cases.mjs`.
 */
import { describe, expect, it, beforeAll } from "vitest";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { loadRecipe } from "./helpers/build-native-recipes.mjs";
import { configJson, describeCodeBindingCases } from "./helpers/code-binding-cases.mjs";

// eslint-disable-next-line @typescript-eslint/no-explicit-any
let sqliteAddon: any;

beforeAll(
  () => {
    sqliteAddon = loadRecipe("sqlite");
  },
  15 * 60 * 1000
);

describe("task_020 sqlite recipe", () => {
  describeCodeBindingCases(() => sqliteAddon, "sqlite");

  it("sqlite: selected_recipe_feature_gate", () => {
    // Selection honesty: the sqlite artifact REFUSES a pgvector config with
    // the exact feature-gate message.
    const pgConfig = configJson(mkdtempSync(join(tmpdir(), "task020-pgcfg-")), {
      pgvector_connection_string: "postgresql://engram:nobody@127.0.0.1:1/none"
    });
    expect(() => new sqliteAddon.NativeProvider(pgConfig)).toThrow(
      /requires the `pgvector` feature/
    );
  });
});
