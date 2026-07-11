//! engram-viz — single-page code-graph workspace.
//!
//! The graph is the hero (full-viewport, always visible). Overlay panels slide
//! in: a left sidebar (repo + insights), a top search bar, a right node-detail
//! panel, and a bottom timeline drawer.

import { Activity, AlertCircle, Loader2, Workflow as GraphIcon } from "lucide-react";
import { GraphCanvas } from "./components/graph/GraphCanvas";
import { LeftSidebar } from "./components/sidebar/LeftSidebar";
import { NodePanel } from "./components/panel/NodePanel";
import { SearchBar } from "./components/search/SearchBar";
import { TimelineDrawer } from "./components/timeline/TimelineDrawer";
import { useGraphData } from "./hooks/useGraphData";
import { useGraphStore } from "./store/graphStore";

export default function App() {
  useGraphData();
  const loading = useGraphStore((s) => s.loading);
  const error = useGraphStore((s) => s.error);
  const nodeCount = useGraphStore((s) => s.nodes.length);
  const toggleTimeline = useGraphStore((s) => s.toggleTimeline);

  return (
    <div className="relative h-screen w-screen overflow-hidden bg-base-950">
      {/* Hero graph (full viewport) */}
      <GraphCanvas />

      {/* Top bar */}
      <header className="absolute left-0 right-0 top-0 z-30 flex h-12 items-center gap-3 border-b border-base-700 bg-base-900/90 px-4 backdrop-blur">
        <div className="flex items-center gap-2">
          <GraphIcon size={16} className="text-accent" />
          <span className="font-mono text-sm font-semibold text-ink">
            engram<span className="text-accent">-viz</span>
          </span>
        </div>
        <div className="ml-2 hidden items-center gap-1.5 text-[11px] text-ink-faint sm:flex">
          {loading ? (
            <>
              <Loader2 size={11} className="animate-spin" />
              loading graph…
            </>
          ) : (
            !error && (
              <span>
                {nodeCount.toLocaleString()} nodes
              </span>
            )
          )}
        </div>
        <div className="mx-auto">
          <SearchBar />
        </div>
        <button
          onClick={toggleTimeline}
          className="flex items-center gap-1.5 rounded-md border border-base-700 bg-base-850 px-2.5 py-1.5 text-[11px] text-ink-muted hover:border-accent hover:text-accent"
        >
          <Activity size={13} />
          Timeline
        </button>
      </header>

      {/* Error banner */}
      {error && (
        <div className="absolute left-1/2 top-14 z-40 flex -translate-x-1/2 items-center gap-2 rounded-md border border-accent-red/40 bg-accent-red/10 px-4 py-2 text-xs text-accent-red">
          <AlertCircle size={14} />
          {error}
          <span className="text-ink-faint">— is the backend running on :3001?</span>
        </div>
      )}

      {/* Left sidebar (below top bar) */}
      <div className="absolute bottom-9 left-0 top-12 z-10">
        <LeftSidebar />
      </div>

      {/* Right node-detail panel */}
      <NodePanel />

      {/* Bottom timeline drawer */}
      <TimelineDrawer />
    </div>
  );
}
