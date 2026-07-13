//! Singleton wrapper around the @engram/node native knowledge engine.
//!
//! Opens the codegraph SQLite db once (path from ENGRAM_DB or the default
//! ~/.engram/codegraph-mem-alpha.db) and exposes typed helpers over the raw
//! `*Json` N-API methods. Every method takes/returns JSON strings at the native
//! boundary; this module stringifies the request, calls the engine, and parses
//! the response into typed values.
//!
//! The native binding's TypeScript declarations lag the Rust surface (they omit
//! the codegraph methods), so we declare the methods we use and cast.

import { loadNativeBinding } from "@engram/node";
import os from "node:os";
import path from "node:path";
import { DatabaseSync } from "node:sqlite";

// --- Domain shapes (subset we consume) -------------------------------

export interface Scope {
  tenant: string;
  workspace: string;
}

export interface EntityLocation {
  path?: string;
  startLine?: number;
  endLine?: number;
}

export interface EntitySourceRef {
  targetType: string;
  targetId: string;
  location?: EntityLocation;
}

export interface Entity {
  id: string;
  graphId?: string;
  kind: string;
  name: string;
  scope?: Scope;
  sourceRefs?: EntitySourceRef[];
  provenance?: {
    source?: string;
    observedAt?: string;
  };
  createdAt?: string;
  validFrom?: string;
  validUntil?: string | null;
  file?: string;
  conceptRefs?: { id: string }[];
}

export interface EntityRef {
  id?: string;
  kind?: string;
  name?: string;
}

export interface Relationship {
  id: string;
  subject: EntityRef;
  predicate: string;
  object: EntityRef;
  confidence?: number;
  createdAt?: string;
}

export interface Chunk {
  id: string;
  documentId?: string;
  kind?: string;
  text?: string;
  location?: EntityLocation;
  entities?: string[];
  createdAt?: string;
}

export interface Source {
  id: string;
  kind?: string;
  name?: string;
}

export interface RankedSymbol {
  key: string;
  score: number;
}

// --- Taxonomy shapes -----------------------------------------------

export interface ConceptLabel {
  value: string;
  language?: string;
}

export interface Concept {
  id: string;
  uri: string;
  schemeId: string;
  prefLabel: ConceptLabel;
  altLabels?: ConceptLabel[];
  definition?: string;
  notation?: string;
  status: string;
  createdAt: string;
  updatedAt?: string;
}

export interface ConceptScheme {
  id: string;
  uri: string;
  name: string;
  scope: Scope;
  version: string;
  createdAt: string;
  updatedAt?: string;
}

export interface ConceptRelation {
  id: string;
  schemeId: string;
  subjectId: string;
  predicate: "broader" | "narrower" | "related";
  objectId: string;
  createdAt: string;
}

// --- Ontology shapes -----------------------------------------------

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
  expression?: unknown;
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

// --- Native engine surface (cast; TS decls lag the codegraph methods) -

interface NativeEngine {
  listEntitiesJson(req: string): string;
  listEntitiesBySourceJson(req: string): string;
  listRelationshipsJson(req: string): string;
  listSourcesJson(req: string): string;
  listChunksJson(req: string): string;
  getEntityJson(req: string): string;
  getChunkJson(req: string): string;
  deadCodeJson(req: string): string;
  centralSymbolsJson(req: string): string;
  bridgeSymbolsJson(req: string): string;
  callCommunitiesJson(req: string): string;
  cyclomaticComplexityJson(req: string): string;
  indexForSearchJson(req: string): string;
  searchCodeJson(req: string): string;
  blastRadiusJson(req: string): string;
  dependencyPathJson(req: string): string;
  listConceptsJson(req: string): string;
  getConceptSchemeJson(req: string): string;
  getOntologyJson(req: string): string;
}

function defaultDbPath(): string {
  const fromEnv = process.env.ENGRAM_DB;
  if (fromEnv && fromEnv.length > 0) return fromEnv;
  return path.join(os.homedir(), ".engram", "codegraph-mem-alpha.db");
}

/**
 * Turns a `stable_source_key` (e.g. `github.com/phanijapps/engram`) into a
 * short display name (`phanijapps/engram`). Keys that don't look like a
 * host/org/repo triple are returned unchanged.
 */
function repoDisplayName(stableSourceKey: string): string {
  const parts = stableSourceKey.split("/").filter(Boolean);
  // host / org / repo  →  org / repo
  if (parts.length >= 3 && parts[0].includes(".")) return parts.slice(1).join("/");
  return stableSourceKey;
}

/** Default Louvain pass count; the result at this value is cacheable. */
const DEFAULT_MAX_PASSES = 10;

class CodegraphEngine {
  readonly scope: Scope = { tenant: "default", workspace: "codegraph" };
  readonly dbPath: string;
  private readonly native: NativeEngine;

  // Caches rebuilt lazily; invalidated by invalidateCache() after a scan.
  private entityCache: Entity[] | null = null;
  private relCache: Relationship[] | null = null;
  // Louvain community map is derived from relationships(); cache it so
  // repeat /api/graph calls skip the recompute. Invalidated with the rest.
  private communityCache: Record<string, number> | null = null;
  private chunkCache: Chunk[] | null = null;
  private entityById: Map<string, Entity> | null = null;
  private entityByName: Map<string, Entity> | null = null;
  private entityChunkIndex: Map<string, Chunk> | null = null;
  private lexicalReady = false;
  private lexicalBuilding = false;

  constructor(dbPath: string) {
    this.dbPath = dbPath;
    const binding = loadNativeBinding();
    // The binding object owns the NativeKnowledgeEngine constructor.
    this.native = new (binding as unknown as {
      NativeKnowledgeEngine: new (path: string | null) => NativeEngine;
    }).NativeKnowledgeEngine(dbPath);
  }

  private enc<T>(value: T): string {
    return JSON.stringify(value);
  }

  /** Ensures the BM25 lexical index is built exactly once per process.
   * NOTE: indexForSearchJson is a synchronous N-API call that blocks the
   * event loop; the first /api/search after startup will stall while it
   * runs. The `building` flag is set so /api/search/ready can report it.
   * Offloading to a worker thread is deferred (see docs/backlog.md). */
  ensureLexical(): void {
    if (this.lexicalReady) return;
    this.lexicalBuilding = true;
    try {
      this.native.indexForSearchJson(this.enc(this.scope));
      this.lexicalReady = true;
    } finally {
      this.lexicalBuilding = false;
    }
  }

  /** Whether the lexical search index is ready (built and not building). */
  get lexicalSearchReady(): boolean {
    return this.lexicalReady;
  }

  /** Whether the lexical search index is currently being built. */
  get lexicalSearchBuilding(): boolean {
    return this.lexicalBuilding;
  }

  invalidateCache(): void {
    this.entityCache = null;
    this.relCache = null;
    this.communityCache = null;
    this.chunkCache = null;
    this.entityById = null;
    this.entityByName = null;
    this.entityChunkIndex = null;
    // Rebuild the lexical index so search reflects the new data.
    this.lexicalReady = false;
    this.lexicalBuilding = false;
  }

  // --- raw data accessors (cached) ------------------------------------

  entities(): Entity[] {
    if (!this.entityCache) {
      this.entityCache = JSON.parse(
        this.native.listEntitiesJson(this.enc({ scope: this.scope })),
      ) as Entity[];
    }
    return this.entityCache;
  }

  relationships(): Relationship[] {
    if (!this.relCache) {
      this.relCache = JSON.parse(
        this.native.listRelationshipsJson(this.enc({ scope: this.scope })),
      ) as Relationship[];
    }
    return this.relCache;
  }

  chunks(): Chunk[] {
    if (!this.chunkCache) {
      this.chunkCache = JSON.parse(
        this.native.listChunksJson(this.enc({ scope: this.scope })),
      ) as Chunk[];
    }
    return this.chunkCache;
  }

  sources(): Source[] {
    return JSON.parse(
      this.native.listSourcesJson(this.enc({ scope: this.scope })),
    ) as Source[];
  }

  entitiesBySource(sourceId: string): Entity[] {
    const raw = this.native.listEntitiesBySourceJson(
      this.enc({ scope: this.scope, stableSourceKey: sourceId }),
    );
    return JSON.parse(raw) as Entity[];
  }

  /**
   * Lists the indexed repositories grouped by `stable_source_key` — the
   * canonical repo identity that entities and relationships are partitioned
   * by (and the value `entitiesBySource` / the `?source=` graph filter
   * expect). This is NOT the `KnowledgeSource.id` (`source-…`), which is
   * metadata about one ingestion event and does not match the graph filter.
   *
   * Uses node:sqlite for discovery (the N-API surface lists KnowledgeSource
   * rows, not distinct stable keys) and counts entities + relationships per
   * key, scoped to this engine's tenant/workspace.
   */
  repos(): {
    id: string;
    name: string;
    entityCount: number;
    relationshipCount: number;
  }[] {
    const db = this.openDbReader();
    const { tenant, workspace } = this.scope;
    const rows = db
      .prepare(
        `SELECT g.stable_source_key AS key,
                COUNT(DISTINCT e.id) AS entities,
                COUNT(DISTINCT r.id) AS rels
           FROM knowledge_graphs g
           LEFT JOIN knowledge_entities e ON e.graph_id = g.id
           LEFT JOIN knowledge_relationships r ON r.graph_id = g.id
          WHERE g.tenant = ? AND g.workspace = ?
            AND g.stable_source_key IS NOT NULL
            AND g.stable_source_key != ''
          GROUP BY g.stable_source_key
          ORDER BY entities DESC`,
      )
      .all(tenant, workspace) as { key: string; entities: number; rels: number }[];
    db.close();
    return rows.map((r) => ({
      id: r.key,
      name: repoDisplayName(r.key),
      entityCount: r.entities,
      relationshipCount: r.rels,
    }));
  }

  entityByIdMap(): Map<string, Entity> {
    if (!this.entityById) {
      const m = new Map<string, Entity>();
      for (const e of this.entities()) m.set(e.id, e);
      this.entityById = m;
    }
    return this.entityById;
  }

  /**
   * First entity per name (best-effort fallback). Analytics keys occasionally
   * fall back to a name when a calls ref lacks an entity id; this lets us enrich
   * those name-keyed nodes with real metadata.
   */
  entityByNameMap(): Map<string, Entity> {
    if (!this.entityByName) {
      const m = new Map<string, Entity>();
      for (const e of this.entities()) {
        if (!m.has(e.name)) m.set(e.name, e);
      }
      this.entityByName = m;
    }
    return this.entityByName;
  }

  /**
   * Resolves an analytics key (entity id or, as fallback, a name) to a real
   * entity id. Returns the input unchanged when no entity matches.
   */
  resolveEntityId(key: string): string {
    if (this.entityByIdMap().has(key)) return key;
    const byName = this.entityByNameMap().get(key);
    if (byName) return byName.id;
    return key;
  }

  /** Looks up an entity by id or (fallback) name. */
  entityByKey(key: string): Entity | undefined {
    return this.entityByIdMap().get(key) ?? this.entityByNameMap().get(key);
  }

  /** Chunk that references a given entity id (entity -> source text). */
  chunkForEntity(entityId: string): Chunk | undefined {
    if (!this.entityChunkIndex) {
      const m = new Map<string, Chunk>();
      for (const c of this.chunks()) {
        if (!c.entities) continue;
        for (const ref of c.entities) {
          if (!m.has(ref)) m.set(ref, c);
        }
      }
      this.entityChunkIndex = m;
    }
    return this.entityChunkIndex.get(entityId);
  }

  // --- single-entity + analytics --------------------------------------

  getEntity(id: string): Entity | null {
    const raw = this.native.getEntityJson(this.enc({ id, scope: this.scope }));
    const parsed = JSON.parse(raw) as Entity | null;
    return parsed ?? null;
  }

  deadCode(): string[] {
    return JSON.parse(
      this.native.deadCodeJson(this.enc({ scope: this.scope })),
    ) as string[];
  }

  centralSymbols(limit = 20): RankedSymbol[] {
    const raw = this.native.centralSymbolsJson(
      this.enc({ scope: this.scope, limit }),
    );
    const tuples = JSON.parse(raw) as [string, number][];
    return tuples.map(([key, score]) => ({ key, score }));
  }

  bridgeSymbols(limit = 20): RankedSymbol[] {
    const raw = this.native.bridgeSymbolsJson(
      this.enc({ scope: this.scope, limit }),
    );
    const tuples = JSON.parse(raw) as [string, number][];
    return tuples.map(([key, score]) => ({ key, score }));
  }

  /** Raw `{ symbolKey: communityLabel }` Louvain partition. */
  communities(maxPasses = DEFAULT_MAX_PASSES): Record<string, number> {
    // The default-passes result is cacheable; a non-default maxPasses is a
    // deliberate recompute (e.g. exploration), so bypass the cache then.
    const cacheable = maxPasses === DEFAULT_MAX_PASSES;
    if (cacheable && this.communityCache) return this.communityCache;
    const raw = this.native.callCommunitiesJson(
      this.enc({ scope: this.scope, maxPasses }),
    );
    const map = JSON.parse(raw) as Record<string, number>;
    if (cacheable) this.communityCache = map;
    return map;
  }

  cyclomaticComplexity(source: string): number {
    return Number(
      this.native.cyclomaticComplexityJson(this.enc({ source })),
    );
  }

  search(query: string, limit = 20): { id: string; score: number }[] {
    this.ensureLexical();
    const raw = this.native.searchCodeJson(this.enc({ query, limit }));
    return JSON.parse(raw) as { id: string; score: number }[];
  }

  // --- codegraph analytics: blast radius + dependency path -------------

  /** Transitive callers of `target` (blast radius), up to `depth` hops. */
  blastRadius(target: string, depth = 5): string[] {
    const raw = this.native.blastRadiusJson(
      this.enc({ scope: this.scope, target, depth }),
    );
    return JSON.parse(raw) as string[];
  }

  /** Shortest call path from `from` to `to`, or null when unreachable. */
  dependencyPath(from: string, to: string): string[] | null {
    const raw = this.native.dependencyPathJson(
      this.enc({ scope: this.scope, from, to }),
    );
    return JSON.parse(raw) as string[] | null;
  }

  // --- taxonomy (SQLite-backed discovery; N-API lacks list-all) --------

  /**
   * Seeds a code-derived taxonomy + ontology from the existing entity data.
   * Writes directly to the SQLite tables (bypassing N-API struct matching).
   * Idempotent — checks if data already exists before seeding.
   */
  seedTaxonomyOntology(): { seeded: boolean; message: string } {
    // Check if already seeded
    const existing = this.taxonomy();
    if (existing.schemes.length > 0) {
      return { seeded: false, message: "taxonomy already exists" };
    }

    const now = new Date().toISOString();
    const { tenant, workspace } = this.scope;
    const db = this.openDbWriter();
    const entities = this.entities();
    const kindCounts = new Map<string, number>();
    for (const e of entities) {
      const k = e.kind || "unknown";
      kindCounts.set(k, (kindCounts.get(k) || 0) + 1);
    }

    // --- Concept Scheme ---
    const schemeId = "scheme-code-entity-taxonomy";
    db.prepare(
      "INSERT OR REPLACE INTO concept_schemes (id, tenant, workspace, record_json) VALUES (?, ?, ?, ?)",
    ).run(schemeId, tenant, workspace, JSON.stringify({
      id: schemeId, uri: `engram://${tenant}/taxonomy/code-entity`, name: "Code Entity Taxonomy",
      scope: this.scope, version: "1.0",
      provenance: { source: "engram-viz", actor: { id: "engram-viz", kind: "system" }, observedAt: now },
      policy: { visibility: "workspace", retention: "durable" },
      createdAt: now,
    }));

    // --- Concepts ---
    // Root: "Code Entity"
    const rootId = "concept-code-entity";
    db.prepare("INSERT OR REPLACE INTO concepts (id, scheme_id, record_json) VALUES (?, ?, ?)")
      .run(rootId, schemeId, JSON.stringify({
        id: rootId, uri: `engram://${tenant}/concept/code-entity`, schemeId,
        prefLabel: { value: "Code Entity" }, altLabels: [], definition: "Any structural code element extracted from the repository.",
        status: "active", provenance: { source: "engram-viz", actor: { id: "engram-viz", kind: "system" }, observedAt: now }, createdAt: now,
      }));

    // One per EntityKind
    for (const [kind, count] of kindCounts) {
      const conceptId = `concept-kind-${kind}`;
      const label = kind.charAt(0).toUpperCase() + kind.slice(1);
      db.prepare("INSERT OR REPLACE INTO concepts (id, scheme_id, record_json) VALUES (?, ?, ?)")
        .run(conceptId, schemeId, JSON.stringify({
          id: conceptId, uri: `engram://${tenant}/concept/${kind}`, schemeId,
          prefLabel: { value: label }, altLabels: [{ value: kind }],
          definition: `${count} ${kind} entities in the indexed repository.`,
          status: "active",
          provenance: { source: "engram-viz", actor: { id: "engram-viz", kind: "system" }, observedAt: now },
          createdAt: now,
        }));

      // Broader relation: root -> kind
      db.prepare("INSERT OR REPLACE INTO concept_relations (id, scheme_id, record_json) VALUES (?, ?, ?)")
        .run(`rel-${rootId}-${conceptId}`, schemeId, JSON.stringify({
          id: `rel-${rootId}-${conceptId}`, schemeId, subjectId: rootId, predicate: "broader", objectId: conceptId,
          provenance: { source: "engram-viz", actor: { id: "engram-viz", kind: "system" }, observedAt: now }, createdAt: now,
        }));
    }

    // --- Ontology ---
    const ontologyId = "ontology-code-structure";
    db.prepare(
      "INSERT OR REPLACE INTO ontologies (id, tenant, workspace, record_json) VALUES (?, ?, ?, ?)",
    ).run(ontologyId, tenant, workspace, JSON.stringify({
      id: ontologyId, name: "Code Structure Ontology", version: "1.0",
      scope: this.scope,
      provenance: { source: "engram-viz", actor: { id: "engram-viz", kind: "system" }, observedAt: now },
      policy: { visibility: "workspace", retention: "durable" },
      createdAt: now,
    }));

    // Classes per EntityKind
    for (const [kind, count] of kindCounts) {
      db.prepare("INSERT OR REPLACE INTO ontology_classes (id, ontology_id, record_json) VALUES (?, ?, ?)")
        .run(`class-${kind}`, ontologyId, JSON.stringify({
          id: `class-${kind}`, ontologyId, name: kind.charAt(0).toUpperCase() + kind.slice(1),
          description: `${count} instances in the code graph.`,
          provenance: { source: "engram-viz", actor: { id: "engram-viz", kind: "system" }, observedAt: now },
          createdAt: now,
        }));
    }

    // Properties
    for (const propName of ["name", "file", "kind"]) {
      db.prepare("INSERT OR REPLACE INTO ontology_properties (id, ontology_id, record_json) VALUES (?, ?, ?)")
        .run(`property-${propName}`, ontologyId, JSON.stringify({
          id: `property-${propName}`, ontologyId, name: propName,
          description: `The ${propName} of a code entity.`,
          propertyType: propName === "kind" ? "enum" : "string",
          provenance: { source: "engram-viz", actor: { id: "engram-viz", kind: "system" }, observedAt: now },
          createdAt: now,
        }));
    }

    db.close();
    return { seeded: true, message: `seeded ${kindCounts.size + 1} concepts + ${kindCounts.size} classes + 3 properties` };
  }

  /**
   * Returns all concept schemes, concepts, and relations for the current
   * scope. Uses node:sqlite for discovery (the N-API surface has no
   * list-all-schemes method) and parses the record_json columns directly.
   */
  taxonomy(): {
    schemes: ConceptScheme[];
    concepts: Concept[];
    relations: ConceptRelation[];
  } {
    const db = this.openDbReader();
    const { tenant, workspace } = this.scope;

    // Schemes scoped to this tenant/workspace.
    const schemeRows = db
      .prepare(
        "SELECT record_json FROM concept_schemes WHERE tenant = ? AND workspace = ?",
      )
      .all(tenant, workspace) as { record_json: string }[];
    const schemes = schemeRows.map(
      (r) => parseRecord(r.record_json) as unknown as ConceptScheme,
    );

    // Concepts for the discovered schemes.
    const schemeIds = schemes.map((s) => s.id);
    let concepts: Concept[] = [];
    let relations: ConceptRelation[] = [];
    if (schemeIds.length > 0) {
      const placeholders = schemeIds.map(() => "?").join(",");
      const conceptRows = db
        .prepare(`SELECT record_json FROM concepts WHERE scheme_id IN (${placeholders})`)
        .all(...schemeIds) as { record_json: string }[];
      concepts = conceptRows.map(
        (r) => parseRecord(r.record_json) as unknown as Concept,
      );

      const relationRows = db
        .prepare(
          `SELECT record_json FROM concept_relations WHERE scheme_id IN (${placeholders})`,
        )
        .all(...schemeIds) as { record_json: string }[];
      relations = relationRows.map(
        (r) => parseRecord(r.record_json) as unknown as ConceptRelation,
      );
    }
    db.close();

    return { schemes, concepts, relations };
  }

  // --- ontology (SQLite-backed discovery; N-API lacks list-all) --------

  /**
   * Returns ontology classes, properties, and axioms for the current scope.
   * The N-API `getOntologyJson` returns only the Ontology header; the child
   * collections (classes, properties, axioms) require direct SQLite reads.
   */
  ontology(): {
    ontologies: {
      id: string;
      uri: string;
      name: string;
      version: string;
      status: string;
    }[];
    classes: OntologyClass[];
    properties: OntologyProperty[];
    axioms: OntologyAxiom[];
  } {
    const db = this.openDbReader();
    const { tenant, workspace } = this.scope;

    const ontoRows = db
      .prepare(
        "SELECT record_json FROM ontologies WHERE tenant = ? AND workspace = ?",
      )
      .all(tenant, workspace) as { record_json: string }[];

    const ontologies = ontoRows.map((r) => {
      const o = parseRecord(r.record_json) as Record<string, unknown>;
      return {
        id: String(o.id ?? ""),
        uri: String(o.uri ?? ""),
        name: String(o.name ?? ""),
        version: String(o.version ?? ""),
        status: String(o.status ?? "active"),
      };
    });

    const ontoIds = ontologies.map((o) => o.id);
    let classes: OntologyClass[] = [];
    let properties: OntologyProperty[] = [];
    let axioms: OntologyAxiom[] = [];
    if (ontoIds.length > 0) {
      const ph = ontoIds.map(() => "?").join(",");
      const classRows = db
        .prepare(`SELECT record_json FROM ontology_classes WHERE ontology_id IN (${ph})`)
        .all(...ontoIds) as { record_json: string }[];
      classes = classRows.map(
        (r) => parseRecord(r.record_json) as unknown as OntologyClass,
      );

      const propRows = db
        .prepare(`SELECT record_json FROM ontology_properties WHERE ontology_id IN (${ph})`)
        .all(...ontoIds) as { record_json: string }[];
      properties = propRows.map(
        (r) => parseRecord(r.record_json) as unknown as OntologyProperty,
      );

      const axiomRows = db
        .prepare(`SELECT record_json FROM ontology_axioms WHERE ontology_id IN (${ph})`)
        .all(...ontoIds) as { record_json: string }[];
      axioms = axiomRows.map(
        (r) => parseRecord(r.record_json) as unknown as OntologyAxiom,
      );
    }
    db.close();

    return { ontologies, classes, properties, axioms };
  }

  /**
   * Clears all taxonomy + ontology data from the db (for re-seeding).
   */
  clearTaxonomyOntology(): void {
    const db = this.openDbWriter();
    db.exec("DELETE FROM concept_relations");
    db.exec("DELETE FROM concepts");
    db.exec("DELETE FROM concept_schemes");
    db.exec("DELETE FROM ontology_properties");
    db.exec("DELETE FROM ontology_classes");
    db.exec("DELETE FROM ontologies");
    db.close();
  }

  /**
   * Seeds a configurable enterprise taxonomy + ontology covering Business, IT,
   * Support, and Customer domains. Pass a custom config JSON to override the
   * default structure.
   */
  seedEnterpriseTaxonomyOntology(configJson?: string): { seeded: boolean; message: string } {
    type EnterpriseTaxonomyConfig = {
      schemeId: string;
      schemeTitle: string;
      domains: Record<string, string[]>;
      ontologyId: string;
      ontologyName: string;
      properties: { name: string; type: string; description: string }[];
    };
    const defaultConfig: EnterpriseTaxonomyConfig = {
      schemeId: "scheme-enterprise-ops",
      schemeTitle: "Enterprise Operations Taxonomy",
      domains: {
        business: ["Strategy", "Process", "Compliance", "Finance", "Procurement"],
        it: ["Infrastructure", "Applications", "Security", "Data", "Cloud", "Development"],
        support: ["Help Desk", "Incident Management", "Change Management", "Knowledge Base", "Service Level"],
        customer: ["Account Management", "Service Requests", "Feedback", "Onboarding", "Escalation"],
      },
      ontologyId: "ontology-enterprise-ops",
      ontologyName: "Enterprise Operations Ontology",
      properties: [
        { name: "owner", type: "string", description: "Team or individual responsible" },
        { name: "priority", type: "enum", description: "Priority level (P1-P4)" },
        { name: "status", type: "enum", description: "Current lifecycle status" },
        { name: "category", type: "string", description: "Primary classification" },
        { name: "impact", type: "enum", description: "Business impact level" },
        { name: "assignee", type: "string", description: "Assigned team member" },
        { name: "sla", type: "string", description: "Service level agreement target" },
      ],
    };
    const config: EnterpriseTaxonomyConfig = configJson
      ? { ...defaultConfig, ...(JSON.parse(configJson) as Partial<EnterpriseTaxonomyConfig>) }
      : defaultConfig;

    this.clearTaxonomyOntology();

    const now = new Date().toISOString();
    const { tenant, workspace } = this.scope;
    const db = this.openDbWriter();
    const prov = { source: "engram-viz", actor: { id: "engram-viz", kind: "system" }, observedAt: now };
    const policy = { visibility: "workspace", retention: "durable" };
    let conceptCount = 0;
    let classCount = 0;

    const schemeId = config.schemeId;
    db.prepare("INSERT OR REPLACE INTO concept_schemes (id, tenant, workspace, record_json) VALUES (?, ?, ?, ?)")
      .run(schemeId, tenant, workspace, JSON.stringify({
        id: schemeId, uri: `engram://${tenant}/taxonomy/enterprise-ops`,
        name: config.schemeTitle, scope: this.scope, version: "1.0",
        provenance: prov, policy, createdAt: now,
      }));

    const rootId = `${schemeId}-root`;
    db.prepare("INSERT OR REPLACE INTO concepts (id, scheme_id, record_json) VALUES (?, ?, ?)")
      .run(rootId, schemeId, JSON.stringify({
        id: rootId, uri: `engram://${tenant}/concept/enterprise`, schemeId,
        prefLabel: { value: "Enterprise" }, altLabels: [], status: "active",
        definition: "Root of the enterprise operations taxonomy.",
        provenance: prov, createdAt: now,
      }));
    conceptCount++;

    for (const [domainKey, subAreas] of Object.entries(config.domains)) {
      const domainId = `${schemeId}-${domainKey}`;
      const domainLabel = domainKey.charAt(0).toUpperCase() + domainKey.slice(1);

      db.prepare("INSERT OR REPLACE INTO concepts (id, scheme_id, record_json) VALUES (?, ?, ?)")
        .run(domainId, schemeId, JSON.stringify({
          id: domainId, uri: `engram://${tenant}/concept/${domainKey}`, schemeId,
          prefLabel: { value: domainLabel }, altLabels: [{ value: domainKey }], status: "active",
          definition: `${domainLabel} domain.`,
          provenance: prov, createdAt: now,
        }));
      conceptCount++;

      db.prepare("INSERT OR REPLACE INTO concept_relations (id, scheme_id, record_json) VALUES (?, ?, ?)")
        .run(`rel-${rootId}-${domainId}`, schemeId, JSON.stringify({
          id: `rel-${rootId}-${domainId}`, schemeId, subjectId: rootId, predicate: "broader", objectId: domainId,
          provenance: prov, createdAt: now,
        }));

      for (const subArea of subAreas) {
        const slug = subArea.toLowerCase().replace(/\s+/g, "-");
        const subId = `${schemeId}-${domainKey}-${slug}`;
        db.prepare("INSERT OR REPLACE INTO concepts (id, scheme_id, record_json) VALUES (?, ?, ?)")
          .run(subId, schemeId, JSON.stringify({
            id: subId, uri: `engram://${tenant}/concept/${domainKey}/${slug}`,
            schemeId, prefLabel: { value: subArea }, altLabels: [], status: "active",
            definition: `${subArea} under the ${domainLabel} domain.`,
            provenance: prov, createdAt: now,
          }));
        conceptCount++;

        db.prepare("INSERT OR REPLACE INTO concept_relations (id, scheme_id, record_json) VALUES (?, ?, ?)")
          .run(`rel-${domainId}-${subId}`, schemeId, JSON.stringify({
            id: `rel-${domainId}-${subId}`, schemeId, subjectId: domainId, predicate: "broader", objectId: subId,
            provenance: prov, createdAt: now,
          }));
      }
    }

    const ontologyId = config.ontologyId;
    db.prepare("INSERT OR REPLACE INTO ontologies (id, tenant, workspace, record_json) VALUES (?, ?, ?, ?)")
      .run(ontologyId, tenant, workspace, JSON.stringify({
        id: ontologyId, name: config.ontologyName, version: "1.0",
        scope: this.scope, provenance: prov, policy, createdAt: now,
      }));

    const crossCuttingClasses = ["Service", "Process", "Incident", "Request", "Asset", "Team", "Policy", "Metric"];
    for (const cls of [...Object.keys(config.domains).map(k => k.charAt(0).toUpperCase() + k.slice(1)), ...crossCuttingClasses]) {
      const clsId = `class-${cls.toLowerCase()}`;
      db.prepare("INSERT OR REPLACE INTO ontology_classes (id, ontology_id, record_json) VALUES (?, ?, ?)")
        .run(clsId, ontologyId, JSON.stringify({
          id: clsId, ontologyId, name: cls,
          description: `${cls} class in the enterprise operations ontology.`,
          provenance: prov, createdAt: now,
        }));
      classCount++;
    }

    for (const prop of config.properties) {
      const propId = `property-${prop.name}`;
      db.prepare("INSERT OR REPLACE INTO ontology_properties (id, ontology_id, record_json) VALUES (?, ?, ?)")
        .run(propId, ontologyId, JSON.stringify({
          id: propId, ontologyId, name: prop.name,
          description: prop.description, propertyType: prop.type,
          provenance: prov, createdAt: now,
        }));
    }

    db.close();
    return {
      seeded: true,
      message: `seeded ${conceptCount} concepts + ${classCount} classes + ${config.properties.length} properties across ${Object.keys(config.domains).length} domains`,
    };
  }

  // --- internals ------------------------------------------------------

  /** Opens a read-only SQLite connection for taxonomy/ontology discovery. */
  private openDbReader(): DatabaseSync {
    return new DatabaseSync(this.dbPath, { readOnly: true });
  }

  /** Opens a read-write SQLite connection for seeding. */
  private openDbWriter(): DatabaseSync {
    return new DatabaseSync(this.dbPath);
  }

  // On-demand auto-tagging: classifies each entity into taxonomy concepts
  // based on file-path patterns. Writes conceptRefs back to each entity's
  // record_json. Idempotent. Pass customRules to override defaults.
  autoTag(customRules?: Record<string, string>): {
    tagged: number;
    untagged: number;
    byConcept: Record<string, number>;
  } {
    const defaultRules: Record<string, string> = {
      "adapters/memory/sqlite": "scheme-enterprise-ops-it-infrastructure",
      "adapters/knowledge/sqlite": "scheme-enterprise-ops-it-infrastructure",
      "adapters/hierarchy/sqlite": "scheme-enterprise-ops-it-infrastructure",
      "adapters/orchestration": "scheme-enterprise-ops-it-infrastructure",
      "adapters/ingest": "scheme-enterprise-ops-it-infrastructure",
      "adapters/integration": "scheme-enterprise-ops-it-infrastructure",
      "adapters/retrieval/sqlite-vec": "scheme-enterprise-ops-it-data",
      "adapters/retrieval/tantivy": "scheme-enterprise-ops-it-data",
      "adapters/retrieval/cross": "scheme-enterprise-ops-it-data",
      "core/domain": "scheme-enterprise-ops-it-development",
      "core/runtime": "scheme-enterprise-ops-it-infrastructure",
      "core/memory": "scheme-enterprise-ops-it-development",
      "core/knowledge": "scheme-enterprise-ops-it-development",
      "core/retrieval": "scheme-enterprise-ops-it-data",
      "core/belief": "scheme-enterprise-ops-it-development",
      "core/hierarchy": "scheme-enterprise-ops-it-development",
      "core/consolidation": "scheme-enterprise-ops-it-development",
      "core/orchestration": "scheme-enterprise-ops-it-development",
      "core/integration": "scheme-enterprise-ops-it-applications",
      "core/eval": "scheme-enterprise-ops-it-development",
      "core/graph-analytics": "scheme-enterprise-ops-it-data",
      "bindings/node": "scheme-enterprise-ops-it-applications",
      "codegraph/": "scheme-enterprise-ops-it-applications",
      "packages/": "scheme-enterprise-ops-it-applications",
      "contracts/": "scheme-enterprise-ops-business-process",
      "docs/": "scheme-enterprise-ops-business-process",
      "examples/": "scheme-enterprise-ops-customer-onboarding",
    };
    const rules = { ...defaultRules, ...(customRules || {}) };

    const entities = this.entities();
    const db = this.openDbWriter();
    const update = db.prepare("UPDATE knowledge_entities SET record_json = ? WHERE id = ?");

    let tagged = 0;
    let untagged = 0;
    const byConcept: Record<string, number> = {};

    for (const entity of entities) {
      const file = entity.sourceRefs?.[0]?.location?.path || "";
      let conceptId: string | null = null;

      // Match the longest path pattern
      let bestMatch = "";
      for (const [pattern, cid] of Object.entries(rules)) {
        if (file.includes(pattern) && pattern.length > bestMatch.length) {
          bestMatch = pattern;
          conceptId = cid;
        }
      }

      // Update the record_json with conceptRefs
      const row = db
        .prepare("SELECT record_json FROM knowledge_entities WHERE id = ?")
        .get(entity.id) as { record_json?: string } | undefined;
      const record = JSON.parse(row?.record_json ?? "{}");

      if (conceptId) {
        record.conceptRefs = [{ id: conceptId }];
        tagged++;
        byConcept[conceptId] = (byConcept[conceptId] || 0) + 1;
      } else {
        record.conceptRefs = [];
        untagged++;
      }

      update.run(JSON.stringify(record), entity.id);
    }

    db.close();
    this.invalidateCache();
    return { tagged, untagged, byConcept };
  }

  /**
   * Returns graph nodes filtered by a taxonomy concept ID.
   * Reads entities whose conceptRefs include the given concept.
   */
  entitiesByConcept(conceptId: string): Entity[] {
    const db = this.openDbReader();
    const rows = db
      .prepare("SELECT record_json FROM knowledge_entities WHERE record_json LIKE ?")
      .all(`%"${conceptId}"%`) as { record_json: string }[];
    db.close();
    return rows.map((r) => {
      const parsed = JSON.parse(r.record_json);
      return {
        id: parsed.id,
        name: parsed.name,
        kind: parsed.kind,
        file: parsed.sourceRefs?.[0]?.path || "",
        sourceRefs: parsed.sourceRefs || [],
        conceptRefs: parsed.conceptRefs || [],
      } as Entity;
    });
  }
}

/** Parses a record_json column, tolerating missing/empty values. */
function parseRecord(json: string): Record<string, unknown> {
  if (!json || json.length === 0) return {};
  try {
    return JSON.parse(json) as Record<string, unknown>;
  } catch {
    return {};
  }
}

// Single process-wide instance.
const dbPath = defaultDbPath();
export const engine = new CodegraphEngine(dbPath);
