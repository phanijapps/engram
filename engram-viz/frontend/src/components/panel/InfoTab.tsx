//! INFO tab — kind, file, line range, community, callers/callees,
//! and blast-radius (transitive callers) trigger.

import { useCallback, useState } from "react";
import {
  ArrowDownLeft,
  ArrowUpRight,
  Crosshair,
  Loader2,
} from "lucide-react";
import { api } from "../../lib/api";
import { communityColor } from "../../lib/colors";
import { useGraphStore } from "../../store/graphStore";
import type { Neighbor, NodeDetail } from "../../lib/types";

function NeighborList({
  title,
  icon,
  neighbors,
  onSelect,
}: {
  title: string;
  icon: React.ReactNode;
  neighbors: Neighbor[];
  onSelect: (id: string) => void;
}) {
  return (
    <div>
      <div className="mb-1.5 flex items-center gap-1.5 text-[10px] font-semibold uppercase tracking-wider text-ink-faint">
        {icon}
        {title}
        <span className="ml-auto font-mono text-ink-faint">{neighbors.length}</span>
      </div>
      {neighbors.length === 0 ? (
        <p className="text-[11px] text-ink-faint">none</p>
      ) : (
        <ul className="space-y-0.5">
          {neighbors.slice(0, 30).map((n) => (
            <li key={n.id}>
              <button
                onClick={() => onSelect(n.id)}
                className="block w-full truncate rounded px-1.5 py-1 text-left text-xs text-ink-muted hover:bg-base-750 hover:text-accent"
                title={n.file ?? n.name}
              >
                <span className="font-mono text-[10px] text-accent-cyan">
                  {n.kind?.slice(0, 4)}{" "}
                </span>
                {n.name}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

export function InfoTab({
  detail,
  onSelect,
}: {
  detail: NodeDetail;
  onSelect: (id: string) => void;
}) {
  const e = detail.entity;
  const setHighlight = useGraphStore((s) => s.setHighlight);
  const clearHighlight = useGraphStore((s) => s.clearHighlight);
  const [blastLoading, setBlastLoading] = useState(false);
  const [blastCount, setBlastCount] = useState<number | null>(null);

  const showBlastRadius = useCallback(() => {
    setBlastLoading(true);
    api
      .blastRadius(e.id, 5)
      .then((data) => {
        const ids = new Set(data.callers.map((c) => c.id));
        // Include the target itself so it lights up.
        ids.add(e.id);
        setHighlight(ids, "#d29922");
        setBlastCount(data.callers.length);
      })
      .catch(() => {
        clearHighlight();
        setBlastCount(0);
      })
      .finally(() => setBlastLoading(false));
  }, [e.id, setHighlight, clearHighlight]);

  return (
    <div className="space-y-4 overflow-y-auto p-4">
      {/* Facts grid */}
      <dl className="grid grid-cols-3 gap-px overflow-hidden rounded-md border border-base-700 bg-base-700 text-xs">
        <div className="bg-base-850 px-2.5 py-2">
          <dt className="text-[10px] uppercase text-ink-faint">Kind</dt>
          <dd className="font-mono text-accent-cyan">{e.kind}</dd>
        </div>
        <div className="bg-base-850 px-2.5 py-2">
          <dt className="text-[10px] uppercase text-ink-faint">Community</dt>
          <dd className="flex items-center gap-1.5 font-mono">
            {detail.community !== null && (
              <span
                className="inline-block h-2.5 w-2.5 rounded-full"
                style={{ backgroundColor: communityColor(detail.community ?? undefined) }}
              />
            )}
            <span className="text-ink">{detail.community ?? "—"}</span>
          </dd>
        </div>
        <div className="bg-base-850 px-2.5 py-2">
          <dt className="text-[10px] uppercase text-ink-faint">Complexity</dt>
          <dd className="font-mono text-ink">
            {detail.complexity ?? "—"}
          </dd>
        </div>
      </dl>

      {/* File location */}
      <div>
        <div className="mb-1 text-[10px] font-semibold uppercase tracking-wider text-ink-faint">
          Location
        </div>
        <div className="rounded-md border border-base-700 bg-base-850 px-2.5 py-2 font-mono text-[11px] text-ink-muted">
          {e.file ?? "unknown"}
          {(e.line || e.endLine) && (
            <span className="text-ink-faint">
              {" "}:{" "}
              {e.line}
              {e.endLine && e.endLine !== e.line ? `–${e.endLine}` : ""}
            </span>
          )}
        </div>
      </div>

      {/* Blast radius */}
      <div>
        <button
          onClick={showBlastRadius}
          disabled={blastLoading}
          className="flex w-full items-center gap-2 rounded-md border border-accent-amber/30 bg-accent-amber/5 px-3 py-2 text-left text-xs text-ink-muted hover:border-accent-amber hover:text-accent-amber disabled:opacity-50"
        >
          {blastLoading ? (
            <Loader2 size={13} className="animate-spin" />
          ) : (
            <Crosshair size={13} />
          )}
          <span className="font-semibold uppercase tracking-wider">
            Blast Radius
          </span>
          {blastCount !== null && (
            <span className="ml-auto font-mono text-[10px] text-ink-faint">
              {blastCount} caller{blastCount === 1 ? "" : "s"}
            </span>
          )}
        </button>
        <p className="mt-1 text-[10px] text-ink-faint">
          Highlights all transitive callers of this symbol (up to 5 hops).
        </p>
      </div>

      {/* Callers / callees */}
      <NeighborList
        title="Callers"
        icon={<ArrowDownLeft size={12} className="text-accent-green" />}
        neighbors={detail.callers}
        onSelect={onSelect}
      />
      <NeighborList
        title="Callees"
        icon={<ArrowUpRight size={12} className="text-accent-amber" />}
        neighbors={detail.callees}
        onSelect={onSelect}
      />
    </div>
  );
}
