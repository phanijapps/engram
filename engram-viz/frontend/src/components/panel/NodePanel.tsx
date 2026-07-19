//! Right slide-in node detail panel with CODE / INFO / HISTORY tabs.

import { useEffect, useState } from "react";
import { Code2, History, Info, X } from "lucide-react";
import { useGraphStore } from "../../store/graphStore";
import { useNodeDetail } from "../../hooks/useNodeDetail";
import { communityColor } from "../../lib/colors";
import { CodeTab } from "./CodeTab";
import { HistoryTab } from "./HistoryTab";
import { InfoTab } from "./InfoTab";

type Tab = "code" | "info" | "history";

const TABS: { id: Tab; label: string; icon: React.ReactNode }[] = [
  { id: "code", label: "CODE", icon: <Code2 size={13} /> },
  { id: "info", label: "INFO", icon: <Info size={13} /> },
  { id: "history", label: "HISTORY", icon: <History size={13} /> },
];

export function NodePanel() {
  const selectedNodeId = useGraphStore((s) => s.selectedNodeId);
  const selectNode = useGraphStore((s) => s.selectNode);
  const focusNode = useGraphStore((s) => s.focusNode);
  const { detail, loading, error } = useNodeDetail(selectedNodeId);
  const [tab, setTab] = useState<Tab>("info");

  // Default to INFO when a new node opens.
  useEffect(() => {
    if (selectedNodeId) setTab("info");
  }, [selectedNodeId]);

  if (!selectedNodeId) return null;

  return (
    <aside className="absolute bottom-9 right-0 top-12 z-20 flex w-[400px] flex-col border-l border-base-700 bg-base-900/97 backdrop-blur shadow-2xl">
      {/* Header */}
      <div className="flex items-start gap-2 border-b border-base-700 px-4 py-3">
        <span
          className="mt-1 inline-block h-2.5 w-2.5 shrink-0 rounded-full"
          style={{
            backgroundColor: communityColor(detail?.community ?? undefined),
          }}
        />
        <div className="min-w-0 flex-1">
          <h2 className="truncate font-mono text-sm text-ink" title={detail?.entity.name}>
            {detail?.entity.name ?? selectedNodeId}
          </h2>
          <p className="truncate text-[11px] text-ink-faint" title={detail?.entity.file}>
            {detail?.entity.kind && (
              <span className="text-accent-cyan">{detail.entity.kind} </span>
            )}
            {detail?.entity.file}
          </p>
        </div>
        <button
          onClick={() => selectNode(null)}
          className="rounded p-1 text-ink-faint hover:bg-base-750 hover:text-ink"
          aria-label="close panel"
        >
          <X size={16} />
        </button>
      </div>

      {/* Tabs */}
      <div className="flex border-b border-base-700">
        {TABS.map((t) => (
          <button
            key={t.id}
            onClick={() => setTab(t.id)}
            className={`flex flex-1 items-center justify-center gap-1.5 py-2.5 text-[11px] font-semibold tracking-wider transition-colors ${
              tab === t.id
                ? "border-b-2 border-accent text-accent"
                : "text-ink-faint hover:text-ink-muted"
            }`}
          >
            {t.icon}
            {t.label}
          </button>
        ))}
      </div>

      {/* Body */}
      <div className="flex-1 overflow-hidden">
        {loading && (
          <div className="px-4 py-8 text-center text-xs text-ink-faint">Loading…</div>
        )}
        {error && (
          <div className="px-4 py-8 text-center text-xs text-accent-red">{error}</div>
        )}
        {!loading && !error && detail && (
          <>
            {tab === "code" && <CodeTab detail={detail} />}
            {tab === "info" && <InfoTab detail={detail} onSelect={focusNode} />}
            {tab === "history" && <HistoryTab detail={detail} />}
          </>
        )}
      </div>
    </aside>
  );
}
