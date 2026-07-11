//! Global UI + graph state (zustand).

import { create } from "zustand";
import type { GraphLink, GraphNode, StatsResponse } from "../lib/types";

interface GraphState {
  // Data.
  stats: StatsResponse | null;
  nodes: GraphNode[];
  links: GraphLink[];
  loading: boolean;
  error: string | null;

  // Interaction.
  focusNodeId: string | null; // node to highlight + recenter (from insights/search)
  selectedNodeId: string | null; // node whose detail panel is open
  hoveredNodeId: string | null;

  // UI panels.
  timelineOpen: boolean;

  // Actions.
  setGraph: (nodes: GraphNode[], links: GraphLink[]) => void;
  setStats: (stats: StatsResponse | null) => void;
  setLoading: (loading: boolean) => void;
  setError: (error: string | null) => void;
  focusNode: (id: string) => void;
  selectNode: (id: string | null) => void;
  setHovered: (id: string | null) => void;
  clearFocus: () => void;
  toggleTimeline: () => void;
}

export const useGraphStore = create<GraphState>((set) => ({
  stats: null,
  nodes: [],
  links: [],
  loading: false,
  error: null,
  focusNodeId: null,
  selectedNodeId: null,
  hoveredNodeId: null,
  timelineOpen: false,

  setGraph: (nodes, links) => set({ nodes, links }),
  setStats: (stats) => set({ stats }),
  setLoading: (loading) => set({ loading }),
  setError: (error) => set({ error }),
  focusNode: (id) => set({ focusNodeId: id, selectedNodeId: id }),
  selectNode: (id) => set({ selectedNodeId: id }),
  setHovered: (id) => set({ hoveredNodeId: id }),
  clearFocus: () => set({ focusNodeId: null, selectedNodeId: null }),
  toggleTimeline: () => set((s) => ({ timelineOpen: !s.timelineOpen })),
}));
