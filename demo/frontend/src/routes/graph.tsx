import { useEffect, useMemo, useRef, useState } from "react";
import ForceGraph3D from "react-force-graph-3d";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { SCOPE } from "@/lib/constants";

type GEntity = {
  id: string;
  name: string;
  kind?: string;
  graphId?: string;
  sourceRefs?: { location?: { path?: string } }[];
  provenance?: { source?: string; method?: string; confidence?: number };
  createdAt?: string;
};
type GRel = {
  id: string;
  subject: { id?: string; name?: string; kind?: string };
  predicate: string;
  object: { id?: string; name?: string; kind?: string };
  graphId?: string;
  confidence?: number;
};
type GGraph = { id: string; name: string };
type Overview = { graphs: GGraph[]; entities: GEntity[]; relationships: GRel[] };

const KIND_COLORS: Record<string, string> = {
  function: "#6ea8ff",
  method: "#6ea8ff",
  class: "#b07cff",
  struct: "#b07cff",
  concept: "#7bd88f",
  value_stream: "#ff8a65",
  requirement: "#ff8a65",
  api: "#4dd0e1",
  person: "#f06292",
  tool: "#ffd54f",
  artifact: "#a0e0a0",
};
const PREDICATE_COLORS: Record<string, string> = {
  calls: "#6ea8ff",
  mentions: "#7bd88f",
  defined_in: "#ff8a65",
  satisfies: "#f06292",
  implements: "#b07cff",
};

type FGNode = {
  id: string;
  name: string;
  kind: string;
  graphId?: string;
  val: number;
  color: string;
  entity: GEntity;
};
type FGLink = {
  source: string;
  target: string;
  color: string;
  predicate: string;
};

function buildGraphData(data: Overview) {
  const degree = new Map<string, number>();
  for (const r of data.relationships) {
    const s = r.subject.id;
    const o = r.object.id;
    if (s) degree.set(s, (degree.get(s) ?? 0) + 1);
    if (o) degree.set(o, (degree.get(o) ?? 0) + 1);
  }
  const nodes: FGNode[] = data.entities.map((e) => ({
    id: e.id,
    name: e.name,
    kind: e.kind ?? "unknown",
    graphId: e.graphId,
    val: Math.max(1, degree.get(e.id) ?? 1),
    color: KIND_COLORS[e.kind ?? ""] ?? "#9aa6bd",
    entity: e,
  }));
  const links: FGLink[] = data.relationships
    .filter((r) => r.subject.id && r.object.id)
    .map((r) => ({
      source: r.subject.id!,
      target: r.object.id!,
      color: PREDICATE_COLORS[r.predicate] ?? "#3a4768",
      predicate: r.predicate,
    }));
  return { nodes, links };
}

function IndexDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (v: boolean) => void;
}) {
  const [root, setRoot] = useState("");
  const [force, setForce] = useState(false);
  const [jobId, setJobId] = useState<string | null>(null);
  const [status, setStatus] = useState<string>("");
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  useEffect(() => {
    if (!jobId) return;
    const tick = async () => {
      try {
        const res = await fetch(`/ingest/jobs/${jobId}`);
        const s = (await res.json()) as { status: string; summary?: Record<string, number> };
        setStatus(
          `${s.status}${s.summary ? ` — ${s.summary.ingested} files · ${s.summary.entities} entities` : ""}`,
        );
        if (s.status === "done" || s.status === "error") {
          if (pollRef.current) clearInterval(pollRef.current);
          pollRef.current = null;
        }
      } catch {
        /* ignore */
      }
    };
    void tick();
    pollRef.current = setInterval(tick, 1500);
    return () => {
      if (pollRef.current) clearInterval(pollRef.current);
    };
  }, [jobId]);

  const start = async () => {
    if (!root.trim()) return;
    setStatus("starting…");
    setJobId(null);
    try {
      const res = await fetch("/ingest/jobs", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ root: root.trim(), force }),
      });
      const body = (await res.json()) as { jobId: string };
      setJobId(body.jobId);
    } catch (e) {
      setStatus(String(e));
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Index a repository</DialogTitle>
        </DialogHeader>
        <div className="space-y-3">
          <Input
            placeholder="/absolute/path/to/repo"
            value={root}
            onChange={(e) => setRoot(e.target.value)}
          />
          <div className="flex items-center gap-2">
            <Switch checked={force} onCheckedChange={setForce} id="idx-f" />
            <Label htmlFor="idx-f" className="text-xs text-muted-foreground">
              Force re-index
            </Label>
            <Button onClick={start} disabled={!root.trim() || !!jobId}>
              Index
            </Button>
          </div>
          {status && <p className="text-sm text-muted-foreground">{status}</p>}
          {status.startsWith("done") && (
            <Button
              variant="secondary"
              onClick={() => {
                onOpenChange(false);
                window.location.reload();
              }}
            >
              Close + refresh graph
            </Button>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}

function NodeDetail({ node, relationships }: { node: FGNode; relationships: GRel[] }) {
  const edges = relationships.filter(
    (r) => r.subject.id === node.id || r.object.id === node.id,
  );
  const filePath = node.entity.sourceRefs?.[0]?.location?.path;
  return (
    <Card className="absolute right-3 top-3 z-10 w-80 max-h-[70vh] overflow-auto">
      <CardHeader className="pb-2">
        <div className="flex items-center gap-2">
          <Badge style={{ backgroundColor: node.color, color: "#fff" }}>{node.kind}</Badge>
          <CardTitle className="text-base">{node.name}</CardTitle>
        </div>
      </CardHeader>
      <CardContent className="space-y-2 text-sm">
        {filePath && (
          <div>
            <span className="text-xs text-muted-foreground">File: </span>
            <code>{filePath}</code>
          </div>
        )}
        {node.entity.provenance?.source && (
          <div className="text-xs text-muted-foreground">
            Source: {node.entity.provenance.source}
          </div>
        )}
        {node.entity.createdAt && (
          <div className="text-xs text-muted-foreground">
            Created: {new Date(node.entity.createdAt).toLocaleString()}
          </div>
        )}
        {edges.length > 0 && (
          <div>
            <div className="text-xs font-medium uppercase text-muted-foreground mb-1">
              Relationships ({edges.length})
            </div>
            <div className="space-y-0.5">
              {edges.slice(0, 20).map((r) => (
                <div key={r.id} className="text-xs">
                  <span style={{ color: PREDICATE_COLORS[r.predicate] ?? "#9aa6bd" }}>
                    {r.subject.id === node.id ? "→" : "←"} {r.predicate}{" "}
                  </span>
                  <span className="font-medium">
                    {r.subject.id === node.id ? r.object.name : r.subject.name}
                  </span>
                </div>
              ))}
              {edges.length > 20 && (
                <div className="text-xs text-muted-foreground">+{edges.length - 20} more</div>
              )}
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

export function Graph() {
  const [data, setData] = useState<Overview | null>(null);
  const [selected, setSelected] = useState<FGNode | null>(null);
  const [error, setError] = useState("");
  const [showIndex, setShowIndex] = useState(false);
  const fgRef = useRef<{ zoomToFit: (ms?: number, pad?: number) => void } | null>(null);

  useEffect(() => {
    const load = async () => {
      try {
        const res = await fetch("/knowledge/overview", {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ scope: SCOPE }),
        });
        if (!res.ok) throw new Error(`${res.status}`);
        setData((await res.json()) as Overview);
      } catch (e) {
        setError(String(e instanceof Error ? e.message : e));
      }
    };
    void load();
  }, []);

  const graphData = useMemo(() => (data ? buildGraphData(data) : { nodes: [], links: [] }), [data]);

  useEffect(() => {
    if (graphData.nodes.length > 0) {
      const t = setTimeout(() => {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        (fgRef.current as any)?.zoomToFit(800, 80);
      }, 1500);
      return () => clearTimeout(t);
    }
  }, [graphData]);

  if (error) return <div className="p-4 text-sm text-destructive">{error}</div>;
  if (!data) return <div className="p-4 text-sm text-muted-foreground">Loading graph…</div>;

  return (
    <div className="relative h-[calc(100vh-3.5rem)] w-full bg-[#0c1124]">
      <ForceGraph3D
        ref={fgRef as never}
        graphData={graphData as never}
        nodeLabel="name"
        nodeColor="color"
        nodeVal="val"
        nodeRelSize={5}
        nodeOpacity={0.9}
        linkColor="color"
        linkWidth={0.5}
        linkOpacity={0.4}
        linkDirectionalParticles={2}
        linkDirectionalParticleWidth={0.5}
        linkDirectionalParticleSpeed={0.004}
        linkDirectionalArrowLength={3}
        linkDirectionalArrowRelPos={1}
        cooldownTicks={200}
        onNodeClick={(node: unknown) => {
          setSelected(node as FGNode);
        }}
        onBackgroundClick={() => setSelected(null)}
      />
      {selected && data && <NodeDetail node={selected} relationships={data.relationships} />}
      {/* Top bar */}
      <div className="absolute left-3 top-3 z-10 flex items-center gap-2">
        <Badge variant="outline" className="bg-background/80 backdrop-blur">
          {graphData.nodes.length} nodes · {graphData.links.length} edges
        </Badge>
        <Button variant="secondary" size="sm" onClick={() => window.location.href = "/dashboard"}>
          Dashboard
        </Button>
        <Button variant="secondary" size="sm" onClick={() => setShowIndex(true)}>
          Index repo
        </Button>
      </div>
      {/* Legend */}
      <div className="absolute left-3 bottom-3 z-10 flex flex-col gap-1 rounded-md bg-background/80 p-2 backdrop-blur">
        {Object.entries(KIND_COLORS).slice(0, 6).map(([kind, color]) => (
          <div key={kind} className="flex items-center gap-2 text-xs">
            <span className="h-2 w-2 rounded-full" style={{ backgroundColor: color }} />
            {kind}
          </div>
        ))}
      </div>
      <IndexDialog open={showIndex} onOpenChange={setShowIndex} />
    </div>
  );
}
