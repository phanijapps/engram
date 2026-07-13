//! Fetches the graph + stats + repo list and populates the store.
//!
//! Stats and the repo list are global (independent of the active repo filter)
//! and are loaded once on mount. The graph is reloaded whenever `sourceFilter`
//! changes so the workspace reflects the selected repository.

import { useEffect } from "react";
import { api } from "../lib/api";
import { useGraphStore } from "../store/graphStore";

export function useGraphData() {
  const setGraph = useGraphStore((s) => s.setGraph);
  const setStats = useGraphStore((s) => s.setStats);
  const setSources = useGraphStore((s) => s.setSources);
  const setCapInfo = useGraphStore((s) => s.setCapInfo);
  const setLoading = useGraphStore((s) => s.setLoading);
  const setError = useGraphStore((s) => s.setError);
  const sourceFilter = useGraphStore((s) => s.sourceFilter);

  // Global aggregates: load once on mount.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const [stats, sources] = await Promise.all([api.stats(), api.sources()]);
        if (cancelled) return;
        setStats(stats);
        setSources(sources.sources);
      } catch {
        // Non-fatal: the graph still loads; the dropdown just stays empty.
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [setStats, setSources]);

  // Graph: reload whenever the repo filter changes.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      setLoading(true);
      setError(null);
      try {
        const graph = await api.graph(sourceFilter);
        if (cancelled) return;
        setGraph(graph.nodes, graph.links);
        setCapInfo(graph.capped ?? false, graph.originalNodeCount ?? null);
      } catch (err) {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : String(err));
        }
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [sourceFilter, setGraph, setCapInfo, setLoading, setError]);
}
