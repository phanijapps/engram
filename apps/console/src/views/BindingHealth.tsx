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

/** Rendered line: one row of facts; stale is informational (edge case spec). */
export function BindingHealthRow({ line }: { line: BindingHealthLine }) {
  if (line.state !== "bound") {
    return (
      <p className="text-xs text-slate-500">
        {line.repo}: <span className="text-slate-400">{line.state}</span>
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
 * engine's code-status surface as a single line of facts. B-7: status only,
 * no code-intelligence browsing.
 */
export function BindingHealthView({
  scope,
  repo,
  onLoad
}: {
  scope: PartitionTuple;
  repo: string;
  onLoad: (scope: PartitionTuple, repo: string) => Promise<BindingHealthLine>;
}) {
  const [line, setLine] = useState<BindingHealthLine | null>(null);

  useEffect(() => {
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
  }, [scope, repo, onLoad]);

  if (line === null) {
    return <p className="text-xs text-slate-500">loading binding health…</p>;
  }
  return <BindingHealthRow line={line} />;
}
