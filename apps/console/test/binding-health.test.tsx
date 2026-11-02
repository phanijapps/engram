/**
 * TASK-014 focused proofs (GATE-009, AC-010): the binding health line shows
 * the bound repo, branch, indexed commit, and freshness state from the
 * ENGINE code-status surface — stale freshness renders as a state, never an
 * error (edge case), and an unbound partition renders the named unbound
 * state. The N-API surface (codeStatusJson) already existed; parity is
 * unchanged (second gate leg proves it).
 */
import { describe, expect, it, afterEach } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { BindingHealthRow, BindingHealthView } from "../src/views/BindingHealth";
import { createConsoleServer, partitionKey } from "../server/app.mjs";
import type { Server } from "node:http";

const PARTITION = { tenant: "acme", subject: null, workspace: "mem-alpha", session: null, environment: null };

function provider(statusBody: () => string, failWith?: string) {
  return {
    capabilitiesJson: () => "{}",
    codeStatusJson: () => {
      if (failWith) throw new Error(failWith);
      return statusBody();
    },
    requireScopeInventoryApi: () => {
      throw new Error("the health line must not touch the inventory");
    }
  };
}

const BOUND_STATUS = () =>
  JSON.stringify({
    binding: { repoId: "mem-alpha", branchRef: "refs/heads/main" },
    indexedCommitOid: "bfa4321300000000000000000000000000000000",
    freshness: {
      branchRef: "refs/heads/main",
      policy: "on_request",
      observedOid: "025d127e0000000000000000000000000000000000",
      observedAt: "2026-11-01T00:00:00Z",
      state: { observed: { matchesIndexed: false } }
    },
    checkpoint: { repoId: "mem-alpha", branchRef: "refs/heads/main" }
  });

const servers: Server[] = [];
afterEach(() => {
  while (servers.length > 0) servers.pop()?.close();
});

type HealthProvider = {
  capabilitiesJson(): string;
  codeStatusJson(): string;
};

async function start(provider: HealthProvider) {
  const app = createConsoleServer({ provider, backend: "sqlite", storage: "/tmp/x" });
  const server = await app.listen(0, "127.0.0.1");
  servers.push(server);
  const address = server.address();
  if (typeof address === "string" || address === null) throw new Error("tcp expected");
  return `http://127.0.0.1:${address.port}`;
}

describe("job012 binding health line (STEP-011)", () => {
  it("binding-health: maps the engine status to the line's facts", async () => {
    const base = await start(provider(BOUND_STATUS));
    const body = await (
      await fetch(`${base}/api/partitions/${partitionKey(PARTITION)}/binding-health?repo=mem-alpha`)
    ).json();
    expect(body.state).toBe("bound");
    expect(body.repo).toBe("mem-alpha");
    expect(body.branchRef).toBe("refs/heads/main");
    expect(body.indexedCommitOid).toContain("bfa43213");
    expect(body.freshnessState).toBe("ref-moved-past-indexed-commit");
    expect(body.observedOid).toContain("025d127e");
  });

  it("binding-health: stale freshness renders as a state, never an error", () => {
    const markup = renderToStaticMarkup(
      <BindingHealthRow
        line={{
          state: "bound",
          repo: "mem-alpha",
          branchRef: "refs/heads/main",
          indexedCommitOid: "bfa43213",
          freshnessState: "ref-moved-past-indexed-commit"
        }}
      />
    );
    expect(markup).toContain("mem-alpha: BOUND");
    expect(markup).toContain("refs/heads/main");
    expect(markup).toContain("INDEXED bfa43213");
    expect(markup).toContain("FRESHNESS-STATE: ref-moved-past-indexed-commit");
    expect(markup).not.toContain("error");
  });

  it("binding-health: the engine's REAL unbound denial ('invalid request: BindingRequired') renders the named unbound state (FIND-ui-003)", async () => {
    // The real engine text: CoreError::InvalidRequest { reason: "BindingRequired" }
    // crosses N-API as from_reason("invalid request: BindingRequired") — the
    // round-1 stub fabricated a "NotFound:" shape the engine never emits.
    const base = await start(provider(BOUND_STATUS, "invalid request: BindingRequired"));
    const body = await (
      await fetch(`${base}/api/partitions/${partitionKey(PARTITION)}/binding-health?repo=not-bound-repo`)
    ).json();
    expect(body.state).toBe("unbound");
    expect(body.message).toContain("No code index is bound");
    const markup = renderToStaticMarkup(<BindingHealthRow line={body} />);
    expect(markup).toContain("unbound");
  });

  it("binding-health: the view carries a repo input, not a hardcoded repo (FIND-ui-002)", () => {
    const markup = renderToStaticMarkup(
      <BindingHealthView scope={PARTITION} onLoad={async () => ({ state: "unbound", repo: "x" })} />
    );
    expect(markup).toContain('id="binding-repo"');
    expect(markup).toContain("enter the bound repo id");
    expect(markup).not.toContain('value="default"');
  });

  it("binding-health: missing repo parameter is a bad request", async () => {
    const base = await start(provider(BOUND_STATUS));
    const response = await fetch(`${base}/api/partitions/${partitionKey(PARTITION)}/binding-health`);
    expect(response.status).toBe(400);
  });
});
