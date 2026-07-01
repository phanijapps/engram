import { useEffect, useState } from "react";
import { Graph3D, buildGraphData, type RawEntity, type RawRelationship } from "@/Graph3D";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { SCOPE } from "@/lib/constants";

type GraphNode = { id: string; name: string; kind: string; graphId: string; degree: number };
type GraphEdge = { subject: string; predicate: string; object: string };
type GraphResponse = { nodes: GraphNode[]; edges: GraphEdge[]; total: number; capped: boolean };

export function Graph() {
  const [data, setData] = useState<{ entities: RawEntity[]; relationships: RawRelationship[] } | null>(null);
  const [meta, setMeta] = useState<{ total: number; capped: boolean } | null>(null);
  const [error, setError] = useState("");

  useEffect(() => {
    fetch("/knowledge/graph-data", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ scope: SCOPE, limit: 500 }),
    })
      .then((r) => r.json() as Promise<GraphResponse>)
      .then((d) => {
        setMeta({ total: d.total, capped: d.capped });
        // Map lightweight graph data to the shapes Graph3D expects.
        const entities: RawEntity[] = d.nodes.map((n) => ({
          id: n.id,
          name: n.name,
          kind: n.kind,
        }));
        const relationships: RawRelationship[] = d.edges.map((e) => ({
          subject: { id: e.subject },
          predicate: e.predicate,
          object: { id: e.object },
        }));
        setData({ entities, relationships });
      })
      .catch((e) => setError(String(e)));
  }, []);

  if (error) return <div className="p-4 text-sm text-destructive">{error}</div>;
  if (!data) return <div className="p-4 text-sm text-muted-foreground">Loading graph…</div>;

  const graphData = buildGraphData(data.entities, data.relationships);

  return (
    <div className="flex h-[calc(100vh-3.5rem)] flex-col">
      <div className="flex items-center gap-2 border-b px-4 py-2">
        <Badge variant="outline">{data.entities.length} nodes</Badge>
        <Badge variant="outline">{data.relationships.length} edges</Badge>
        {meta?.capped && (
          <Badge variant="secondary">top 500 of {meta.total.toLocaleString()}</Badge>
        )}
        <div className="ml-auto">
          <Button variant="ghost" size="sm" onClick={() => window.location.reload()}>
            Refresh
          </Button>
        </div>
      </div>
      <div className="flex-1 overflow-hidden">
        <Graph3D data={graphData} />
      </div>
    </div>
  );
}
