//! INFO tab — kind, file, line range, community, and callers/callees.

import { ArrowDownLeft, ArrowUpRight } from "lucide-react";
import { communityColor } from "../../lib/colors";
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
