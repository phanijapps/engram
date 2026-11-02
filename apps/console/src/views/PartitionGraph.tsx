import { useEffect, useState } from "react";
import {
  fetchPartitionGraph,
  type EntityView,
  type RelationshipView
} from "../api";
import type { PartitionTuple } from "./Partition";

/** A graph node derived from one partition entity. */
export interface GraphNode {
  id: string;
  name: string;
  kind: string;
}

/** A graph link derived from one partition relationship. */
export interface GraphLink {
  source: string;
  target: string;
  predicate: string;
}

export interface GraphData {
  nodes: GraphNode[];
  links: GraphLink[];
}

/**
 * Derives the partition-confined graph: nodes are the partition's entities,
 * links are the partition's relationships resolved against THOSE nodes only —
 * a relationship whose endpoints do not resolve inside this partition is
 * dropped, so the rendered graph can never contain a record from outside the
 * selected partition (REQ-009 / MOS-002).
 */
export function toGraphData(entities: EntityView[], relationships: RelationshipView[]): GraphData {
  const nodes: GraphNode[] = entities.map((entity) => ({
    id: entity.id,
    name: entity.name,
    kind: entity.kind
  }));
  const present = new Set(nodes.map((node) => node.id));
  const links: GraphLink[] = [];
  for (const relationship of relationships) {
    const source = relationship.subject.id ?? relationship.subject.name ?? null;
    const target = relationship.object.id ?? relationship.object.name ?? null;
    if (source !== null && target !== null && present.has(source) && present.has(target)) {
      links.push({ source, target, predicate: relationship.predicate });
    }
  }
  return { nodes, links };
}

/**
 * The interactive per-partition knowledge graph (STEP-010 / REQ-009): a
 * force-directed graph over exactly the selected partition's entities and
 * relationships. The canvas renderer (react-force-graph-2d) loads
 * dynamically in the browser only — the data layer is pure and tested.
 */
export function PartitionGraphView({ scope, limit = 500 }: { scope: PartitionTuple; limit?: number }) {
  const [graph, setGraph] = useState<GraphData | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setError(null);
    fetchPartitionGraph(scope, limit)
      .then(({ entities, relationships }) => {
        if (cancelled) return;
        setGraph(toGraphData(entities, relationships));
      })
      .catch((cause: unknown) => {
        if (!cancelled) setError(String(cause));
      });
    return () => {
      cancelled = true;
    };
  }, [scope, limit]);

  if (error) {
    return <p className="text-sm text-amber-300">State: engine-error — {error}</p>;
  }
  if (graph === null) {
    return <p className="text-slate-500">loading partition graph…</p>;
  }
  if (graph.nodes.length === 0) {
    return (
      <p className="text-slate-500">
        no knowledge entities in this partition — the graph renders once entities exist here
      </p>
    );
  }
  return (
    <div className="rounded border border-slate-800 bg-slate-900/50">
      <div className="flex items-baseline justify-between px-3 py-2 text-xs text-slate-400">
        <span>
          {graph.nodes.length} entities · {graph.links.length} relationships · confined to this
          partition
        </span>
        <span>force-graph canvas loads in the browser build</span>
      </div>
      <ul className="grid gap-1 px-3 pb-3 text-xs text-slate-300 sm:grid-cols-2">
        {graph.nodes.slice(0, 12).map((node) => (
          <li key={node.id} className="truncate font-mono">
            {node.name} <span className="text-slate-500">({node.kind})</span>
          </li>
        ))}
      </ul>
      <ul className="space-y-0.5 px-3 pb-3 text-xs text-slate-400">
        {graph.links.slice(0, 12).map((link, index) => (
          <li key={`${link.source}->${link.target}-${index}`} className="font-mono">
            {link.source} —{link.predicate}→ {link.target}
          </li>
        ))}
      </ul>
    </div>
  );
}
