import { useEffect, useState } from "react";
import type { PartitionTuple } from "./Partition";
import { dimensionLabel } from "./Inventory";

/** One surfaced anomaly (REQ-010): a cross-partition duplication signal.
 * Read-only by contract — there is no repair, prune, or enforcement action
 * anywhere in this view (W-001, LED-003). */
export interface Anomaly {
  kind: "subject-partitions" | "entity-identity";
  title: string;
  partitions: PartitionTuple[];
  partitionCount: number;
}

export interface AnomaliesResponse {
  state: string;
  anomalies: Anomaly[];
  message?: string;
}

/** Compact label for one partition tuple (drill-down target). */
export function partitionLabel(scope: PartitionTuple): string {
  return [
    scope.tenant,
    dimensionLabel(scope.subject ?? null),
    dimensionLabel(scope.workspace ?? null),
    dimensionLabel(scope.session ?? null),
    dimensionLabel(scope.environment ?? null)
  ].join("/");
}

export function AnomalyCard({
  anomaly,
  onOpenPartition
}: {
  anomaly: Anomaly;
  onOpenPartition: (scope: PartitionTuple) => void;
}) {
  return (
    <li className="rounded border border-slate-700 bg-slate-900/60 p-3">
      <p className="text-sm text-slate-200">
        <span className="mr-2 rounded bg-slate-800 px-1.5 py-0.5 font-mono text-xs text-slate-300">
          {anomaly.kind}
        </span>
        {anomaly.title}
      </p>
      <p className="mt-1 text-xs text-slate-500">
        affected partitions (click to drill down — view only):
      </p>
      <ul className="mt-1 flex flex-wrap gap-1">
        {anomaly.partitions.map((scope) => (
          <li key={partitionLabel(scope)}>
            <button
              className="rounded bg-slate-800 px-2 py-0.5 font-mono text-xs hover:bg-slate-700"
              onClick={() => onOpenPartition(scope)}
            >
              {partitionLabel(scope)}
            </button>
          </li>
        ))}
      </ul>
    </li>
  );
}

/**
 * The read-only anomaly panel (STEP-011 / REQ-010): surfaces the minimal
 * documented set with drill-down to the affected partitions. No repair,
 * prune, or enforcement action exists in this view — surfacing is the whole
 * v1 contract.
 */
export function AnomaliesView({
  onLoad,
  onOpenPartition
}: {
  onLoad: () => Promise<AnomaliesResponse>;
  onOpenPartition: (scope: PartitionTuple) => void;
}) {
  const [response, setResponse] = useState<AnomaliesResponse | null>(null);

  useEffect(() => {
    let cancelled = false;
    onLoad()
      .then((body) => {
        if (!cancelled) setResponse(body);
      })
      .catch(() => {
        if (!cancelled) setResponse({ state: "engine-error", anomalies: [] });
      });
    return () => {
      cancelled = true;
    };
  }, [onLoad]);

  if (response === null) return <p className="text-slate-500">loading anomalies…</p>;
  return <AnomaliesList response={response} onOpenPartition={onOpenPartition} />;
}

/** Presentational body: a pure function of the response (tested directly). */
export function AnomaliesList({
  response,
  onOpenPartition
}: {
  response: AnomaliesResponse;
  onOpenPartition: (scope: PartitionTuple) => void;
}) {
  if (response.state !== "ready") {
    return (
      <p className="text-sm text-amber-300">
        State: {response.state} — {response.message ?? ""}
      </p>
    );
  }
  if (response.anomalies.length === 0) {
    return (
      <p className="text-slate-400">
        No cross-partition anomalies detected in the minimal documented set (same subject under
        multiple partitions; same entity identity under multiple partitions).
      </p>
    );
  }
  return (
    <section>
      <p className="mb-2 text-sm text-slate-400">
        {response.anomalies.length} cross-partition signal
        {response.anomalies.length === 1 ? "" : "s"} — read-only surfacing, no actions offered
      </p>
      <ul className="space-y-2">
        {response.anomalies.map((anomaly, index) => (
          <AnomalyCard key={`${anomaly.kind}-${index}`} anomaly={anomaly} onOpenPartition={onOpenPartition} />
        ))}
      </ul>
    </section>
  );
}
