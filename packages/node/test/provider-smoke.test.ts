/* End-to-end smoke: exercises the REAL built addon (engram_node.node) through the
 * provider facade. Skips when the addon is absent (fresh checkout / CI without
 * `pnpm run build:native`). Proves the two integration facts mocks cannot:
 * scan lands entities in the knowledge store, and consolidation EXECUTION
 * returns a ConsolidationRun — both through the held NativeProvider over napi. */
import { describe, it, expect } from "vitest";
import { existsSync, mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { join } from "node:path";

import { createNativeProviderTransport } from "../src/provider.js";
import {loadNativeBinding} from "../src/binding.js";

const ADDON_PATH = fileURLToPath(new URL("../engram_node.node", import.meta.url));

describe.skipIf(!existsSync(ADDON_PATH))("NativeProviderTransport end-to-end (real addon)", () => {
  it("constructs from config, scans a repo, and runs consolidation execution", async () => {
    const base = mkdtempSync(join(tmpdir(), "engram-smoke-"));
    const store = join(base, "store");
    const repo = join(base, "repo");
    mkdirSync(repo, { recursive: true });
    writeFileSync(join(repo, "fixture.rs"), "pub fn smoke() -> u32 { 42 }\n");

    const configJson = JSON.stringify({
      storage_path: store,
      trusted_root: base,
      scope_policy: "Strict",
      enable_vector: false,
      embedding_provider: {
        provider_type: "none",
        model: "none",
        dimensions: 384,
        prompt_profile: "query"
      },
      migration_mode: "Apply",
      capability_policy: "FailClosed"
    });

    try {
      const transport = createNativeProviderTransport({ configJson });

      // 1. The held provider opens from a config (the keystone) and reports capabilities.
      const caps = (await transport.capabilities()) as { families?: unknown[] };
      expect(caps).toBeDefined();

      // 2. Scan writes entities through the facade + napi boundary.
      const scope = { tenant: "smoke", workspace: "smoke" };
      // The real no-model provider must surface the shared operation's typed
      // unsupported failure, rather than silently reporting an empty backfill.
      await expect(transport.backfillEmbeddings({
        scope,
        requester: {actor: {id: "engram-node", kind: "agent"}}
      })).rejects.toThrow(/unsupported/i);
      const summary = (await transport.scan({ path: repo, scope })) as {
        scanned?: number;
        entities?: number;
      };
      // eslint-disable-next-line no-console
      console.log("[smoke] scan:", JSON.stringify(summary));
      expect(summary.scanned).toBeGreaterThanOrEqual(1);
      expect(summary.entities).toBeGreaterThanOrEqual(1);

      const context={readerVersion:1 as const,scope,requester:{actor:{id:"engram-node",kind:"agent" as const}}};
      const sources=await transport.listCodeSources(context);expect(sources).toHaveLength(1);
      const source=sources[0]!;
      const read=await transport.readCodeSource({repository:source.file.repository,path:"fixture.rs"},context);
      expect(read.text).toBe("pub fn smoke() -> u32 { 42 }\n");
      expect(read.coverage?.some(c=>c.role==="symbol" && c.complete)).toBe(true);
      const second=await transport.scan({path:repo,scope}) as {unchanged:number;ingested:number};
      expect(second.unchanged).toBe(1);expect(second.ingested).toBe(0);
      // 3. Consolidation EXECUTION (non-dry-run) returns a ConsolidationRun.
      const run = (await transport.consolidate({ scope, dryRun: false })) as {
        status?: unknown;
        tasks?: unknown[];
      };
      // eslint-disable-next-line no-console
      console.log("[smoke] consolidate:", JSON.stringify(run));
      expect(run.tasks).toBeInstanceOf(Array);
    } finally {
      rmSync(base, { recursive: true, force: true });
    }
  }, 60_000);

  it("round-trips exact polyglot source through authorized transport APIs", async () => {
    const base = mkdtempSync(join(tmpdir(), "engram-source-transfer-"));
    const sourceStore = join(base, "source-store");
    const targetStore = join(base, "target-store");
    const repo = join(base, "repo");
    mkdirSync(repo, { recursive: true });

    const expected = new Map([
      ["UserService.ts", Buffer.from("\uFEFFimport { fee } from './fee';\r\nexport class UserService {\r\n  getUser(id: string) {\r\n    function nested() {\r\n      if (!id) throw new Error('missing');\r\n      return fee(id);\r\n    }\r\n    return nested();\r\n  }\r\n}\r\n")],
      ["worker.rs", Buffer.from("pub fn outer() -> i32 {\n    fn nested() -> i32 {\n        panic!(\"fee\");\n    }\n    nested()\n}\n")],
      ["empty.py", Buffer.from("")],
    ]);
    for (const [path, text] of expected) writeFileSync(join(repo, path), text);

    const config = (storage_path: string) => JSON.stringify({
      storage_path,
      trusted_root: base,
      scope_policy: "Strict",
      enable_vector: false,
      embedding_provider: {
        provider_type: "none",
        model: "none",
        dimensions: 384,
        prompt_profile: "query"
      },
      migration_mode: "Apply",
      capability_policy: "FailClosed"
    });
    const scope = { tenant: "polyglot", workspace: "roundtrip" };
    const owner = {
      actor: { id: "engram-node", kind: "agent" as const },
      permissions: ["code:source:export", "code:source:write", "code:source:activate"]
    };
    const context = { readerVersion: 1 as const, scope, requester: owner };

    try {
      const source = createNativeProviderTransport({ configJson: config(sourceStore) });
      const target = createNativeProviderTransport({ configJson: config(targetStore) });
      const summary = (await source.scan({ path: repo, scope })) as { scanned?: number };
      expect(summary.scanned).toBeGreaterThanOrEqual(expected.size);

      const sources = await source.listCodeSources(context);
      expect(sources).toHaveLength(expected.size);
      const byPath = new Map(sources.map((record) => [record.file.path, record]));
      for (const [path, bytes] of expected) {
        const record = byPath.get(path);
        expect(record, `source manifest for ${path}`).toBeDefined();
        const read = await source.readCodeSource(
          { repository: record!.file.repository, path },
          context
        );
        expect(Buffer.from(read.text, "utf8")).toEqual(bytes);
        expect(read.manifest.policy).toEqual(record!.policy);
        expect(read.manifest.provenance).toEqual(record!.provenance);
        if (path !== "empty.py") {
          expect(read.coverage?.some((chunk) => chunk.role === "symbol" && chunk.complete)).toBe(true);
        }
      }

      const data = await source.exportAuthorized(context) as Record<string, unknown>;
      const exportedSources = data.code_sources as Array<{ manifest: (typeof sources)[number] }>;
      expect(exportedSources).toHaveLength(sources.length);
      for (const bundle of exportedSources) {
        const original = byPath.get(bundle.manifest.file.path);
        expect(original).toBeDefined();
        expect(bundle.manifest.policy).toEqual(original!.policy);
        expect(bundle.manifest.provenance).toEqual(original!.provenance);
      }
      const withoutExportGrant = {
        ...context,
        requester: {
          actor: owner.actor,
          permissions: ["code:source:write", "code:source:activate"]
        }
      };
      await expect(target.dryRunImportAuthorized(data, withoutExportGrant)).rejects.toThrow();
      const otherScope = { tenant: "other", workspace: "roundtrip" };
      await expect(target.dryRunImportAuthorized(data, { ...context, scope: otherScope })).rejects.toThrow();
      expect(await target.listCodeSources({ ...context, scope: otherScope })).toHaveLength(0);
      expect(await target.listCodeSources(context)).toHaveLength(0);

      const validationReport = await target.dryRunImportAuthorized(data, context);
      await expect(
        target.applyImportAuthorized(
          { validation_report: validationReport, import_data: data },
          withoutExportGrant
        )
      ).rejects.toThrow();
      expect(await target.listCodeSources(context)).toHaveLength(0);
      await target.applyImportAuthorized({ validation_report: validationReport, import_data: data }, context);
      const flat = new (loadNativeBinding().NativeKnowledgeEngine)(join(targetStore,"knowledge.db"));
      const importedSources = await target.listCodeSources(context);
      expect(importedSources).toHaveLength(sources.length);
      for (const [path, bytes] of expected) {
        const original = byPath.get(path)!;
        const imported = await target.readCodeSource(
          { repository: original.file.repository, path },
          context
        );
        expect(Buffer.from(imported.text, "utf8")).toEqual(bytes);
        expect(imported.manifest.policy).toEqual(original.policy);
        expect(imported.manifest.provenance).toEqual(original.provenance);
        const chunks = await target.listChunksByDocument(
          imported.manifest.documentId!,
          scope
        ) as Array<{ code?: { role?: string } }>;
        expect(chunks.every((chunk) => chunk.code?.role !== "source_snapshot")).toBe(true);
        if (path !== "empty.py") expect(chunks.length).toBeGreaterThan(0);
        const legacy = JSON.parse(flat.listChunksByDocumentJson(JSON.stringify({documentId:imported.manifest.documentId,scope,readerVersion:0}))) as Array<Record<string,unknown>>;
        expect(legacy).toHaveLength(chunks.length);
        expect(legacy.every(c=>c.code===undefined && c.documentKind===undefined)).toBe(true);
        const snapshots=imported.manifest.snapshotIds;
        expect(snapshots).toBeDefined(); expect(snapshots!.length).toBeGreaterThan(0);
        for (const snapshot of snapshots!) {
          expect(JSON.parse(flat.getChunkJson(JSON.stringify({id:snapshot,scope,readerVersion:0})))).toBeNull();
        }
      }
    } finally {
      rmSync(base, { recursive: true, force: true });
    }
  }, 60_000);

  // Live model QA is explicit: default tests remain usable offline without a
  // cached model. ENGRAM_LIVE_FASTEMBED=1 enables the existing local model proof.
  it.skipIf(process.env.ENGRAM_LIVE_FASTEMBED !== "1")("embeds actual code locally and retrieves it through the native semantic lane", async () => {
    const base = mkdtempSync(join(tmpdir(), "engram-code-vectors-"));
    const repo = join(base, "repo");
    mkdirSync(repo);
    const text = "pub fn reconcile_invoice_total(subtotal: i64, tax: i64) -> i64 { subtotal + tax }\n";
    writeFileSync(join(repo, "billing.rs"), text);
    const scope = {tenant: "native-code", workspace: "billing"};
    const requester = {actor: {id: "engram-node", kind: "agent" as const}};
    try {
      const transport = createNativeProviderTransport({configJson: JSON.stringify({
        storage_path: join(base, "store"), trusted_root: base, scope_policy: "Strict",
        embedding_provider: {provider_type:"fastembed",model:"BAAI/bge-small-en-v1.5",dimensions:384,prompt_profile:"query"},
        enable_vector: true, migration_mode:"Apply", capability_policy:"FailClosed"
      })});
      expect(await transport.capabilities()).toMatchObject({code_embeddings:"Supported"});
      await transport.scan({path: repo, scope});
      const report = await transport.backfillEmbeddings({scope, requester, limit:64});
      expect(report.codeEmbedded).toBeGreaterThan(0);
      expect(report.embedded).toBeGreaterThan(0);
      expect(report.remaining).toBe(0);
      const again = await transport.backfillEmbeddings({scope, requester, limit:64});
      expect(again).toMatchObject({embedded:0,newEmbeddings:0,reused:0,codeEmbedded:0,remaining:0});
      const recall = await transport.recall({query:"calculate an invoice total with tax",scope,requester,modes:["semantic"],limit:5});
      expect(recall.sourceFailures ?? []).toHaveLength(0);
      expect(recall.items.some(item => item.content.includes("reconcile_invoice_total") && item.fusionTrace?.source.split("+").includes("vector"))).toBe(true);
      console.log("[code-vectors] native:", JSON.stringify({report,vectorHits:recall.items.filter(item => item.fusionTrace?.source.split("+").includes("vector")).map(item => item.targetId)}));
      const context = {readerVersion:1 as const,scope,requester};
      const manifests = await transport.listCodeSources(context);
      expect(manifests).toHaveLength(1);
      const source = await transport.readCodeSource({repository:manifests[0]!.file.repository,path:"billing.rs"},context);
      expect(source.text).toBe(text);
    } finally {
      rmSync(base, {recursive:true,force:true});
    }
  }, 60_000);

});
