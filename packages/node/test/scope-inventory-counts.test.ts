/**
 * TASK-006 focused proof — count truth (GATE-002, AC-002): the port's
 * inventory counts must equal DIRECT store queries per engine, on both
 * recipes. Each recipe leg (child process, one addon per process) seeds a
 * fixture whose partitions differ across subject / workspace / session
 * dimensions — including a fully "(unset)" partition — then reports the
 * engine's counts AND its own direct queries over the governed tables
 * (`node:sqlite` against the sqlite store files; `psql` inside the
 * disposable fixture container for pgvector). This file asserts the two
 * sides are equal per engine; cross-engine equivalence is GATE-005's
 * differential.
 */
import { describe, expect, it } from "vitest";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const LEG = fileURLToPath(new URL("./helpers/scope-inventory-counts-leg.mjs", import.meta.url));

interface CountsByKind {
  memory: Record<string, number>;
  entity: Record<string, number>;
  belief: Record<string, number>;
}

interface LegResult {
  recipe: string;
  tenant: string;
  engine: CountsByKind;
  direct: CountsByKind;
}

function runLeg(recipe: string, nonce: string): LegResult {
  const stdout = execFileSync(process.execPath, [LEG, recipe, nonce], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "inherit"]
  });
  const lines = stdout.trim().split("\n");
  const last = lines[lines.length - 1];
  if (last === undefined) throw new Error(`${recipe} leg produced no output`);
  return JSON.parse(last);
}

function assertCountTruth(result: LegResult): void {
  for (const kind of ["memory", "entity", "belief"] as const) {
    expect(
      result.direct[kind],
      `${result.recipe}: direct store queries must return at least one partition for ${kind}`
    ).toBeDefined();
    expect(result.engine[kind]).toEqual(result.direct[kind]);
  }
  // Stated-once fixture expectations so a double drift cannot pass: the
  // '(unset)' partition carries 1 memory + 1 entity + 2 beliefs, and the
  // session-dimension partition carries 1 memory + 1 entity + 0 beliefs.
  const unsetKey = `${result.tenant}/(unset)/(unset)/(unset)/(unset)`;
  expect(result.engine.memory[unsetKey]).toBe(1);
  expect(result.engine.entity[unsetKey]).toBe(1);
  expect(result.engine.belief[unsetKey]).toBe(2);
  const sessionKey = `${result.tenant}/agent-1/proj-a/s-9/prod`;
  expect(result.engine.memory[sessionKey]).toBe(1);
  expect(result.engine.entity[sessionKey]).toBe(1);
  expect(result.engine.belief[sessionKey] ?? 0).toBe(0);
}

describe.skipIf(!process.env.ENGRAM_TEST_PG_URL || !process.env.PG_FIXTURE_CONTAINER_ID)(
  "job012 scope-inventory count truth (both recipes)",
  () => {
    const nonce = `job012-counts-${process.pid}-${Date.now()}`;

    it(
      "sqlite: inventory counts equal direct store queries",
      { timeout: 30 * 60 * 1000 },
      () => {
        assertCountTruth(runLeg("sqlite", nonce));
      }
    );

    it(
      "pgvector: inventory counts equal direct store queries",
      { timeout: 30 * 60 * 1000 },
      () => {
        assertCountTruth(runLeg("pgvector", nonce));
      }
    );
  }
);
