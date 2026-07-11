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
  line?: number;
  endLine?: number;
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

class CodegraphEngine {
  readonly scope: Scope = { tenant: "default", workspace: "codegraph" };
  readonly dbPath: string;
  private readonly native: NativeEngine;

  // Caches rebuilt lazily; invalidated by invalidateCache() after a scan.
  private entityCache: Entity[] | null = null;
  private relCache: Relationship[] | null = null;
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

  /** Ensures the BM25 lexical index is built exactly once per process. */
  ensureLexical(): void {
    if (this.lexicalReady) return;
    this.native.indexForSearchJson(this.enc(this.scope));
    this.lexicalReady = true;
  }

  /** Whether the lexical search index is ready (built and not building). */
  get lexicalSearchReady(): boolean {
    return this.lexicalReady;
  }

  /** Whether the lexical search index is currently being built. */
  get lexicalSearchBuilding(): boolean {
    return this.lexicalBuilding;
  }

  /**
   * Pre-warms the lexical search index in a non-blocking background task.
   * Sets the building flag so callers can poll readiness. Safe to call
   * multiple times — the second call is a no-op.
   */
  prewarmLexical(): void {
    if (this.lexicalReady || this.lexicalBuilding) return;
    this.lexicalBuilding = true;
    // Fire-and-forget: the native call runs synchronously on a background
    // thread, but indexForSearchJson itself blocks the Node event loop.
    // Using setImmediate defers it past the current request cycle.
    setImmediate(() => {
      try {
        this.native.indexForSearchJson(this.enc(this.scope));
        this.lexicalReady = true;
      } catch (err) {
        console.error("[engine] lexical pre-warm failed:", err);
      } finally {
        this.lexicalBuilding = false;
      }
    });
  }

  invalidateCache(): void {
    this.entityCache = null;
    this.relCache = null;
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
  communities(maxPasses = 10): Record<string, number> {
    const raw = this.native.callCommunitiesJson(
      this.enc({ scope: this.scope, maxPasses }),
    );
    return JSON.parse(raw) as Record<string, number>;
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

  // --- internals ------------------------------------------------------

  /** Opens a read-only SQLite connection for taxonomy/ontology discovery. */
  private openDbReader(): DatabaseSync {
    return new DatabaseSync(this.dbPath, { readOnly: true });
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
