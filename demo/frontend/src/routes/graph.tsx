import { useEffect, useState } from "react";
import { Graph3D, buildGraphData, type RawEntity, type RawRelationship } from "@/Graph3D";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { SCOPE } from "@/lib/constants";

type GraphNode = {
  id: string;
  name: string;
  kind: string;
  graphId: string;
  degree: number;
  sourcePath?: string;
  repo?: string;
  aliases?: string[];
  confidence?: number;
};
type GraphEdge = { subject: string; predicate: string; object: string };
type GraphResponse = { nodes: GraphNode[]; edges: GraphEdge[]; total: number; capped: boolean };

export function Graph() {
  const [data, setData] = useState<{ entities: RawEntity[]; relationships: RawRelationship[] } | null>(null);
  const [meta, setMeta] = useState<{ total: number; capped: boolean } | null>(null);
  const [error, setError] = useState("");
  const [showAllKinds, setShowAllKinds] = useState(false);

  useEffect(() => {
    fetch("/knowledge/graph-data", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ scope: SCOPE, limit: 500, kinds: showAllKinds ? ["*"] : undefined }),
    })
      .then((r) => r.json() as Promise<GraphResponse>)
      .then((d) => {
        setMeta({ total: d.total, capped: d.capped });
        const entities: RawEntity[] = d.nodes.map((n) => ({
          id: n.id,
          name: n.name,
          kind: n.kind,
          sourcePath: n.sourcePath,
          repo: n.repo,
          aliases: n.aliases,
          provenance: { source: n.repo, confidence: n.confidence },
        }));
        const relationships: RawRelationship[] = d.edges.map((e) => ({
          subject: { id: e.subject },
          predicate: e.predicate,
          object: { id: e.object },
        }));
        setData({ entities, relationships });
      })
      .catch((e) => setError(String(e)));
  }, [showAllKinds]);

  if (error) return <div className="p-4 text-sm text-destructive">{error}</div>;
  if (!data) return <div className="p-4 text-sm text-muted-foreground">Loading graph…</div>;

  const graphData = buildGraphData(data.entities, data.relationships);

  return (
    <div className="flex h-[calc(100vh-3.5rem)] flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b px-4 py-2">
        <Badge variant="outline">{data.entities.length} nodes</Badge>
        <Badge variant="outline">{data.relationships.length} edges</Badge>
        {meta?.capped && (
          <Badge variant="secondary">top 500 of {meta.total.toLocaleString()}</Badge>
        )}
        <div className="ml-auto flex items-center gap-2">
          <Button
            variant={showAllKinds ? "secondary" : "outline"}
            size="sm"
            onClick={() => setShowAllKinds((v) => !v)}
            title={showAllKinds ? "Showing all entity kinds" : "Showing structural kinds only (repo/module/function/class/…)"}
          >
            {showAllKinds ? "All kinds" : "Structural only"}
          </Button>
          <Button variant="ghost" size="sm" onClick={() => window.location.reload()}>
            Refresh
          </Button>
        </div>
      </div>
      <div className="min-h-0 flex-1">
        <Graph3D data={graphData} />
      </div>
    </div>
  );
}
