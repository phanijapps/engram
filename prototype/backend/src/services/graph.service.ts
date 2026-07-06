// Graph visualization computation for the explorer.
//
// Pure projection of raw entities + relationships into the lightweight node/edge
// shape the UI renders: degree computation, generic-name deprioritization, and
// top-N filtering. No transport, no HTTP — the route loads the records and
// hands them here.

type Entity = Record<string, unknown>;
type Relationship = Record<string, unknown>;

export type GraphNode = {
  id: string;
  name: string;
  kind: string;
  graphId: string;
  degree: number;
  sourcePath: string;
  repo: string;
  aliases: string[];
  confidence?: number;
};

export type GraphEdge = {
  subject: string;
  predicate: string;
  object: string;
};

export type GraphData = {
  nodes: GraphNode[];
  edges: GraphEdge[];
  total: number;
  capped: boolean;
  error?: string;
};

const GENERIC = new Set([
  "implementation", "operator", "detail", "details", "utility", "utilities", "util", "utils",
  "helper", "helpers", "base", "type", "types", "value", "values", "data", "info", "default",
  "custom", "general", "common", "unknown", "anonymous", "foo", "bar", "test", "tests",
  "sample", "example", "item", "items", "list", "map", "set", "vec", "ptr",
]);

const isGenericName = (name: string): boolean =>
  name.trim().length < 3 || GENERIC.has(name.toLowerCase());

/**
 * Projects entities + relationships into a degree-ranked, top-N node/edge set
 * for rendering. `kinds === null` means "all kinds"; otherwise entities are
 * filtered to the given kinds. Pure: identical inputs yield identical outputs.
 */
export function computeGraphData(
  entities: Entity[],
  relationships: Relationship[],
  opts: { kinds: string[] | null; maxNodes: number },
): GraphData {
  const { kinds, maxNodes } = opts;

  // Filter entities by kind if specified.
  const filteredEnts = kinds
    ? entities.filter((e) => kinds.includes(String(e.kind ?? "unknown")))
    : entities;

  // Compute degree per entity (from filtered set).
  const degree = new Map<string, number>();
  for (const r of relationships) {
    const s = (r.subject as Record<string, unknown> | undefined)?.id;
    const o = (r.object as Record<string, unknown> | undefined)?.id;
    if (typeof s === "string") degree.set(s, (degree.get(s) ?? 0) + 1);
    if (typeof o === "string") degree.set(o, (degree.get(o) ?? 0) + 1);
  }

  // Top-N: non-generic first, then by degree.
  const ranked = filteredEnts
    .map((e) => ({
      id: String(e.id ?? ""),
      deg: degree.get(String(e.id ?? "")) ?? 0,
      gen: isGenericName(String(e.name ?? "")),
    }))
    .sort((a, b) => Number(a.gen) - Number(b.gen) || b.deg - a.deg);
  const topIds = new Set(ranked.slice(0, maxNodes).map((x) => x.id));
  const entByid = new Map(filteredEnts.map((e) => [String(e.id ?? ""), e]));
  const nodes: GraphNode[] = [...topIds]
    .map((id) => entByid.get(id))
    .filter((e): e is Entity => !!e)
    .map((e) => {
      const refs = (e.sourceRefs as Array<Record<string, unknown>> | undefined) ?? [];
      const sourcePath = refs
        .map((r) => (r.location as Record<string, unknown> | undefined)?.path)
        .find((p): p is string => typeof p === "string" && p.length > 0);
      const prov = e.provenance as Record<string, unknown> | undefined;
      return {
        id: String(e.id ?? ""),
        name: String(e.name ?? ""),
        kind: String(e.kind ?? "unknown"),
        graphId: String(e.graphId ?? ""),
        degree: degree.get(String(e.id ?? "")) ?? 0,
        sourcePath: sourcePath ?? "",
        repo: typeof prov?.source === "string" ? (prov.source as string) : "",
        aliases: Array.isArray(e.aliases) ? (e.aliases as string[]).slice(0, 8) : [],
        confidence: typeof prov?.confidence === "number" ? (prov.confidence as number) : undefined,
      };
    });
  const nodeIds = new Set(nodes.map((n) => n.id));
  const edges: GraphEdge[] = relationships
    .filter((r) => {
      const s = String((r.subject as Record<string, unknown> | undefined)?.id ?? "");
      const o = String((r.object as Record<string, unknown> | undefined)?.id ?? "");
      return s && o && nodeIds.has(s) && nodeIds.has(o);
    })
    .slice(0, 2000)
    .map((r) => ({
      subject: String((r.subject as Record<string, unknown> | undefined)?.id ?? ""),
      predicate: String(r.predicate ?? ""),
      object: String((r.object as Record<string, unknown> | undefined)?.id ?? ""),
    }));
  return { nodes, edges, total: filteredEnts.length, capped: filteredEnts.length > maxNodes };
}
