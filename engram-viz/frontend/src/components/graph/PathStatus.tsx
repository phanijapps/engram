//! Path-mode status bar: shows which endpoints are selected and fetches the
//! dependency path when both are chosen. Clicking a node while pathMode is
//! active is intercepted here via store state.

import { useCallback, useEffect, useState } from "react";
import { GitFork, X } from "lucide-react";
import { useGraphStore } from "../../store/graphStore";
import { api } from "../../lib/api";

export function PathStatus() {
  const pathMode = useGraphStore((s) => s.pathMode);
  const togglePathMode = useGraphStore((s) => s.togglePathMode);
  const selectedNodeId = useGraphStore((s) => s.selectedNodeId);
  const pathFromId = useGraphStore((s) => s.pathFromId);
  const pathToId = useGraphStore((s) => s.pathToId);
  const setPathFrom = useGraphStore((s) => s.setPathFrom);
  const setPathTo = useGraphStore((s) => s.setPathTo);
  const setHighlight = useGraphStore((s) => s.setHighlight);
  const clearHighlight = useGraphStore((s) => s.clearHighlight);
  const resetPath = useGraphStore((s) => s.resetPath);
  const nodes = useGraphStore((s) => s.nodes);

  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const nameById = useCallback(
    (id: string | null) => {
      if (!id) return "—";
      return nodes.find((n) => n.id === id)?.name ?? id;
    },
    [nodes],
  );

  // When a node is clicked in path mode, assign it to from/to in sequence.
  useEffect(() => {
    if (!pathMode || !selectedNodeId) return;
    if (!pathFromId) {
      setPathFrom(selectedNodeId);
    } else if (!pathToId && selectedNodeId !== pathFromId) {
      setPathTo(selectedNodeId);
    }
  }, [pathMode, selectedNodeId, pathFromId, pathToId, setPathFrom, setPathTo]);

  // When both endpoints are set, fetch the dependency path.
  useEffect(() => {
    if (!pathMode || !pathFromId || !pathToId) return;
    let cancelled = false;
    setLoading(true);
    setError(null);
    api
      .path(pathFromId, pathToId)
      .then((data) => {
        if (cancelled) return;
        if (data.path && data.path.length > 0) {
          const ids = new Set(data.path.map((n) => n.id));
          setHighlight(ids, "#58a6ff");
        } else {
          setError("No call path found between these symbols.");
          clearHighlight();
        }
      })
      .catch((err) => {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : String(err));
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [pathMode, pathFromId, pathToId, setHighlight, clearHighlight]);

  if (!pathMode) return null;

  return (
    <div className="absolute left-1/2 top-14 z-30 flex -translate-x-1/2 items-center gap-3 rounded-md border border-accent-green/40 bg-base-900/95 px-4 py-2 backdrop-blur">
      <GitFork size={14} className="text-accent-green" />
      <span className="text-[11px] font-semibold uppercase tracking-wider text-accent-green">
        Path Finder
      </span>
      <div className="flex items-center gap-2 text-[11px] text-ink-muted">
        <span className="max-w-32 truncate rounded border border-base-700 bg-base-850 px-2 py-0.5 font-mono">
          {pathFromId ? nameById(pathFromId) : "click from…"}
        </span>
        <span className="text-ink-faint">→</span>
        <span className="max-w-32 truncate rounded border border-base-700 bg-base-850 px-2 py-0.5 font-mono">
          {pathToId ? nameById(pathToId) : "click to…"}
        </span>
      </div>
      {loading && <span className="text-[10px] text-ink-faint">tracing…</span>}
      {error && <span className="text-[10px] text-accent-amber">{error}</span>}
      <button
        onClick={() => {
          resetPath();
          togglePathMode();
        }}
        className="text-ink-faint hover:text-ink"
        title="Exit path mode"
      >
        <X size={14} />
      </button>
    </div>
  );
}
