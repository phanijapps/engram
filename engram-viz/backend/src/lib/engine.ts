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
}

function defaultDbPath(): string {
  const fromEnv = process.env.ENGRAM_DB;
  if (fromEnv && fromEnv.length > 0) return fromEnv;
  return path.join(os.homedir(), ".engram", "codegraph-mem-alpha.db");
}

class CodegraphEngine {
  readonly scope: Scope = { tenant: "default", workspace: "codegraph" };
  private readonly native: NativeEngine;

  // Caches rebuilt lazily; invalidated by invalidateCache() after a scan.
  private entityCache: Entity[] | null = null;
  private relCache: Relationship[] | null = null;
  private chunkCache: Chunk[] | null = null;
  private entityById: Map<string, Entity> | null = null;
  private entityByName: Map<string, Entity> | null = null;
  private entityChunkIndex: Map<string, Chunk> | null = null;
  private lexicalReady = false;

  constructor(dbPath: string) {
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

  invalidateCache(): void {
    this.entityCache = null;
    this.relCache = null;
    this.chunkCache = null;
    this.entityById = null;
    this.entityByName = null;
    this.entityChunkIndex = null;
    // Rebuild the lexical index so search reflects the new data.
    this.lexicalReady = false;
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
}

// Single process-wide instance.
const dbPath = defaultDbPath();
export const engine = new CodegraphEngine(dbPath);
