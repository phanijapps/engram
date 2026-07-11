//! Floating EntityKind legend (bottom-right corner). Clicking a kind filters
//! the graph to only that kind.

import { useMemo } from "react";
import { X } from "lucide-react";
import { useGraphStore } from "../../store/graphStore";
import { kindColors } from "../../lib/colors";
import type { GraphNode } from "../../lib/types";

export function KindLegend({ nodes }: { nodes: GraphNode[] }) {
  const kindFilter = useGraphStore((s) => s.kindFilter);
  const setKindFilter = useGraphStore((s) => s.setKindFilter);

  const kinds = useMemo(() => {
    const counts = new Map<string, number>();
    for (const n of nodes) {
      counts.set(n.kind, (counts.get(n.kind) ?? 0) + 1);
    }
    return [...counts.entries()].sort((a, b) => b[1] - a[1]);
  }, [nodes]);

  if (kinds.length === 0) return null;

  return (
    <div className="pointer-events-auto absolute bottom-12 right-3 z-15 w-48 rounded-md border border-base-700 bg-base-850/95 p-3 backdrop-blur">
      <div className="mb-2 flex items-center justify-between">
        <span className="text-[10px] font-semibold uppercase tracking-wider text-ink-faint">
          Entity Kinds
        </span>
        {kindFilter && (
          <button
            onClick={() => setKindFilter(null)}
            className="text-ink-faint hover:text-ink"
            title="Clear filter"
          >
            <X size={11} />
          </button>
        )}
      </div>
      <div className="space-y-0.5">
        {kinds.map(([kind, count]) => {
          const active = kindFilter === kind;
          const color = kindColors[kind] ?? "#6e7681";
          return (
            <button
              key={kind}
              onClick={() => setKindFilter(active ? null : kind)}
              className={`flex w-full items-center gap-2 rounded px-1.5 py-1 text-left ${
                active ? "bg-base-750 ring-1 ring-accent" : "hover:bg-base-750"
              }`}
            >
              <span
                className="inline-block h-2.5 w-2.5 shrink-0 rounded-full"
                style={{ backgroundColor: color }}
              />
              <span className="truncate font-mono text-[10px] text-ink-muted capitalize">
                {kind}
              </span>
              <span className="ml-auto font-mono text-[9px] text-ink-faint">
                {count}
              </span>
            </button>
          );
        })}
      </div>
      {kindFilter && (
        <p className="mt-2 border-t border-base-700 pt-2 text-[9px] text-ink-faint">
          Filtering to <span className="text-accent">{kindFilter}</span> only.
          Click again to clear.
        </p>
      )}
    </div>
  );
}
