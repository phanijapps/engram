import { useEffect, useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { KnowledgeGraph } from "@/components/knowledge-graph";
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
  const [data, setData] = useState<GraphResponse | null>(null);
  const [error, setError] = useState("");
  const [showAllKinds, setShowAllKinds] = useState(false);

  useEffect(() => {
    fetch("/knowledge/graph-data", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ scope: SCOPE, limit: 500, kinds: showAllKinds ? ["*"] : undefined }),
    })
      .then((r) => r.json() as Promise<GraphResponse>)
      .then(setData)
      .catch((e) => setError(String(e)));
  }, [showAllKinds]);

  if (error) return <div className="p-4 text-sm text-destructive">{error}</div>;
  if (!data) return <div className="p-4 text-sm text-muted-foreground">Loading graph…</div>;

  return (
    <div className="flex h-[calc(100vh-3.5rem)] flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b px-4 py-2">
        <Badge variant="outline">{data.nodes.length} nodes</Badge>
        <Badge variant="outline">{data.edges.length} edges</Badge>
        {data.capped && (
          <Badge variant="secondary">top 500 of {data.total.toLocaleString()}</Badge>
        )}
        <div className="ml-auto flex items-center gap-2">
          <Button
            variant={showAllKinds ? "secondary" : "outline"}
            size="sm"
            onClick={() => setShowAllKinds((v) => !v)}
            title={showAllKinds ? "Showing all entity kinds" : "Showing structural kinds only"}
          >
            {showAllKinds ? "All kinds" : "Structural only"}
          </Button>
        </div>
      </div>
      <div className="min-h-0 flex-1">
        {data.nodes.length === 0 ? (
          <div className="p-4 text-sm text-muted-foreground">
            No entities in scope yet — index a repository from the Dashboard.
          </div>
        ) : (
          <KnowledgeGraph nodes={data.nodes} edges={data.edges} />
        )}
      </div>
    </div>
  );
}
