//! Floating graph controls overlay: zoom-to-fit, node-count limiter,
//! community filter, path-finder mode toggle, and group-by-kind toggle.

import { useMemo } from "react";
import {
  GitFork,
  Layers,
  Maximize2,
  Eye,
  EyeOff,
  Check,
} from "lucide-react";
import { useGraphStore } from "../../store/graphStore";
import type { GraphNode } from "../../lib/types";

interface Props {
  /** Imperative ref handle to the react-force-graph instance. */
  fgRef: React.MutableRefObject<any>;
  nodes: GraphNode[];
}

export function GraphControls({ fgRef, nodes }: Props) {
  const nodeLimit = useGraphStore((s) => s.nodeLimit);
  const setNodeLimit = useGraphStore((s) => s.setNodeLimit);
  const hiddenCommunities = useGraphStore((s) => s.hiddenCommunities);
  const toggleCommunity = useGraphStore((s) => s.toggleCommunity);
  const pathMode = useGraphStore((s) => s.pathMode);
  const togglePathMode = useGraphStore((s) => s.togglePathMode);
  const groupByKind = useGraphStore((s) => s.groupByKind);
  const toggleGroupByKind = useGraphStore((s) => s.toggleGroupByKind);

  const communityList = useMemo(() => {
    const counts = new Map<number, number>();
    for (const n of nodes) {
      if (n.community !== undefined) {
        counts.set(n.community, (counts.get(n.community) ?? 0) + 1);
      }
    }
    return [...counts.entries()].sort((a, b) => b[1] - a[1]);
  }, [nodes]);

  const zoomToFit = () => {
    fgRef.current?.zoomToFit(400, 40);
  };

  return (
    <div className="pointer-events-auto absolute right-3 top-16 z-15 flex flex-col gap-2">
      {/* Zoom to fit */}
      <button
        onClick={zoomToFit}
        title="Zoom to fit"
        className="flex h-8 w-8 items-center justify-center rounded-md border border-base-700 bg-base-850/90 text-ink-muted backdrop-blur hover:border-accent hover:text-accent"
      >
        <Maximize2 size={14} />
      </button>

      {/* Path mode toggle */}
      <button
        onClick={togglePathMode}
        title="Path finder mode — click two nodes to trace the call path"
        className={`flex h-8 w-8 items-center justify-center rounded-md border backdrop-blur ${
          pathMode
            ? "border-accent-green bg-accent-green/15 text-accent-green"
            : "border-base-700 bg-base-850/90 text-ink-muted hover:border-accent hover:text-accent"
        }`}
      >
        <GitFork size={14} />
      </button>

      {/* Group by kind toggle */}
      <button
        onClick={toggleGroupByKind}
        title="Group nodes by EntityKind"
        className={`flex h-8 w-8 items-center justify-center rounded-md border backdrop-blur ${
          groupByKind
            ? "border-accent-purple bg-accent-purple/15 text-accent-purple"
            : "border-base-700 bg-base-850/90 text-ink-muted hover:border-accent hover:text-accent"
        }`}
      >
        <Layers size={14} />
      </button>

      {/* Node limit + community filter panel */}
      <div className="w-52 rounded-md border border-base-700 bg-base-850/95 p-3 backdrop-blur">
        {/* Node limit */}
        <div className="mb-3">
          <div className="mb-1.5 flex items-center justify-between">
            <span className="text-[10px] font-semibold uppercase tracking-wider text-ink-faint">
              Node limit
            </span>
            <span className="font-mono text-[10px] text-ink-muted">
              {nodeLimit === null ? "all" : `top ${nodeLimit}`}
            </span>
          </div>
          <div className="flex items-center gap-1">
            {[100, 200, 500].map((n) => (
              <button
                key={n}
                onClick={() => setNodeLimit(nodeLimit === n ? null : n)}
                className={`flex-1 rounded border px-1 py-0.5 text-[10px] ${
                  nodeLimit === n
                    ? "border-accent bg-accent/10 text-accent"
                    : "border-base-700 text-ink-faint hover:text-ink-muted"
                }`}
              >
                {n}
              </button>
            ))}
            <button
              onClick={() => setNodeLimit(null)}
              className={`flex-1 rounded border px-1 py-0.5 text-[10px] ${
                nodeLimit === null
                  ? "border-accent bg-accent/10 text-accent"
                  : "border-base-700 text-ink-faint hover:text-ink-muted"
              }`}
            >
              all
            </button>
          </div>
        </div>

        {/* Community filter */}
        <div>
          <div className="mb-1.5 flex items-center justify-between">
            <span className="text-[10px] font-semibold uppercase tracking-wider text-ink-faint">
              Communities
            </span>
            <span className="font-mono text-[10px] text-ink-muted">
              {communityList.length - hiddenCommunities.length}/
              {communityList.length || 0}
            </span>
          </div>
          {communityList.length === 0 ? (
            <p className="text-[10px] text-ink-faint">No communities detected.</p>
          ) : (
            <div className="max-h-40 space-y-0.5 overflow-y-auto">
              {communityList.slice(0, 12).map(([label, count]) => {
                const hidden = hiddenCommunities.includes(label);
                return (
                  <button
                    key={label}
                    onClick={() => toggleCommunity(label)}
                    className="flex w-full items-center gap-1.5 rounded px-1 py-0.5 text-left hover:bg-base-750"
                  >
                    {hidden ? (
                      <EyeOff size={10} className="shrink-0 text-ink-faint" />
                    ) : (
                      <Check size={10} className="shrink-0 text-accent" />
                    )}
                    <span
                      className="inline-block h-2 w-2 shrink-0 rounded-full"
                      style={{
                        backgroundColor: `hsl(${(label * 47) % 360}, 62%, 60%)`,
                      }}
                    />
                    <span className="font-mono text-[10px] text-ink-muted">
                      {label}
                    </span>
                    <span className="ml-auto font-mono text-[9px] text-ink-faint">
                      {count}
                    </span>
                  </button>
                );
              })}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
