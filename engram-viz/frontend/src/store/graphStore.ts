//! Global UI + graph state (zustand).

import { create } from "zustand";
import type { GraphLink, GraphNode, StatsResponse } from "../lib/types";

export type SidebarTab = "insights" | "taxonomy" | "ontology";

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

  // T7: Polish.
  sidebarCollapsed: boolean;
  sidebarTab: SidebarTab;
  nodeLimit: number | null; // null = show all nodes
  hiddenCommunities: number[]; // community labels hidden from the graph
  searchReady: boolean;

  // T9: Kind filter.
  kindFilter: string | null; // null = no filter

  // T10: Highlight overlay + path mode.
  highlightNodeIds: Set<string>; // nodes highlighted (blast radius / path / taxonomy)
  highlightColor: string | null; // color for the highlight overlay
  pathMode: boolean; // when true, clicks select path endpoints
  pathFromId: string | null;
  pathToId: string | null;
  groupByKind: boolean;

  // UI panels.
  timelineOpen: boolean;

  // Actions — data.
  setGraph: (nodes: GraphNode[], links: GraphLink[]) => void;
  setStats: (stats: StatsResponse | null) => void;
  setLoading: (loading: boolean) => void;
  setError: (error: string | null) => void;

  // Actions — interaction.
  focusNode: (id: string) => void;
  selectNode: (id: string | null) => void;
  setHovered: (id: string | null) => void;
  clearFocus: () => void;

  // Actions — T7 polish.
  toggleSidebar: () => void;
  setSidebarTab: (tab: SidebarTab) => void;
  setNodeLimit: (limit: number | null) => void;
  toggleCommunity: (community: number) => void;
  setSearchReady: (ready: boolean) => void;

  // Actions — T9 kind filter.
  setKindFilter: (kind: string | null) => void;

  // Actions — T10 highlights + path.
  setHighlight: (ids: Set<string>, color: string) => void;
  clearHighlight: () => void;
  togglePathMode: () => void;
  setPathFrom: (id: string | null) => void;
  setPathTo: (id: string | null) => void;
  resetPath: () => void;
  toggleGroupByKind: () => void;

  // Actions — panels.
  toggleTimeline: () => void;
}

export const useGraphStore = create<GraphState>((set) => ({
  nodes: [],
  links: [],
  loading: false,
  error: null,
  stats: null,
  focusNodeId: null,
  selectedNodeId: null,
  hoveredNodeId: null,

  sidebarCollapsed: false,
  sidebarTab: "insights",
  nodeLimit: 200,
  hiddenCommunities: [],
  searchReady: false,

  kindFilter: null,

  highlightNodeIds: new Set<string>(),
  highlightColor: null,
  pathMode: false,
  pathFromId: null,
  pathToId: null,
  groupByKind: false,

  timelineOpen: false,

  setGraph: (nodes, links) => set({ nodes, links }),
  setStats: (stats) => set({ stats }),
  setLoading: (loading) => set({ loading }),
  setError: (error) => set({ error }),

  focusNode: (id) => set({ focusNodeId: id, selectedNodeId: id }),
  selectNode: (id) => set({ selectedNodeId: id }),
  setHovered: (id) => set({ hoveredNodeId: id }),
  clearFocus: () => set({ focusNodeId: null, selectedNodeId: null }),

  toggleSidebar: () => set((s) => ({ sidebarCollapsed: !s.sidebarCollapsed })),
  setSidebarTab: (tab) => set({ sidebarTab: tab }),
  setNodeLimit: (limit) => set({ nodeLimit: limit }),
  toggleCommunity: (community) =>
    set((s) => ({
      hiddenCommunities: s.hiddenCommunities.includes(community)
        ? s.hiddenCommunities.filter((c) => c !== community)
        : [...s.hiddenCommunities, community],
    })),
  setSearchReady: (ready) => set({ searchReady: ready }),

  setKindFilter: (kind) => set({ kindFilter: kind }),

  setHighlight: (ids, color) =>
    set({ highlightNodeIds: ids, highlightColor: color }),
  clearHighlight: () =>
    set({
      highlightNodeIds: new Set<string>(),
      highlightColor: null,
    }),
  togglePathMode: () =>
    set((s) => ({
      pathMode: !s.pathMode,
      pathFromId: null,
      pathToId: null,
      highlightNodeIds: new Set<string>(),
      highlightColor: null,
    })),
  setPathFrom: (id) => set({ pathFromId: id }),
  setPathTo: (id) => set({ pathToId: id }),
  resetPath: () =>
    set({
      pathFromId: null,
      pathToId: null,
      highlightNodeIds: new Set<string>(),
      highlightColor: null,
    }),
  toggleGroupByKind: () => set((s) => ({ groupByKind: !s.groupByKind })),

  toggleTimeline: () => set((s) => ({ timelineOpen: !s.timelineOpen })),
}));
