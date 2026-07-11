//! Fetches the graph + stats on mount and populates the store.

import { useEffect } from "react";
import { api } from "../lib/api";
import { useGraphStore } from "../store/graphStore";

export function useGraphData() {
  const setGraph = useGraphStore((s) => s.setGraph);
  const setStats = useGraphStore((s) => s.setStats);
  const setLoading = useGraphStore((s) => s.setLoading);
  const setError = useGraphStore((s) => s.setError);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      setLoading(true);
      setError(null);
      try {
        const [stats, graph] = await Promise.all([api.stats(), api.graph()]);
        if (cancelled) return;
        setStats(stats);
        setGraph(graph.nodes, graph.links);
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
  }, [setGraph, setStats, setLoading, setError]);
}
