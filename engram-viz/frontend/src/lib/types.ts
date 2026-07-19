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

/** One indexed repository. `id` is the stable_source_key — the value the
 *  `?source=` graph filter and entitiesBySource() expect. */
export interface RepoSource {
  id: string;
  name: string;
  entityCount: number;
  relationshipCount: number;
}

export interface SourceListResponse {
  sources: RepoSource[];
}

export interface GraphNode {
  id: string;
  name: string;
  kind: string;
  file?: string;
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
  sourceFilter?: string | null;
  /** True when the server pruned the graph to the top-`maxNodes` by degree. */
  capped?: boolean;
  /** The pre-cap node count, for a "Showing N of M" notice. */
  originalNodeCount?: number;
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

// --- T8: Taxonomy ---------------------------------------------------

export interface ConceptLabel {
  value: string;
  language?: string;
}

export interface TaxonomyConcept {
  id: string;
  uri: string;
  schemeId: string;
  prefLabel: ConceptLabel;
  altLabels?: ConceptLabel[];
  definition?: string;
  notation?: string;
  status: string;
  createdAt: string;
}

export interface TaxonomyScheme {
  id: string;
  uri: string;
  name: string;
  version: string;
  createdAt: string;
}

export interface ConceptRelation {
  id: string;
  schemeId: string;
  subjectId: string;
  predicate: "broader" | "narrower" | "related";
  objectId: string;
  createdAt: string;
}

export interface TaxonomyResponse {
  schemes: TaxonomyScheme[];
  concepts: TaxonomyConcept[];
  relations: ConceptRelation[];
}

// --- T9: Ontology ---------------------------------------------------

export interface OntologySummary {
  id: string;
  uri: string;
  name: string;
  version: string;
  status: string;
}

export interface OntologyClass {
  id: string;
  ontologyId: string;
  uri: string;
  label: string;
  description?: string;
  parentClassIds?: string[];
  status: string;
}

export interface OntologyProperty {
  id: string;
  ontologyId: string;
  uri: string;
  label: string;
  kind: string;
  domainClassId?: string;
  rangeClassId?: string;
  datatype?: string;
  status: string;
}

export interface OntologyAxiom {
  id: string;
  ontologyId: string;
  kind: string;
  subjectClassId?: string;
  propertyId?: string;
  objectClassId?: string;
}

export interface OntologyResponse {
  ontologies: OntologySummary[];
  classes: OntologyClass[];
  properties: OntologyProperty[];
  axioms: OntologyAxiom[];
}

// --- T10: Blast radius + path --------------------------------------

export interface BlastRadiusResponse {
  target: string;
  depth: number;
  callers: { id: string; name: string; kind: string; file?: string }[];
}

export interface PathNode {
  id: string;
  name: string;
  kind: string;
  file?: string;
}

export interface PathResponse {
  from: string;
  to: string;
  path: PathNode[] | null;
}

// --- Search readiness ----------------------------------------------

export interface SearchReadyResponse {
  ready: boolean;
  building: boolean;
}
