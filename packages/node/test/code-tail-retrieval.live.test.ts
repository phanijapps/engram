// Construction proof independent of the frozen relevance evaluation.
import { describe, expect, it } from "vitest";
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { createNativeProviderTransport } from "../src/provider.js";

const addon = fileURLToPath(new URL("../engram_node.node", import.meta.url));

describe.skipIf(!existsSync(addon) || process.env.ENGRAM_LIVE_FASTEMBED !== "1")("actual long-tail code retrieval", () => {
  it("ranks competing cancel and refund tails using only real segment vectors", async () => {
    const base = mkdtempSync(join(tmpdir(), "engram-paired-code-tails-"));
    const repo = join(base, "repo");
    mkdirSync(repo);
    // Neutral headers and a shared body prevent the query from naming a parent.
    const body = Array.from({ length: 650 }, (_, i) => `  const value_${i} = id;\n`).join("");
    const a = `export async function process_alpha(id: string) {\n${body}  return cancel_subscription_and_stop_future_renewals(id);\n}\n`;
    const b = `export async function process_beta(id: string) {\n${body}  return refund_captured_invoice_payment_to_customer(id);\n}\n`;
    expect(body.length).toBeGreaterThan(8192);
    writeFileSync(join(repo, "billing.ts"), a + b);
    const scope = { tenant: "paired-tails", workspace: "billing" };
    const requester = { actor: { id: "engram-node", kind: "agent" as const } };
    const context = { scope, requester, readerVersion: 1 as const };
    try {
      const transport = createNativeProviderTransport({
        configJson: JSON.stringify({
          storage_path: join(base, "store"), trusted_root: base, scope_policy: "Strict",
          enable_vector: true, migration_mode: "Apply", capability_policy: "FailClosed",
          embedding_provider: { provider_type: "fastembed", model: "BAAI/bge-small-en-v1.5", dimensions: 384, prompt_profile: "query" }
        }),
        codeIntelligence: { profile: "fast-v1", retrieval: { enabled_lanes: ["vector"] } }
      });
      const scanned = await transport.scan({ path: repo, scope });
      expect(scanned).toMatchObject({ errors: 0 });
      const indexed = await transport.backfillEmbeddings({ scope, requester, limit: 512 });
      expect(indexed.codeEmbedded).toBeGreaterThan(4);
      expect(indexed.remaining).toBe(0);
      const observed = [];
      for (const [query, name, tail, start] of [
        ["cancel a subscription and stop future renewals", "process_alpha", "cancel_subscription_and_stop_future_renewals", 0],
        ["refund a captured invoice payment to the customer", "process_beta", "refund_captured_invoice_payment_to_customer", a.length]
      ] as const) {
        const packet = await transport.codeEvidence({ query, resultLimit: 2 }, context);
        expect(packet.sourceFailures).toEqual([]);
        expect(packet.diagnostics).toMatchObject({ effectiveReranker: "none", scorerState: "disabled", reindexRequired: false });
        expect(packet.diagnostics.candidateCounts.vector).toBeGreaterThan(0);
        expect(packet.diagnostics.candidateCounts.lexical).toBeUndefined();
        expect(packet.diagnostics.candidateCounts.graph_expansion).toBe(0);
        expect(packet.diagnostics.scoredPairs).toBe(0);
        expect(packet.items[0]?.name).toContain(name);
        const winner = packet.items[0]!;
        expect(winner.excerpts.some(e => e.text.includes(tail) && e.source.startByte > start + 8192)).toBe(true);
        for (const excerpt of winner.excerpts) {
          expect(Buffer.from(a + b).subarray(excerpt.source.startByte, excerpt.source.endByte).toString("utf8")).toBe(excerpt.text);
        }
        // Without a calibration artifact, inspection ranks remain explicitly unvalidated.
        expect(packet.evidenceStatus).toBe("insufficient_evidence");
        observed.push({ query, parent: winner.parentUnitId, name: winner.name,
          winningSegments: winner.segments.map(s => s.segmentId), excerpts: winner.excerpts.map(e => e.source), diagnostics: packet.diagnostics });
      }
      expect(observed[0]!.parent).not.toBe(observed[1]!.parent);
      expect(observed[0]!.winningSegments[0]).not.toBe(observed[1]!.winningSegments[0]);
      console.log("[paired-code-tails]", JSON.stringify({ indexed, observed }));
    } finally {
      rmSync(base, { recursive: true, force: true });
    }
  }, 120_000);
});
