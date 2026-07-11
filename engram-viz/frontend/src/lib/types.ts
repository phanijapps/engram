//! Shared API response types (mirror the backend route shapes).

export interface SourceInfo {
  id: string;
  kind: string;
  name: string;
}

export interface StatsResponse {
  nodeCount: number;
  edgeCount: number;
  relationshipCount: number;
  sources: SourceInfo[];
}

export interface GraphNode {
  id: string;
  name: string;
  kind: string;
  file?: string;
  line?: number;
  endLine?: number;
  community?: number;
  degree: number;
}

export interface GraphLink {
  source: string;
  target: string;
}

export interface GraphResponse {
  nodeCount: number;
  edgeCount: number;
  communityCount: number;
  nodes: GraphNode[];
  links: GraphLink[];
}

export interface InsightItem {
  id: string;
  name: string;
  kind: string;
  file?: string;
  score?: number;
  category?: string;
}

export interface InsightsResponse {
  deadCode: InsightItem[];
  centralSymbols: InsightItem[];
  bridgeSymbols: InsightItem[];
}

export interface Neighbor {
  id: string;
  name: string;
  kind: string;
  file?: string;
}

export interface NodeEntity {
  id: string;
  name: string;
  kind: string;
  file?: string;
  line?: number;
  endLine?: number;
  validFrom?: string;
  validUntil?: string | null;
  provenance?: {
    source?: string;
    observedAt?: string;
  };
}

export interface NodeDetail {
  entity: NodeEntity;
  source: string | null;
  complexity: number | null;
  community: number | null;
  callers: Neighbor[];
  callees: Neighbor[];
}

export interface SearchResult {
  id: string;
  name: string;
  kind: string;
  file?: string;
  score: number;
}

export interface SearchResponse {
  query: string;
  results: SearchResult[];
}

export interface TimelineBucket {
  date: string;
  count: number;
}

export interface TimelineResponse {
  timeline: TimelineBucket[];
  recentSymbols: { id: string; name: string; kind: string; validFrom?: string }[];
  overview: {
    communityCount: number;
    largestCommunitySize: number;
    classifiedSymbols: number;
  };
}
