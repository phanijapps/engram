import { useEffect, useRef, useState } from "react";
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

/** Kind → color for the canvas nodes (bright hues on the dark shell). */
const KIND_COLORS: Record<string, string> = {
  project: "#38bdf8",
  repository: "#34d399",
  concept: "#c084fc",
  function: "#fbbf24",
  class: "#f472b6",
  module: "#f97316",
  file: "#94a3b8"
};

function colorFor(node: GraphNode): string {
  return KIND_COLORS[node.kind?.toLowerCase?.() ?? ""] ?? "#e2e8f0";
}

/**
 * The interactive per-partition knowledge graph (STEP-010 / REQ-009 /
 * DEC-006): a react-force-graph-2d canvas over exactly the selected
 * partition's entities and relationships. The canvas module loads
 * dynamically (it touches window/canvas at import time, so it loads only in
 * the browser); while it loads — or if it cannot load — the bounded summary
 * list renders under an honest caption. Data derivation stays pure and is
 * what the canvas consumes.
 */
export function PartitionGraphView({ scope, limit = 500 }: { scope: PartitionTuple; limit?: number }) {
  const [graph, setGraph] = useState<GraphData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [ForceGraph, setForceGraph] = useState<React.ComponentType<Record<string, unknown>> | null>(
    null
  );
  const [canvasState, setCanvasState] = useState<"loading" | "ready" | "unavailable">("loading");
  const containerRef = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState<{ width: number; height: number } | null>(null);

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

  useEffect(() => {
    let cancelled = false;
    import("react-force-graph-2d")
      .then((module) => {
        if (cancelled) return;
        setForceGraph(() => module.default as never);
        setCanvasState("ready");
      })
      .catch(() => {
        if (!cancelled) setCanvasState("unavailable");
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    const measure = () => {
      const element = containerRef.current;
      if (element) {
        setSize({ width: element.clientWidth, height: Math.max(320, element.clientWidth * 0.5) });
      }
    };
    measure();
    window.addEventListener("resize", measure);
    return () => window.removeEventListener("resize", measure);
  }, []);

  if (error) {
    return <p className="text-sm text-amber-300">State: engine-error — {error}</p>;
  }
  if (graph === null) {
    return <p className="text-slate-400">loading partition graph…</p>;
  }
  if (graph.nodes.length === 0) {
    return (
      <p className="text-slate-400">
        no knowledge entities in this partition — the graph renders once entities exist here
      </p>
    );
  }

  const canvasReady = canvasState === "ready" && ForceGraph !== null && size !== null;

  return (
    <div ref={containerRef} className="rounded border border-slate-800 bg-slate-900/50">
      <div className="flex items-baseline justify-between px-3 py-2 text-xs text-slate-400">
        <span>
          {graph.nodes.length} entities · {graph.links.length} relationships · confined to this
          partition · drag to pan, scroll to zoom, hover for labels
        </span>
        <span>
          {canvasState === "loading" ? "loading canvas…" : null}
          {canvasState === "unavailable" ? "canvas unavailable — summary view" : null}
          {canvasReady ? "interactive force-graph" : null}
        </span>
      </div>
      {canvasReady ? (
        <ForceGraph
          width={size.width}
          height={size.height}
          graphData={{ nodes: graph.nodes, links: graph.links }}
          nodeLabel={(node: unknown) => {
            const n = node as GraphNode;
            return `${n.name} (${n.kind})`;
          }}
          nodeColor={(node: unknown) => colorFor(node as GraphNode)}
          nodeRelSize={5}
          linkColor={() => "#475569"}
          linkDirectionalParticles={2}
          linkDirectionalParticleWidth={1.5}
          cooldownTicks={120}
        />
      ) : (
        <ul className="grid gap-1 px-3 pb-3 text-xs text-slate-300 sm:grid-cols-2">
          {graph.nodes.slice(0, 24).map((node) => (
            <li key={node.id} className="truncate font-mono">
              {node.name} <span className="text-slate-400">({node.kind})</span>
            </li>
          ))}
        </ul>
      )}
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
