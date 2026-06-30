import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Progress } from "@/components/ui/progress";
import { buildGraphData, type RawEntity, type RawRelationship } from "@/Graph3D";
import { Graph3DCard } from "@/components/graph-3d-card";

type Status = "idle" | "scanning" | "done" | "error";

type ScanEvent =
  | {
      type: "progress";
      index: number;
      total: number;
      file: string;
      unchanged?: boolean;
      entities?: RawEntity[];
      relationships?: RawRelationship[];
      llm?: string;
    }
  | { type: "skip"; file: string; reason?: string }
  | { type: "error"; file: string; message: string }
  | { type: "done"; summary?: Record<string, number> };

export function RepoIndex() {
  const [path, setPath] = useState("");
  const [enhance, setEnhance] = useState(false);
  const [status, setStatus] = useState<Status>("idle");
  const [progress, setProgress] = useState<{ index: number; total: number; file: string } | null>(null);
  const [summary, setSummary] = useState<Record<string, number> | null>(null);
  const [llmUnavailable, setLlmUnavailable] = useState(false);
  const [skips, setSkips] = useState<string[]>([]);
  const [errors, setErrors] = useState<string[]>([]);
  const [graph, setGraph] = useState<{ entities: RawEntity[]; relationships: RawRelationship[] }>({
    entities: [],
    relationships: [],
  });
  const [error, setError] = useState("");

  const scan = async () => {
    if (!path.trim()) return;
    setStatus("scanning");
    setProgress(null);
    setSummary(null);
    setLlmUnavailable(false);
    setSkips([]);
    setErrors([]);
    setGraph({ entities: [], relationships: [] });
    setError("");

    const ents: RawEntity[] = [];
    const rels: RawRelationship[] = [];

    try {
      const res = await fetch("/ingest/scan", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ path, enhance }),
      });
      if (!res.ok || !res.body) throw new Error(`${res.status} ${await res.text()}`);

      const reader = res.body.getReader();
      const decoder = new TextDecoder();
      let buffer = "";
      for (;;) {
        const { done, value } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split("\n");
        buffer = lines.pop() ?? "";
        for (const line of lines) {
          if (!line.trim()) continue;
          let evt: ScanEvent;
          try {
            evt = JSON.parse(line) as ScanEvent;
          } catch {
            continue;
          }
          if (evt.type === "progress") {
            if (Array.isArray(evt.entities)) ents.push(...evt.entities);
            if (Array.isArray(evt.relationships)) rels.push(...evt.relationships);
            setProgress({ index: evt.index, total: evt.total, file: evt.file });
            setGraph({ entities: [...ents], relationships: [...rels] });
            if (evt.llm === "unavailable") setLlmUnavailable(true);
          } else if (evt.type === "skip") {
            setSkips((s) => [...s, `${evt.file} (${evt.reason ?? "skipped"})`].slice(-12));
          } else if (evt.type === "error") {
            setErrors((e) => [...e, `${evt.file}: ${evt.message}`].slice(-12));
          } else if (evt.type === "done") {
            setSummary(
              evt.summary ?? {
                ingested: 0,
                unchanged: 0,
                skipped: 0,
                entities: ents.length,
                relationships: rels.length,
                errors: 0,
              }
            );
          }
        }
      }
      setStatus("done");
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
      setStatus("error");
    }
  };

  const graphData = graph.entities.length
    ? buildGraphData(graph.entities, graph.relationships)
    : null;
  const pct = progress && progress.total ? Math.round((progress.index / progress.total) * 100) : null;

  return (
    <Card>
      <CardHeader>
        <CardTitle>Index a folder or repo</CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="flex flex-wrap items-center gap-2">
          <Input
            placeholder="/absolute/path/to/repo"
            value={path}
            onChange={(e) => setPath(e.target.value)}
            className="min-w-[20rem]"
          />
          <div className="flex items-center gap-2">
            <Switch checked={enhance} onCheckedChange={setEnhance} id="idx-enhance" />
            <Label htmlFor="idx-enhance">LLM enhance</Label>
          </div>
          <Button onClick={scan} disabled={!path.trim() || status === "scanning"}>
            {status === "scanning" ? "Indexing…" : "Index"}
          </Button>
        </div>
        {error && <p className="text-sm text-destructive">{error}</p>}
        {llmUnavailable && (
          <p className="text-sm text-destructive">
            LLM unavailable (set .env creds) — scan ran deterministic-only
          </p>
        )}
        {status === "scanning" && progress && (
          <div className="space-y-1">
            <Progress value={pct ?? 0} />
            <p className="text-xs text-muted-foreground">
              {pct}% · {progress.index}/{progress.total} · <code>{progress.file}</code>
            </p>
          </div>
        )}
        {summary && (
          <p className="text-sm text-muted-foreground">
            Indexed <strong>{summary.ingested}</strong> file(s) · {summary.unchanged} unchanged
            · {summary.skipped} skipped · {summary.entities} entities ·{" "}
            {summary.relationships} edges
            {(summary.llmEntities || summary.llmRelationships)
              ? ` · LLM +${summary.llmEntities} entities · +${summary.llmRelationships} edges`
              : ""}
            {summary.errors ? ` · ${summary.errors} errors` : ""}
          </p>
        )}
        {skips.length > 0 && (
          <details className="text-sm">
            <summary className="cursor-pointer text-muted-foreground">Skipped ({skips.length})</summary>
            <ul className="mt-1 space-y-0.5 text-xs text-muted-foreground">
              {skips.map((s, i) => <li key={i}>{s}</li>)}
            </ul>
          </details>
        )}
        {errors.length > 0 && (
          <details className="text-sm">
            <summary className="cursor-pointer text-destructive">Errors ({errors.length})</summary>
            <ul className="mt-1 space-y-0.5 text-xs text-destructive">
              {errors.map((s, i) => <li key={i}>{s}</li>)}
            </ul>
          </details>
        )}
        <Graph3DCard data={graphData} />
      </CardContent>
    </Card>
  );
}
