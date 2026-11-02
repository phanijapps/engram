import { useEffect, useState } from "react";
import { fetchPartitionRecent, type MemoryRecordView } from "../api";
import type { PartitionTuple } from "./Partition";

/** The recent-additions view for one partition (STEP-010 / REQ-008): the
 * newest memories of the selected partition, rendered from engine reads in
 * the engine's newest-first order. No client-side reordering. */
export function RecentAdditionsView({ scope, limit = 20 }: { scope: PartitionTuple; limit?: number }) {
  const [recent, setRecent] = useState<MemoryRecordView[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setError(null);
    fetchPartitionRecent(scope, limit)
      .then((page) => {
        if (!cancelled) setRecent(page.items);
      })
      .catch((cause: unknown) => {
        if (!cancelled) setError(String(cause));
      });
    return () => {
      cancelled = true;
    };
  }, [scope, limit]);

  if (error) {
    return (
      <p className="text-sm text-amber-300">State: engine-error — {error}</p>
    );
  }
  if (recent === null) {
    return <p className="text-slate-400">loading recent additions…</p>;
  }
  if (recent.length === 0) {
    return (
      <p className="text-slate-500">
        no recent additions in this partition — new memories appear here as they are written
      </p>
    );
  }
  return (
    <ol className="space-y-1">
      {recent.map((memory, index) => (
        <li key={memory.id} className="rounded bg-slate-900/70 px-3 py-1.5 text-sm">
          <span className="mr-2 font-mono text-xs text-slate-500">#{index + 1}</span>
          <span className="font-mono text-xs text-slate-500">{memory.id}</span>
          <p className="text-slate-200">{memory.content.text}</p>
          {memory.createdAt ? (
            <p className="text-xs text-slate-500">{memory.createdAt}</p>
          ) : null}
        </li>
      ))}
    </ol>
  );
}
