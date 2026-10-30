/**
 * TASK-020 focused proof (@engram/client): the typed client surface carries
 * the bound code operations with denial parity (BindingRequired /
 * BindingConflict envelopes agree with the native surfaces), fails typed when
 * a transport lacks the surface, and the generated contract type shapes
 * match the client's declared shapes.
 */
import { describe, expect, it } from "vitest";

import {
  createEngramClient,
  EngramCodeBindingUnsupportedError,
  isBindingDenied,
  type CodeBindingStatus,
  type EngramTransport
} from "../src/index.js";
import type { CanonicalSpan, CodeBinding, CodeGeneration, IndexedSource } from "@engram/contracts";

const scope = { tenant: "task-020", workspace: "client" };
const legacyScanResult = {
  mode: "legacy-document-only" as const,
  scanned: 0,
  ingested: 0,
  unchanged: 0,
  skipped: 0,
  entities: 0,
  relationships: 0,
  errors: 0,
  git_remote: null,
  git_branch: null,
  git_sha: null,
  coverage_warnings: []
};

/** A transport whose native side rejects code ops with the Rust envelope. */
const denyingTransport: EngramTransport = {
  writeMemory: () => Promise.reject(new Error("not used")),
  retrieve: () => Promise.reject(new Error("not used")),
  forget: () => Promise.reject(new Error("not used")),
  codeBind: () =>
    Promise.reject(
      new Error(
        "BindingRequired: code scans require a persisted binding; bind the repository first"
      )
    ),
  codeStatus: () => Promise.reject(new Error("BindingRequired")),
  codeRegister: () =>
    Promise.reject(new Error("PolicyDenied: CodeAuthorizationRequired")),
  codeRefresh: () =>
    Promise.reject(new Error("BindingConflict: asserted branch does not match the bound ref")),
  scanCode: () =>
    Promise.reject(
      new Error(
        "BindingRequired: code scans require a persisted binding; bind the repository first"
      )
    )
};

/** A transport with a working code-binding surface (matrix positive path). */
const workingTransport: EngramTransport = {
  writeMemory: () => Promise.reject(new Error("not used")),
  retrieve: () => Promise.reject(new Error("not used")),
  forget: () => Promise.reject(new Error("not used")),
  codeBind: (request) =>
    Promise.resolve({
      scope: request.scope,
      repoId: request.repo,
      branchRef: request.branchRef,
      version: 1,
      activeGeneration: null,
      epoch: 0
    }),
  codeRegister: (request) =>
    Promise.resolve({
      repo_id: request.repo,
      locator_id: request.path,
      acquisition: request.mode === "configured-remote" ? "configured_remote" : "local_only"
    }),
  codeStatus: () =>
    Promise.resolve({
      binding: {
        scope,
        repoId: "repo-client",
        branchRef: "refs/heads/main",
        version: 1,
        activeGeneration: null,
        epoch: 0
      },
      attempt: null,
      codeEpoch: 1,
      indexedCommitOid: null,
      freshness: {
        branchRef: "refs/heads/main",
        policy: "on_request",
        observedOid: null,
        observedAt: null,
        state: "unknown"
      },
      checkpoint: {
        repoId: "repo-client",
        branchRef: "refs/heads/main",
        bindingVersion: 1,
        codeEpoch: 1,
        generation: null
      }
    } satisfies CodeBindingStatus),
  codeRefresh: () =>
    Promise.resolve({
      branchRef: "refs/heads/main",
      observedOid: "c".repeat(40),
      observedAt: "2026-10-30T00:00:00Z",
      policy: "on_request"
    }),
  scanCode: () => Promise.resolve(legacyScanResult)
};

describe("task_020", () => {
  it("backend_surface_matrix", async () => {
    const client = createEngramClient({ transport: workingTransport });
    const code = client.codeBinding();

    // The full operation matrix rides the one typed surface.
    const binding = await code.codeBind({ scope, repo: "repo-client", branchRef: "refs/heads/main" });
    expect(binding.branchRef).toBe("refs/heads/main");
    expect(binding.version).toBe(1);

    const status = await code.codeStatus({ scope, repo: "repo-client" });
    expect(status.freshness.state).toBe("unknown");
    expect(status.freshness.policy).toBe("on_request");
    expect(status.checkpoint.repoId).toBe("repo-client");
    expect(status.indexedCommitOid).toBeNull();

    const registration = await code.codeRegister({
      scope,
      repo: "repo-client",
      path: "/repos/repo-client",
      mode: "local-only"
    });
    expect(registration).toMatchObject({ repo_id: "repo-client" });

    const observed = await code.codeRefresh({ scope, repo: "repo-client" });
    expect(observed.observedOid).toBe("c".repeat(40));
    expect(observed.policy).toBe("on_request");

    const scan = await code.scanCode({ scope, repo: "repo-client" });
    expect(scan).toEqual(legacyScanResult);

    const legacy = await code.scanCode({ scope, path: "/docs" });
    expect(legacy).toEqual(legacyScanResult);

    // @ts-expect-error Bound ingestion and legacy document scans are exclusive.
    code.scanCode({ scope, path: "/repo", repo: "repo-client" });
  });

  it("legacy_unbound_denied", async () => {
    const client = createEngramClient({ transport: denyingTransport });
    const code = client.codeBinding();
    // The client surfaces the SAME typed denials the provider and legacy
    // native paths produce (envelope parity).
    await expect(code.scanCode({ scope, repo: "repo-x" })).rejects.toThrow(
      /BindingRequired/
    );
    await expect(
      code.codeBind({ scope, repo: "repo-x", branchRef: "refs/heads/other" })
    ).rejects.toThrow(/BindingRequired/);
    await expect(code.codeRefresh({ scope, repo: "repo-x" })).rejects.toThrow(/BindingConflict/);
  });

  it("direct_retrieval_denied", async () => {
    // A transport WITHOUT the code surface fails typed — never a silent
    // fallback — and the Rust denial envelopes are recognizable as binding
    // denials across every surface.
    const bare: EngramTransport = {
      writeMemory: () => Promise.reject(new Error("not used")),
      retrieve: () => Promise.reject(new Error("not used")),
      forget: () => Promise.reject(new Error("not used"))
    };
    const client = createEngramClient({ transport: bare });
    expect(() => client.codeBinding()).toThrow(EngramCodeBindingUnsupportedError);

    expect(
      isBindingDenied(
        new Error("BindingRequired: code lane unavailable — no published code generation")
      )
    ).toBe(true);
    expect(isBindingDenied(new Error("BindingConflict: asserted branch mismatch"))).toBe(true);
    expect(isBindingDenied(new Error("some unrelated failure"))).toBe(false);
  });

  it("generated_type_shapes", () => {
    // Compile-time: the generated contract shapes typecheck against literal
    // values (field names, nullability, camelCase) — the same shapes the
    // client runtime types mirror.
    const binding: CodeBinding = {
      scope,
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
    const span: CanonicalSpan = { startLine: 2, endLine: 4 };
    expect(binding.branchRef).toBe("refs/heads/main");
    expect(generation.snapshotDigest).toHaveLength(64);
    expect(source.path).toBe("src/lib.rs");
    expect(span.startLine).toBeLessThanOrEqual(span.endLine);
  });
});
