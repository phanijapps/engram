import { useCallback, useEffect, useState } from "react";
import {
  fetchPartitionEntities,
  fetchPartitionMemories,
  type EntityView,
  type MemoryRecordView,
  type PartitionRecords
} from "../api";
import { dimensionLabel } from "./Inventory";
import { RecentAdditionsView } from "./RecentAdditions";

/** Header for the selected partition: the exact tuple with "(unset)" labels. */
export function PartitionHeader({ scope }: { scope: PartitionTuple }) {
  return (
    <div className="mb-4">
      <h2 className="text-base font-semibold">Partition</h2>
      <p className="font-mono text-xs text-slate-300">
        tenant {scope.tenant} · subject {dimensionLabel(scope.subject ?? null)} · workspace{" "}
        {dimensionLabel(scope.workspace ?? null)} · session {dimensionLabel(scope.session ?? null)} ·
        environment {dimensionLabel(scope.environment ?? null)}
      </p>
      <p className="mt-1 text-xs text-slate-500">
        Every read on this page is engine-scoped to exactly this tuple — isolation is the engine's,
        not the console's (REQ-003).
      </p>
    </div>
  );
}

export type PartitionTuple = {
  tenant: string;
  subject?: string | null;
  workspace?: string | null;
  session?: string | null;
  environment?: string | null;
};

const PAGE_SIZE = 25;

/**
 * Isolated partition browsing (STEP-009 / REQ-003): one partition, engine-
 * scoped reads only, paginated listings. Selection happens upstream
 * (InventoryView); this view never widens or filters client-side.
 */
export function PartitionView({
  scope,
  onBack
}: {
  scope: PartitionTuple;
  onBack: () => void;
}) {
  const [memories, setMemories] = useState<PartitionRecords<MemoryRecordView> | null>(null);
  const [entities, setEntities] = useState<PartitionRecords<EntityView> | null>(null);
  const [memoryOffset, setMemoryOffset] = useState(0);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setError(null);
    Promise.all([
      fetchPartitionMemories(scope, PAGE_SIZE, memoryOffset),
      fetchPartitionEntities(scope, PAGE_SIZE, 0)
    ])
      .then(([memoryPage, entityPage]) => {
        if (cancelled) return;
        setMemories(memoryPage);
        setEntities(entityPage);
      })
      .catch((cause: unknown) => {
        if (!cancelled) setError(String(cause));
      });
    return () => {
      cancelled = true;
    };
  }, [scope, memoryOffset]);

  const hasNext = memories?.nextOffset != null;
  const hasPrev = memoryOffset > 0;
  const next = useCallback(() => {
    if (memories?.nextOffset != null) setMemoryOffset(memories.nextOffset);
  }, [memories]);
  const prev = useCallback(() => {
    setMemoryOffset(Math.max(0, memoryOffset - PAGE_SIZE));
  }, [memoryOffset]);

  if (error) {
    return (
      <section className="rounded border border-amber-700/50 bg-amber-950/30 p-4">
        <h2 className="font-semibold text-amber-300">State: engine-error</h2>
        <p className="text-sm text-amber-200/80">{error}</p>
        <button className="mt-2 text-sm underline" onClick={onBack}>
          back to inventory
        </button>
      </section>
    );
  }

  return (
    <section>
      <button className="mb-3 text-sm text-slate-400 underline" onClick={onBack}>
        ← inventory
      </button>
      <PartitionHeader scope={scope} />
      <div className="grid gap-6 lg:grid-cols-3">
        <div>
          <h3 className="mb-1 text-sm font-semibold uppercase tracking-wide text-slate-400">
            memories
          </h3>
          {memories === null ? (
            <p className="text-slate-500">loading…</p>
          ) : memories.items.length === 0 ? (
            <p className="text-slate-500">
              no memories in this partition (engine-scoped read returned none)
            </p>
          ) : (
            <ul className="space-y-1">
              {memories.items.map((memory) => (
                <li key={memory.id} className="rounded bg-slate-900/70 px-3 py-1.5 text-sm">
                  <span className="font-mono text-xs text-slate-500">{memory.id}</span>
                  <p className="text-slate-200">{memory.content.text}</p>
                </li>
              ))}
            </ul>
          )}
          <div className="mt-2 flex gap-2 text-sm">
            <button
              className="rounded bg-slate-800 px-2 py-1 disabled:opacity-40"
              disabled={!hasPrev}
              onClick={prev}
            >
              ← prev
            </button>
            <button
              className="rounded bg-slate-800 px-2 py-1 disabled:opacity-40"
              disabled={!hasNext}
              onClick={next}
            >
              next →
            </button>
            <span className="self-center text-xs text-slate-500">
              offset {memoryOffset}
              {memories?.nextOffset != null ? ", more pages" : ", end"}
            </span>
          </div>
        </div>
        <div>
          <h3 className="mb-1 text-sm font-semibold uppercase tracking-wide text-slate-400">
            recent additions
          </h3>
          <RecentAdditionsView scope={scope} />
        </div>
        <div>
          <h3 className="mb-1 text-sm font-semibold uppercase tracking-wide text-slate-400">
            knowledge entities
          </h3>
          {entities === null ? (
            <p className="text-slate-500">loading…</p>
          ) : entities.items.length === 0 ? (
            <p className="text-slate-500">no entities in this partition</p>
          ) : (
            <ul className="space-y-1">
              {entities.items.map((entity) => (
                <li key={entity.id} className="rounded bg-slate-900/70 px-3 py-1.5 text-sm">
                  <span className="font-mono text-xs text-slate-500">{entity.id}</span>{" "}
                  <span className="text-slate-200">{entity.name}</span>{" "}
                  <span className="text-xs text-slate-500">({entity.kind})</span>
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>
    </section>
  );
}
