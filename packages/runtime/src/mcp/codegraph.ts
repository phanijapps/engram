/**
 * Pure-TypeScript codegraph algorithms over a flat edge list (Tier-3 MCP tools).
 *
 * These mirror the call-edge analyses the Rust codegraph crates provide
 * (dead-code / blast-radius over call edges), but run client-side over the
 * relationship list returned by `listRelationships`. They are deliberately
 * dependency-free and operate on a flattened edge list where `subject` /
 * `object` are already strings (a symbol name, or its entity id when no name).
 *
 * The flattening lives in the MCP tool handlers (`tools.ts`): each
 * `KnowledgeRelationship` is mapped to `{ subject, predicate, object }` by
 * reading `rel.subject.name ?? rel.subject.id` (and likewise for `object`).
 */

/** A flattened directed edge: `subject -[predicate]-> object`. */
export interface CodeEdge {
  subject: string;
  predicate: string;
  object: string;
}

/** A named entity for dead-code analysis (`{ name, kind }` from a KnowledgeEntity). */
export interface CodeEntity {
  name: string;
  kind: string;
}

/** A single hop in a symbol-context walk. */
export interface ContextHop {
  depth: number;
  direction: "caller" | "callee";
  symbol: string;
  via: string;
}

/** Result of a symbol-context walk: the immediate + transitive callers/callees. */
export interface SymbolContext {
  callers: string[];
  callees: string[];
  context: ContextHop[];
}

/**
 * Resolve a user-supplied (possibly bare) symbol query to the qualified NAME(s)
 * of matching entities (RFC-0020 T7). After T1, code entity names are qualified
 * (`{repo}/{path}::{bare}`), but users type bare names. This bridges that gap: a
 * query matches an entity name when
 *   - the query equals the name verbatim (user typed the qualified form), or
 *   - the name ends with `"::" + query` (a bare symbol within a path-qualified
 *     name, e.g. query `parse_symbol` → `my-repo/src/lib.rs::parse_symbol`), or
 *   - the name ends with `"/" + query` (the last path segment matches, e.g.
 *     query `lib` → `my-repo/lib`).
 *
 * Returns the matching entity NAMES (not ids) so the result can seed the
 * name-keyed adjacency maps in `symbolContextBFS` / `changeImpactBFS`. A bare
 * suffix WITHOUT a leading `::` / `/` delimiter never matches (`foobar` is NOT
 * a suffix match for query `bar`), so the resolver does not flood on short
 * substrings.
 */
export function resolveSymbolNames(
  names: Iterable<string>,
  query: string,
): string[] {
  const out: string[] = [];
  const dcSuffix = "::" + query;
  const slashSuffix = "/" + query;
  for (const name of names) {
    if (name === query || name.endsWith(dcSuffix) || name.endsWith(slashSuffix)) {
      out.push(name);
    }
  }
  return out;
}

/**
 * BFS from `symbol` in both directions, up to `depth` hops, capped at `cap`
 * discovered symbols per direction.
 *
 * - **callees** (forward): what `symbol` points to — edges where it is the
 *   `subject` (e.g. what it calls / depends on).
 * - **callers** (reverse): who points to `symbol` — edges where it is the
 *   `object` (e.g. who calls / depends on it).
 */
export function symbolContextBFS(
  edges: CodeEdge[],
  symbol: string,
  depth: number,
  cap: number,
): SymbolContext {
  const forward = buildAdjacency(edges, (e) => e.subject, (e) => e.object);
  const reverse = buildAdjacency(edges, (e) => e.object, (e) => e.subject);

  const callees = bfs(forward, symbol, depth, cap);
  const callers = bfs(reverse, symbol, depth, cap);

  const context: ContextHop[] = [
    ...callees.hops.map((h) => ({ ...h, direction: "callee" as const })),
    ...callers.hops.map((h) => ({ ...h, direction: "caller" as const })),
  ].sort((a, b) => a.depth - b.depth);

  return { callers: callers.nodes, callees: callees.nodes, context };
}

/**
 * Reverse BFS — who depends on `target` (upstream callers). Returns one row per
 * discovered caller with its hop depth and the predicate it reaches via.
 * Used for change-impact / blast-radius: editing `target` ripples to every
 * returned caller.
 */
export function changeImpactBFS(
  edges: CodeEdge[],
  target: string,
  depth: number,
  cap: number,
): Array<{ depth: number; caller: string; via: string }> {
  const reverse = buildAdjacency(edges, (e) => e.object, (e) => e.subject);
  const { hops } = bfs(reverse, target, depth, cap);
  return hops.map((h) => ({ depth: h.depth, caller: h.symbol, via: h.via }));
}

/**
 * Multi-seed symbol context (RFC-0020 T7): runs `symbolContextBFS` from each of
 * `symbols` and unions callers/callees/context, de-duped. Used by the
 * `symbol_context` MCP tool when a bare query resolves to multiple qualified
 * names. Callers and callees are de-duped by name (first-seen order); context
 * hops are de-duped by `${direction}:${symbol}:${via}` (the same caller reached
 * via the same predicate from two seeds is one hop), then re-sorted by depth.
 * With a single seed this is identical to `symbolContextBFS`.
 */
export function symbolContextBFSMulti(
  edges: CodeEdge[],
  symbols: string[],
  depth: number,
  cap: number,
): SymbolContext {
  const callers: string[] = [];
  const callees: string[] = [];
  const context: ContextHop[] = [];
  const seenCallers = new Set<string>();
  const seenCallees = new Set<string>();
  const seenHops = new Set<string>();
  for (const symbol of symbols) {
    const result = symbolContextBFS(edges, symbol, depth, cap);
    for (const c of result.callers) {
      if (!seenCallers.has(c)) {
        seenCallers.add(c);
        callers.push(c);
      }
    }
    for (const c of result.callees) {
      if (!seenCallees.has(c)) {
        seenCallees.add(c);
        callees.push(c);
      }
    }
    for (const hop of result.context) {
      const key = `${hop.direction}:${hop.symbol}:${hop.via}`;
      if (!seenHops.has(key)) {
        seenHops.add(key);
        context.push(hop);
      }
    }
  }
  context.sort((a, b) => a.depth - b.depth);
  return { callers, callees, context };
}

/**
 * Multi-seed change impact (RFC-0020 T7): runs `changeImpactBFS` from each of
 * `targets` and unions the blast radius, de-duped by `${caller}:${via}`. The
 * union is re-sorted by depth (then caller) for deterministic output. With a
 * single target this is identical to `changeImpactBFS`.
 */
export function changeImpactBFSMulti(
  edges: CodeEdge[],
  targets: string[],
  depth: number,
  cap: number,
): Array<{ depth: number; caller: string; via: string }> {
  const out: Array<{ depth: number; caller: string; via: string }> = [];
  const seen = new Set<string>();
  for (const target of targets) {
    for (const row of changeImpactBFS(edges, target, depth, cap)) {
      const key = `${row.caller}:${row.via}`;
      if (!seen.has(key)) {
        seen.add(key);
        out.push(row);
      }
    }
  }
  out.sort((a, b) => a.depth - b.depth || a.caller.localeCompare(b.caller));
  return out;
}

/**
 * Dead-code detection: entities with zero incoming edges (nothing references
 * them). Entry points (e.g. `main`) are flagged too — the caller must interpret
 * the result, same as the Rust codegraph dead-code query.
 */
export function deadCode(
  edges: CodeEdge[],
  entities: CodeEntity[],
): CodeEntity[] {
  const incoming = new Set(edges.map((e) => e.object));
  return entities.filter((entity) => !incoming.has(entity.name));
}

/**
 * Degree centrality: the top symbols by total degree (in + out). `inDegree` is
 * the count of edges pointing at a symbol; `outDegree` is the count leaving it.
 */
export function centralSymbols(
  edges: CodeEdge[],
  limit: number,
): Array<{ name: string; inDegree: number; outDegree: number; totalDegree: number }> {
  const inDeg = new Map<string, number>();
  const outDeg = new Map<string, number>();

  for (const edge of edges) {
    outDeg.set(edge.subject, (outDeg.get(edge.subject) ?? 0) + 1);
    inDeg.set(edge.object, (inDeg.get(edge.object) ?? 0) + 1);
  }

  const symbols = new Set<string>([...inDeg.keys(), ...outDeg.keys()]);
  const rows = [...symbols].map((name) => {
    const inDegree = inDeg.get(name) ?? 0;
    const outDegree = outDeg.get(name) ?? 0;
    return { name, inDegree, outDegree, totalDegree: inDegree + outDegree };
  });
  rows.sort((a, b) => b.totalDegree - a.totalDegree || a.name.localeCompare(b.name));
  return rows.slice(0, limit);
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

type Adjacency = Map<string, Array<{ node: string; via: string }>>;

/** Builds an adjacency map keyed by `keyOf(edge)` → list of `valOf(edge)` values. */
function buildAdjacency(
  edges: CodeEdge[],
  keyOf: (e: CodeEdge) => string,
  valOf: (e: CodeEdge) => string,
): Adjacency {
  const map: Adjacency = new Map();
  for (const edge of edges) {
    const key = keyOf(edge);
    const list = map.get(key);
    const entry = { node: valOf(edge), via: edge.predicate };
    if (list) {
      list.push(entry);
    } else {
      map.set(key, [entry]);
    }
  }
  return map;
}

/** A discovered node in a BFS walk. */
interface BfsResult {
  nodes: string[];
  hops: Array<{ depth: number; symbol: string; via: string }>;
}

/** Generic BFS over an adjacency map from `start`, up to `depth` hops, capped at
 *  `cap` discovered nodes (excluding the start). Returns the discovered nodes
 *  and the hop each was first reached at. */
function bfs(adjacency: Adjacency, start: string, depth: number, cap: number): BfsResult {
  const nodes: string[] = [];
  const hops: Array<{ depth: number; symbol: string; via: string }> = [];
  const visited = new Set<string>([start]);
  let frontier: Array<{ node: string; via: string }> = adjacency.get(start) ?? [];

  for (let d = 1; d <= depth && frontier.length > 0; d++) {
    const next: Array<{ node: string; via: string }> = [];
    for (const { node, via } of frontier) {
      if (visited.has(node)) continue;
      visited.add(node);
      nodes.push(node);
      hops.push({ depth: d, symbol: node, via });
      if (nodes.length >= cap) {
        return { nodes, hops };
      }
      for (const neighbor of adjacency.get(node) ?? []) {
        if (!visited.has(neighbor.node)) {
          next.push(neighbor);
        }
      }
    }
    frontier = next;
  }
  return { nodes, hops };
}
