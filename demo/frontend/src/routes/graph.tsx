import { useEffect, useState } from "react";
import { Graph3D, buildGraphData, type RawEntity, type RawRelationship } from "@/Graph3D";
import { Card, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { SCOPE } from "@/lib/constants";

type Overview = {
  entities: RawEntity[];
  relationships: RawRelationship[];
};

export function Graph() {
  const [data, setData] = useState<Overview | null>(null);
  const [capped, setCapped] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    fetch("/knowledge/overview", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ scope: SCOPE }),
    })
      .then((r) => r.json() as Promise<Overview>)
      .then((d) => {
        // Client-side cap: top 500 entities by degree, edges between them.
        // Prevents rendering 26K nodes (the force-graph death scenario).
        const degree = new Map<string, number>();
        for (const r of d.relationships) {
          const s = r.subject?.id;
          const o = r.object?.id;
          if (s) degree.set(s, (degree.get(s) ?? 0) + 1);
          if (o) degree.set(o, (degree.get(o) ?? 0) + 1);
        }
        const topIds = new Set(
          [...degree.entries()]
            .sort((a, b) => b[1] - a[1])
            .slice(0, 500)
            .map(([id]) => id),
        );
        if (d.entities.length > 500) {
          const cappedEntities = d.entities.filter((e) => topIds.has(e.id));
          const cappedRels = d.relationships.filter(
            (r) => topIds.has(r.subject?.id ?? "") && topIds.has(r.object?.id ?? ""),
          );
          setData({ entities: cappedEntities, relationships: cappedRels });
          setCapped(true);
        } else {
          setData(d);
          setCapped(false);
        }
      })
      .catch((e) => setError(String(e)));
  }, []);

  if (error) return <div className="p-4 text-sm text-destructive">{error}</div>;
  if (!data) return <div className="p-4 text-sm text-muted-foreground">Loading graph…</div>;

  const graphData = buildGraphData(data.entities, data.relationships);

  return (
    <div className="flex h-[calc(100vh-3.5rem)] flex-col">
      <div className="flex items-center gap-2 border-b px-4 py-2">
        <Badge variant="outline">{data.entities.length} entities</Badge>
        <Badge variant="outline">{data.relationships.length} edges</Badge>
        {capped && (
          <Badge variant="secondary">top 500 by degree (of full graph)</Badge>
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
