//! Bottom slide-up drawer: horizontal timeline of symbol introductions.

import { useEffect, useState } from "react";
import { Activity, ChevronDown, ChevronUp, Clock } from "lucide-react";
import { api } from "../../lib/api";
import { useGraphStore } from "../../store/graphStore";
import type { TimelineResponse } from "../../lib/types";

export function TimelineDrawer() {
  const open = useGraphStore((s) => s.timelineOpen);
  const toggle = useGraphStore((s) => s.toggleTimeline);
  const focusNode = useGraphStore((s) => s.focusNode);
  const [data, setData] = useState<TimelineResponse | null>(null);

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    api.timeline().then((d) => {
      if (!cancelled) setData(d);
    });
    return () => {
      cancelled = true;
    };
  }, [open]);

  const maxCount = data ? Math.max(...data.timeline.map((b) => b.count), 1) : 1;

  return (
    <div
      className={`absolute bottom-0 left-0 right-0 z-20 border-t border-base-700 bg-base-900/97 backdrop-blur transition-transform duration-300 ${
        open ? "translate-y-0" : "translate-y-[calc(100%-2.25rem)]"
      }`}
    >
      {/* Header / toggle */}
      <button
        onClick={toggle}
        className="flex h-9 w-full items-center gap-2 px-4 text-xs text-ink-muted hover:text-ink"
      >
        <Activity size={13} className="text-accent" />
        <span className="font-semibold uppercase tracking-wider">Timeline</span>
        {data && (
          <span className="text-ink-faint">
            {data.timeline.length} day{data.timeline.length === 1 ? "" : "s"} ·{" "}
            {data.overview.communityCount} communities
          </span>
        )}
        <span className="ml-auto">
          {open ? <ChevronDown size={15} /> : <ChevronUp size={15} />}
        </span>
      </button>

      {open && (
        <div className="max-h-56 overflow-y-auto px-4 pb-4">
          {/* Histogram */}
          {data && data.timeline.length > 0 && (
            <div className="mb-4 flex h-20 items-end gap-1">
              {data.timeline.map((b) => (
                <div
                  key={b.date}
                  className="group relative flex flex-1 flex-col items-center justify-end"
                  title={`${b.date}: ${b.count} symbols`}
                >
                  <div
                    className="w-full rounded-t bg-gradient-to-t from-accent/40 to-accent"
                    style={{ height: `${(b.count / maxCount) * 100}%`, minHeight: "2px" }}
                  />
                  <span className="mt-1 font-mono text-[9px] text-ink-faint">
                    {b.date.slice(5)}
                  </span>
                </div>
              ))}
            </div>
          )}

          {/* Recent symbols */}
          <div>
            <div className="mb-1.5 flex items-center gap-1.5 text-[10px] font-semibold uppercase tracking-wider text-ink-faint">
              <Clock size={11} />
              Most recently introduced
            </div>
            <div className="flex flex-wrap gap-1.5">
              {(data?.recentSymbols ?? []).map((s) => (
                <button
                  key={s.id}
                  onClick={() => focusNode(s.id)}
                  className="rounded border border-base-700 bg-base-850 px-2 py-1 text-[11px] text-ink-muted hover:border-accent hover:text-accent"
                >
                  <span className="font-mono text-[9px] text-accent-cyan">
                    {s.kind.slice(0, 3)}{" "}
                  </span>
                  {s.name}
                </button>
              ))}
            </div>
            {data && data.timeline.length <= 1 && (
              <p className="mt-3 text-[10px] text-ink-faint">
                Timeline is sparse — this repository was indexed in a single pass,
                so every symbol shares one introduction day. Incremental re-scans
                over time populate a richer timeline.
              </p>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
