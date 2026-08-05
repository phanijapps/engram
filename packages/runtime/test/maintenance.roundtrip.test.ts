//! Round-trip test: reflectLlm + contradictLlm against the REAL provider (the
//! addon), proving the hand-rolled Belief + Contradiction JSON actually
//! deserializes in Rust + comes back via listBeliefs / listContradictions. The
//! stub-transport tests (maintenance.llm.test.ts) only prove the op logic; this is
//! the only test that catches a wrong serde key. Skips when the build chain isn't
//! ready (mirrors mcp.smoke.test.ts).

import { describe, it, expect } from "vitest";
import { existsSync, mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { createNativeProviderTransport } from "@engram/node";

import { buildWriteMemoryRequest } from "../src/mcp/requests.js";
import { reflectLlm } from "../src/maintenance/reflect.js";
import { contradictLlm } from "../src/maintenance/contradict.js";
import { createLlmProvider } from "../src/maintenance/llm.js";

const ADDON = fileURLToPath(new URL("../../node/engram_node.node", import.meta.url));
const NODE_DIST = fileURLToPath(new URL("../../node/dist/index.js", import.meta.url));
const ready = existsSync(ADDON) && existsSync(NODE_DIST);

const stubLlm = (
  toolCalls: Array<{ name: string; arguments: Record<string, unknown> }>,
) =>
  createLlmProvider({
    provider: "stub",
    model: "stub",
    completeOverride: async () => ({ toolCalls, text: "" }),
  });

function freshProvider(): { transport: ReturnType<typeof createNativeProviderTransport>; scope: { tenant: string; workspace: string } } {
  const store = mkdtempSync(join(tmpdir(), "engram-maintain-rt-"));
  const configJson = JSON.stringify({
    storage_path: join(store, "db"),
    trusted_root: store,
    scope_policy: "Strict",
    embedding_provider: {
      provider_type: "none",
      model: "none",
      dimensions: 384,
      prompt_profile: "query",
    },
    migration_mode: "Apply",
    capability_policy: "FailClosed",
  });
  return {
    transport: createNativeProviderTransport({ configJson }),
    scope: { tenant: "t", workspace: "w" },
  };
}

describe.skipIf(!ready)("maintenance ops round-trip through the REAL provider", () => {
  it("reflectLlm writes a reflection-llm belief that listBeliefs returns", async () => {
    const { transport, scope } = freshProvider();

    await transport.write(
      buildWriteMemoryRequest({ text: "the cat naps by the radiator", scope }),
    );
    await reflectLlm({
      transport,
      scope,
      llm: stubLlm([
        {
          name: "record_belief",
          arguments: { subjectKey: "cat", content: "Cats seek warmth.", confidence: 0.8 },
        },
      ]),
    });

    const beliefs = (await transport.listBeliefs(scope)) as Array<{
      subject?: { key?: string };
      provenance?: { method?: string };
    }>;
    const ours = beliefs.find((b) => b.provenance?.method === "reflection-llm");
    expect(ours, "reflection-llm belief must round-trip through the real provider").toBeTruthy();
    expect(ours!.subject!.key).toBe("cat");
  }, 30000);

  it("contradictLlm writes a contradiction-llm record that listContradictions returns", async () => {
    const { transport, scope } = freshProvider();

    // Two memories → two beliefs (distinct subjects) via reflectLlm.
    await transport.write(buildWriteMemoryRequest({ text: "X is fast", scope }));
    await transport.write(buildWriteMemoryRequest({ text: "X is slow", scope }));
    await reflectLlm({
      transport,
      scope,
      llm: stubLlm([
        { name: "record_belief", arguments: { subjectKey: "x-fast", content: "X is fast" } },
        { name: "record_belief", arguments: { subjectKey: "x-slow", content: "X is slow" } },
      ]),
    });

    const beliefs = (await transport.listBeliefs(scope)) as Array<{ id?: string }>;
    expect(beliefs.length).toBeGreaterThanOrEqual(2);
    const a = beliefs[0]!.id;
    const b = beliefs[1]!.id;
    expect(a && b).toBeTruthy();

    await contradictLlm({
      transport,
      scope,
      llm: stubLlm([
        {
          name: "find_contradiction",
          arguments: { beliefIds: [a, b], kind: "tension", severity: 0.7, reasoning: "fast vs slow" },
        },
      ]),
    });

    const contras = (await transport.listContradictions(scope)) as Array<{
      kind?: string;
      targets?: Array<{ targetId?: string }>;
      provenance?: { method?: string };
    }>;
    const c = contras.find((x) => x.provenance?.method === "contradiction-llm");
    expect(c, "contradiction-llm record must round-trip through the real provider").toBeTruthy();
    expect(c!.kind).toBe("tension");
    expect(c!.targets!.length).toBe(2);
    expect(c!.targets!.map((t) => t.targetId).sort()).toEqual([a, b].sort());
  }, 30000);
});
