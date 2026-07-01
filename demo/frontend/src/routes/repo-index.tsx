import { useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import {
  buildGraphData,
  type GraphData,
  type RawEntity,
  type RawRelationship,
} from "@/Graph3D";
import { Graph3DCard } from "@/components/graph-3d-card";
import { SCOPE } from "@/lib/constants";

type JobState = {
  status: string; // "running" | "done" | "error" | "unknown"
  currentFile?: string;
  processed?: number;
  ingested?: number;
  unchanged?: number;
  skipped?: number;
  errors?: number;
  summary?: {
    scanned: number;
    ingested: number;
    unchanged: number;
    skipped: number;
    entities: number;
    relationships: number;
    errors: number;
  };
  error?: string;
};

type Overview = {
  entities: RawEntity[];
  relationships: RawRelationship[];
};

export function RepoIndex() {
  const [root, setRoot] = useState("");
  const [force, setForce] = useState(false);
  const [jobId, setJobId] = useState<string | null>(null);
  const [job, setJob] = useState<JobState | null>(null);
  const [graph, setGraph] = useState<GraphData | null>(null);
  const [error, setError] = useState("");
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Poll the job until it settles.
  useEffect(() => {
    if (!jobId) return;
    const tick = async () => {
      try {
        const res = await fetch(`/ingest/jobs/${jobId}`);
        if (!res.ok) throw new Error(`${res.status} ${await res.text()}`);
        const state = (await res.json()) as JobState;
        setJob(state);
        if (state.status === "done" || state.status === "error") {
          if (pollRef.current) clearInterval(pollRef.current);
          pollRef.current = null;
          if (state.status === "done") {
            // Load the freshly-indexed graph for visualization.
            const ovRes = await fetch("/knowledge/overview", {
              method: "POST",
              headers: { "content-type": "application/json" },
              body: JSON.stringify({ scope: SCOPE }),
            });
            if (ovRes.ok) {
              const ov = (await ovRes.json()) as Overview;
              setGraph(buildGraphData(ov.entities, ov.relationships));
            }
          }
        }
      } catch (e) {
        setError(String(e instanceof Error ? e.message : e));
        if (pollRef.current) clearInterval(pollRef.current);
        pollRef.current = null;
      }
    };
    void tick();
    pollRef.current = setInterval(tick, 1200);
    return () => {
      if (pollRef.current) clearInterval(pollRef.current);
      pollRef.current = null;
    };
  }, [jobId]);

  const start = async () => {
    if (!root.trim()) return;
    setError("");
    setJob(null);
    setGraph(null);
    try {
      const res = await fetch("/ingest/jobs", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ root: root.trim(), force }),
      });
      if (!res.ok) throw new Error(`${res.status} ${await res.text()}`);
      setJobId(((await res.json()) as { jobId: string }).jobId);
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
    }
  };

  const running = job?.status === "running";
  const summary = job?.summary;

  return (
    <Card>
      <CardHeader>
        <CardTitle>Index a folder or repo</CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="flex items-center gap-2">
          <Input
            placeholder="/absolute/path/to/repo"
            value={root}
            onChange={(e) => setRoot(e.target.value)}
            className="min-w-[24rem]"
          />
          <div className="flex items-center gap-2">
            <Switch checked={force} onCheckedChange={setForce} id="force" />
            <Label htmlFor="force" className="text-xs text-muted-foreground">
              Force re-index
            </Label>
          </div>
          <Button onClick={start} disabled={!root.trim() || running}>
            {running ? "Indexing…" : "Index"}
          </Button>
        </div>
        <p className="text-xs text-muted-foreground">
          Runs a parallel Rust scan in the background (rayon); progress is polled.
          Deterministic extraction only — use /ingest for LLM-enhanced per-doc
          extraction.
        </p>
        {error && <p className="text-sm text-destructive">{error}</p>}
        {job && (
          <div className="space-y-1">
            <div className="flex items-center gap-2">
              <Badge
                variant={running ? "default" : job.status === "done" ? "outline" : "destructive"}
                className={running ? "animate-pulse" : ""}
              >
                {running ? "indexing" : job.status}
              </Badge>
            </div>
            {(running || job.currentFile) && job.currentFile && (
              <p className="text-xs text-muted-foreground">
                <code>{job.currentFile}</code>
              </p>
            )}
            {(job.processed ?? 0) > 0 && (
              <p className="text-xs text-muted-foreground">
                processed {job.processed} · ingested {job.ingested} · unchanged{" "}
                {job.unchanged} · skipped {job.skipped} · errors {job.errors}
              </p>
            )}
            {summary && (
              <p className="text-sm text-muted-foreground">
                Scanned <strong>{summary.scanned}</strong> · ingested{" "}
                {summary.ingested} · unchanged {summary.unchanged} · skipped{" "}
                {summary.skipped} · {summary.entities} entities ·{" "}
                {summary.relationships} edges
                {summary.errors ? ` · ${summary.errors} errors` : ""}
              </p>
            )}
            {job.error && <p className="text-sm text-destructive">{job.error}</p>}
          </div>
        )}
        <Graph3DCard data={graph} />
      </CardContent>
    </Card>
  );
}
