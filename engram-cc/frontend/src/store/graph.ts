//! Drill state for the Graph tab (zustand). Clicking a community meta-node opens
//! a drill: fetch a bounded, paginated sample of its member entities; selecting a
//! member fetches its detail (community, degree, provenance). All loads go
//! through the BFF client; the server enforces every bound (page + per-community
//! caps). The client state is intentionally simple — loading flags + an append
//! cursor + an "exhausted" latch — so the E2E drill flow is its verification.

import { create } from "zustand";
import {
  api,
  type CommunityMetaNode,
  type EntityDetail,
  type GraphEntityView,
  type GraphRelationshipView,
} from "../lib/api.ts";

const MEMBERS_PAGE = 100;

interface DrillState {
  community: { id: string; name: string; memberCount: number } | null;
  members: GraphEntityView[];
  memberEdges: GraphRelationshipView[];
  membersCursor: string | null;
  membersExhausted: boolean;
  membersLoading: boolean;
  selectedEntityId: string | null;
  detail: EntityDetail | null;
  detailLoading: boolean;
  error: string | null;

  drillCommunity: (node: CommunityMetaNode) => Promise<void>;
  loadMoreMembers: () => Promise<void>;
  selectEntity: (id: string) => Promise<void>;
  clearDrill: () => void;
}

export const useGraphStore = create<DrillState>((set, get) => ({
  community: null,
  members: [],
  memberEdges: [],
  membersCursor: null,
  membersExhausted: false,
  membersLoading: false,
  selectedEntityId: null,
  detail: null,
  detailLoading: false,
  error: null,

  drillCommunity: async (node) => {
    set({
      community: { id: node.id, name: node.name, memberCount: node.memberCount },
      members: [],
      memberEdges: [],
      membersCursor: null,
      membersExhausted: false,
      membersLoading: true,
      selectedEntityId: null,
      detail: null,
      error: null,
    });
    try {
      const page = await api.communityMembers(node.id, null, MEMBERS_PAGE);
      set({
        members: page.items,
        memberEdges: page.edges,
        membersCursor: page.nextCursor,
        membersExhausted: !page.nextCursor,
        membersLoading: false,
      });
    } catch (e) {
      set({ membersLoading: false, error: msg(e) });
    }
  },

  loadMoreMembers: async () => {
    const { community, membersCursor, membersExhausted, membersLoading } = get();
    if (!community || membersLoading || membersExhausted || !membersCursor) return;
    set({ membersLoading: true });
    try {
      const page = await api.communityMembers(community.id, membersCursor, MEMBERS_PAGE);
      set((s) => ({
        members: [...s.members, ...page.items],
        memberEdges: [...s.memberEdges, ...page.edges],
        membersCursor: page.nextCursor,
        membersExhausted: !page.nextCursor,
        membersLoading: false,
      }));
    } catch (e) {
      set({ membersLoading: false, error: msg(e) });
    }
  },

  selectEntity: async (id) => {
    set({ selectedEntityId: id, detail: null, detailLoading: true, error: null });
    try {
      const detail = await api.entityDetail(id);
      set({ detail, detailLoading: false });
    } catch (e) {
      set({ detailLoading: false, error: msg(e) });
    }
  },

  clearDrill: () =>
    set({
      community: null,
      members: [],
      memberEdges: [],
      membersCursor: null,
      membersExhausted: false,
      membersLoading: false,
      selectedEntityId: null,
      detail: null,
      detailLoading: false,
      error: null,
    }),
}));

function msg(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}
