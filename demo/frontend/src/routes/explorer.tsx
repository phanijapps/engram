import { useEffect, useMemo, useRef, useState } from "react";
import ForceGraph2D from "react-force-graph-2d";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { SCOPE } from "@/lib/constants";

type OverviewGraph = { id: string; name: string };
type OverviewEntity = { id: string; graphId?: string; kind?: string; name: string };
type OverviewRelationship = {
  id: string;
  graphId?: string;
  subject: { id?: string; name?: string; kind?: string };
  predicate: string;
  object: { id?: string; name?: string; kind?: string };
};
type Overview = {
  graphs: OverviewGraph[];
  entities: OverviewEntity[];
  relationships: OverviewRelationship[];
};

type GNode = {
  id: string;
  name: string;
  kind: string;
  graphId?: string;
  cluster?: boolean;
  count?: number;
  color: string;
};
type GLink = {
  source: string;
  target: string;
  predicate?: string;
  cross?: boolean;
  color: string;
};

// Stable per-repo palette keyed on graph id.
const PALETTE = [
  "#6ea8ff",
  "#7bd88f",
  "#e0b34a",
  "#b07cff",
  "#ff8a65",
  "#4dd0e1",
  "#f06292",
  "#a0e0a0",
];
function colorFor(key: string): string {
  let h = 0;
  for (let i = 0; i < key.length; i++) h = (h * 31 + key.charCodeAt(i)) >>> 0;
  return PALETTE[h % PALETTE.length];
}

function buildHighLevel(data: Overview): { nodes: GNode[]; links: GLink[] } {
  const nodes: GNode[] = data.graphs.map((g) => ({
    id: g.id,
    name: g.name,
    kind: "cluster",
    cluster: true,
    graphId: g.id,
    count: data.entities.filter((e) => e.graphId === g.id).length,
    color: colorFor(g.id),
  }));
  // Cross-repo links: repos that share an entity name.
  const byName = new Map<string, Set<string>>();
  for (const e of data.entities) {
    if (!e.graphId) continue;
    const key = e.name.toLowerCase();
    if (!byName.has(key)) byName.set(key, new Set());
    byName.get(key)!.add(e.graphId);
  }
  const links: GLink[] = [];
  const seen = new Set<string>();
  for (const graphs of byName.values()) {
    if (graphs.size < 2) continue;
    const arr = [...graphs];
    for (let a = 0; a < arr.length; a++) {
      for (let b = a + 1; b < arr.length; b++) {
        const k = [arr[a], arr[b]].sort().join("|");
        if (seen.has(k)) continue;
        seen.add(k);
        links.push({ source: arr[a], target: arr[b], cross: true, color: "#64748b" });
      }
    }
  }
  return { nodes, links };
}

function buildDrillDown(data: Overview, graphId: string): { nodes: GNode[]; links: GLink[] } {
  const repoColor = colorFor(graphId);
  const mine = data.entities.filter((e) => e.graphId === graphId);
  const myIds = new Set(mine.map((e) => e.id));
  const nodes: GNode[] = mine.map((e) => ({ ...e, kind: e.kind ?? "entity", color: repoColor }));

  const links: GLink[] = [];
  // Internal relationships.
  for (const r of data.relationships) {
    if (r.graphId !== graphId) continue;
    const s = r.subject.id;
    const o = r.object.id;
    if (s && o && myIds.has(s) && myIds.has(o)) {
      links.push({ source: s, target: o, predicate: r.predicate, color: "#3a4768" });
    }
  }
  // Cross-repo links: my entities that share a name with another repo's entity.
  for (const e of mine) {
    const key = e.name.toLowerCase();
    for (const other of data.entities) {
      if (other.graphId === graphId || !other.graphId) continue;
      if (other.name.toLowerCase() === key) {
        links.push({ source: e.id, target: other.id, cross: true, color: "#8a6a2a" });
        // Ensure the other-repo entity is a visible node (ghost).
        if (!nodes.some((n) => n.id === other.id)) {
          nodes.push({ ...other, kind: other.kind ?? "entity", color: colorFor(other.graphId) });
        }
      }
    }
  }
  return { nodes, links };
}

export function Explorer() {
  const [data, setData] = useState<Overview | null>(null);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [selected, setSelected] = useState<GNode | null>(null);
  const [error, setError] = useState("");
  const fgRef = useRef<{ zoomToFit: (ms?: number, pad?: number) => void } | null>(null);
  const lastClick = useRef<{ id: string; t: number } | null>(null);

  const load = async () => {
    try {
      const res = await fetch("/knowledge/overview", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ scope: SCOPE }),
      });
      if (!res.ok) throw new Error(`${res.status} ${await res.text()}`);
      setData((await res.json()) as Overview);
      setError("");
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
    }
  };

  useEffect(() => {
    void load();
  }, []);

  const graphData = useMemo(() => {
    if (!data) return { nodes: [] as GNode[], links: [] as GLink[] };
    return expanded ? buildDrillDown(data, expanded) : buildHighLevel(data);
  }, [data, expanded]);

  // Recenter when the view changes.
  useEffect(() => {
    const t = setTimeout(() => fgRef.current?.zoomToFit(400, 60), 600);
    return () => clearTimeout(t);
  }, [graphData]);

  const drawNode = (node: GNode & { x: number; y: number }, ctx: CanvasRenderingContext2D, scale: number) => {
    const r = node.cluster ? 10 + Math.min(16, (node.count ?? 0) / 2) : 5;
    ctx.fillStyle = node.color;
    ctx.beginPath();
    ctx.arc(node.x, node.y, r, 0, 2 * Math.PI);
    ctx.fill();
    if (node.cluster) {
      ctx.strokeStyle = "#0c1124";
      ctx.lineWidth = 2;
      ctx.stroke();
    }
    if (scale > 1.1 || node.cluster) {
      ctx.fillStyle = node.cluster ? "#fff" : "#e6ecff";
      ctx.font = `${node.cluster ? 5 : 4}px sans-serif`;
      ctx.textAlign = "center";
      ctx.fillText(node.name, node.x, node.y + r + 4);
      if (node.cluster && node.count !== undefined) {
        ctx.fillStyle = "#9aa6bd";
        ctx.fillText(`${node.count}`, node.x, node.y + 1.5);
      }
    }
  };

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between">
          <CardTitle>Knowledge graph explorer</CardTitle>
          <div className="flex items-center gap-2">
            {expanded && (
              <Button
                variant="secondary"
                onClick={() => {
                  setExpanded(null);
                  setSelected(null);
                }}
              >
                ← All repos
              </Button>
            )}
            <Button variant="outline" onClick={load}>
              Refresh
            </Button>
          </div>
        </div>
      </CardHeader>
      <CardContent className="space-y-3">
        {error && <p className="text-sm text-destructive">{error}</p>}
        <p className="text-sm text-muted-foreground">
          {expanded
            ? `Drilled into one repo — nodes are its entities; amber links cross into other repos via shared names.`
            : `High level: each circle is a repo/source (size = entity count); grey links connect repos that share an entity name. Double-click a repo to drill in.`}
        </p>
        <div className="h-[560px] overflow-hidden rounded-md border bg-[#0c1124]">
          {/* eslint-disable-next-line @typescript-eslint/no-explicit-any */}
          <ForceGraph2D
            ref={fgRef as any}
            graphData={graphData as any}
            nodeCanvasObject={drawNode as any}
            nodeRelSize={1}
            linkColor={(l: GLink) => l.color}
            linkWidth={(l: GLink) => (l.cross ? 0.5 : 1)}
            onNodeClick={(n: GNode) => {
              const now = Date.now();
              const last = lastClick.current;
              if (last && last.id === n.id && now - last.t < 350) {
                // double-click
                lastClick.current = null;
                if (n.cluster) {
                  setExpanded(n.id);
                  setSelected(null);
                }
              } else {
                lastClick.current = { id: n.id, t: now };
                setSelected(n);
              }
            }}
            onBackgroundClick={() => setSelected(null)}
            cooldownTicks={120}
          />
        </div>
        {selected && (
          <div className="rounded-md border p-3 text-sm">
            <div className="mb-1 flex items-center gap-2">
              <Badge variant={selected.cluster ? "default" : "outline"}>
                {selected.cluster ? "repo" : selected.kind}
              </Badge>
              <span className="font-medium">{selected.name}</span>
            </div>
            <p className="text-xs text-muted-foreground">
              {selected.cluster
                ? `${selected.count} entities`
                : `belongs to ${data?.graphs.find((g) => g.id === selected.graphId)?.name ?? selected.graphId ?? "?"}`}
            </p>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
