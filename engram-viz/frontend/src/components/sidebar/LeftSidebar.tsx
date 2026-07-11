//! Left sidebar: repo selector, node/edge counts, and clickable insights.

import { useEffect, useState } from "react";
import {
  AlertTriangle,
  Boxes,
  Crosshair,
  Link2,
  Network,
  Zap,
} from "lucide-react";
import { api } from "../../lib/api";
import { useGraphStore } from "../../store/graphStore";
import type { InsightsResponse } from "../../lib/types";
import { InsightCard } from "./InsightCard";

export function LeftSidebar() {
  const stats = useGraphStore((s) => s.stats);
  const focusNode = useGraphStore((s) => s.focusNode);
  const [insights, setInsights] = useState<InsightsResponse | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    api
      .insights(10)
      .then((data) => {
        if (!cancelled) setInsights(data);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const repoName = stats?.sources?.[0]?.name
    ? stats.sources[0].name.replace(/^mcp-scan\s*/, "")
    : "codegraph";

  return (
    <aside className="flex h-full w-72 shrink-0 flex-col border-r border-base-700 bg-base-900/95 backdrop-blur">
      {/* Repo selector */}
      <div className="border-b border-base-700 px-4 py-3">
        <label className="mb-1 block text-[10px] font-semibold uppercase tracking-wider text-ink-faint">
          Repository
        </label>
        <div className="flex items-center gap-2 rounded-md border border-base-700 bg-base-850 px-2 py-1.5">
          <Boxes size={14} className="text-accent" />
          <span className="truncate text-xs text-ink" title={repoName}>
            {repoName}
          </span>
        </div>
      </div>

      {/* Counts */}
      <div className="grid grid-cols-2 gap-px border-b border-base-700 bg-base-700">
        <div className="bg-base-900 px-4 py-3">
          <div className="flex items-center gap-1.5 text-ink-faint">
            <Network size={12} />
            <span className="text-[10px] uppercase tracking-wider">Nodes</span>
          </div>
          <div className="mt-0.5 font-mono text-lg text-ink">
            {stats?.nodeCount?.toLocaleString() ?? "—"}
          </div>
        </div>
        <div className="bg-base-900 px-4 py-3">
          <div className="flex items-center gap-1.5 text-ink-faint">
            <Link2 size={12} />
            <span className="text-[10px] uppercase tracking-wider">Edges</span>
          </div>
          <div className="mt-0.5 font-mono text-lg text-ink">
            {stats?.edgeCount?.toLocaleString() ?? "—"}
          </div>
        </div>
      </div>

      {/* Insights */}
      <div className="flex-1 space-y-3 overflow-y-auto p-3">
        <InsightCard
          title="Central symbols"
          icon={<Crosshair size={13} />}
          items={insights?.centralSymbols ?? []}
          loading={loading}
          onSelect={focusNode}
          badge="score"
          accentClass="text-accent"
        />
        <InsightCard
          title="Bridge symbols"
          icon={<Zap size={13} />}
          items={insights?.bridgeSymbols ?? []}
          loading={loading}
          onSelect={focusNode}
          badge="score"
          accentClass="text-accent-amber"
        />
        <InsightCard
          title="Dead code"
          icon={<AlertTriangle size={13} />}
          items={insights?.deadCode ?? []}
          loading={loading}
          onSelect={focusNode}
          badge="category"
          accentClass="text-accent-red"
        />
      </div>
    </aside>
  );
}
