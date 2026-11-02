import { useEffect, useState } from "react";
import type { PartitionTuple } from "./Partition";

/** The one-line binding health facts (REQ-011): repo, branch, indexed commit,
 * freshness. Stale freshness is a state, not an error. */
export interface BindingHealthLine {
  state: "bound" | "unbound" | string;
  repo: string;
  branchRef?: string | null;
  indexedCommitOid?: string | null;
  freshnessState?: string;
  observedOid?: string | null;
  message?: string;
}

/** localStorage key for the operator's last-used repo id (single operator,
 * single machine — B-1; remembering the repo is convenience, not a secret). */
export const REPO_STORAGE_KEY = "engram-console.binding-repo";

function readStoredRepo(): string {
  try {
    return globalThis.localStorage?.getItem(REPO_STORAGE_KEY) ?? "";
  } catch {
    return "";
  }
}

function storeRepo(repo: string): void {
  try {
    globalThis.localStorage?.setItem(REPO_STORAGE_KEY, repo);
  } catch {
    // Storage is best-effort; the session state still works.
  }
}

/** Rendered line: one row of facts; stale is informational (edge case spec). */
export function BindingHealthRow({ line }: { line: BindingHealthLine }) {
  if (line.state !== "bound") {
    return (
      <p className="text-xs text-slate-400">
        {line.repo}: <span className="text-slate-300">{line.state}</span>
        {line.message ? ` — ${line.message}` : ""}
      </p>
    );
  }
  const stale = line.freshnessState === "ref-moved-past-indexed-commit";
  return (
    <p className="font-mono text-xs text-slate-300">
      {line.repo}: BOUND {line.branchRef} · INDEXED {line.indexedCommitOid?.slice(0, 8) ?? "—"} ·{" "}
      <span className={stale ? "text-amber-300" : "text-emerald-300"}>
        FRESHNESS-STATE: {line.freshnessState}
      </span>
    </p>
  );
}

/**
 * The index-binding health line for one partition (STEP-011 / REQ-011): the
 * engine's code-status surface as a single line of facts. The repo id is an
 * engine key (bindings are per scope+repo — the owner's store binds
 * `mem-alpha`, not "default"), so the operator supplies it; the last-used
 * value is remembered. B-7: status only, no code-intelligence browsing.
 */
export function BindingHealthView({
  scope,
  onLoad,
  initialRepo
}: {
  scope: PartitionTuple;
  onLoad: (scope: PartitionTuple, repo: string) => Promise<BindingHealthLine>;
  initialRepo?: string;
}) {
  const [repo, setRepo] = useState(() => initialRepo ?? readStoredRepo());
  const [line, setLine] = useState<BindingHealthLine | null>(null);
  const [submitted, setSubmitted] = useState(() => repo.length > 0);

  useEffect(() => {
    if (!submitted || repo.length === 0) return;
    let cancelled = false;
    onLoad(scope, repo)
      .then((body) => {
        if (!cancelled) setLine(body);
      })
      .catch((cause: unknown) => {
        if (!cancelled) {
          setLine({ state: "engine-error", repo, message: String(cause) });
        }
      });
    return () => {
      cancelled = true;
    };
  }, [scope, repo, submitted, onLoad]);

  return (
    <div className="mb-4 flex flex-wrap items-center gap-2">
      <label className="text-xs text-slate-400" htmlFor="binding-repo">
        code binding repo:
      </label>
      <input
        id="binding-repo"
        className="w-44 rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono text-xs text-slate-200 focus-visible:outline focus-visible:outline-2 focus-visible:outline-sky-500"
        value={repo}
        placeholder="e.g. mem-alpha"
        onChange={(event) => setRepo(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            storeRepo(repo);
            setSubmitted(true);
          }
        }}
        onBlur={() => {
          if (repo.length > 0) {
            storeRepo(repo);
            setSubmitted(true);
          }
        }}
      />
      {submitted && repo.length > 0 && line === null ? (
        <span className="text-xs text-slate-400">loading binding health…</span>
      ) : null}
      {submitted && repo.length > 0 && line ? <BindingHealthRow line={line} /> : null}
      {!submitted || repo.length === 0 ? (
        <span className="text-xs text-slate-400">
          enter the bound repo id to show its binding health line
        </span>
      ) : null}
    </div>
  );
}
