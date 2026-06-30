import { useState } from "react";
import {
  Graph3D,
  buildGraphData,
  type RawEntity,
  type RawRelationship,
} from "./Graph3D";

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
    }
  | { type: "skip"; file: string; reason?: string }
  | { type: "error"; file: string; message: string }
  | { type: "done"; summary?: Record<string, number> };

export function ScanPanel() {
  const [path, setPath] = useState("");
  const [status, setStatus] = useState<Status>("idle");
  const [progress, setProgress] = useState<{ index: number; total: number; file: string } | null>(null);
  const [summary, setSummary] = useState<Record<string, number> | null>(null);
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
        body: JSON.stringify({ path }),
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
            // Skip a malformed NDJSON line instead of breaking the whole stream.
            continue;
          }
          if (evt.type === "progress") {
            if (Array.isArray(evt.entities)) ents.push(...evt.entities);
            if (Array.isArray(evt.relationships)) rels.push(...evt.relationships);
            setProgress({ index: evt.index, total: evt.total, file: evt.file });
            setGraph({ entities: [...ents], relationships: [...rels] });
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

  const graphData = graph.entities.length ? buildGraphData(graph.entities, graph.relationships) : null;
  const pct = progress && progress.total ? Math.round((progress.index / progress.total) * 100) : null;

  return (
    <section className="panel app__scan">
      <h2>Index a folder or repo</h2>
      <div className="scan__row">
        <input
          placeholder="/absolute/path/to/repo"
          value={path}
          onChange={(e) => setPath(e.target.value)}
        />
        <button onClick={scan} disabled={!path.trim() || status === "scanning"}>
          {status === "scanning" ? "Indexing…" : "Index"}
        </button>
      </div>
      {error && <div className="app__error">{error}</div>}
      {status === "scanning" && progress && (
        <div className="scan__progress">
          {pct}% · {progress.index}/{progress.total} · <code>{progress.file}</code>
        </div>
      )}
      {summary && (
        <div className="scan__summary">
          Indexed <strong>{summary.ingested}</strong> file(s) · {summary.unchanged} unchanged ·{" "}
          {summary.skipped} skipped · {summary.entities} entities · {summary.relationships} edges
          {summary.errors ? ` · ${summary.errors} errors` : ""}
        </div>
      )}
      {skips.length > 0 && (
        <details className="scan__details">
          <summary>Skipped ({skips.length})</summary>
          <ul>
            {skips.map((s, i) => (
              <li key={i}>{s}</li>
            ))}
          </ul>
        </details>
      )}
      {errors.length > 0 && (
        <details className="scan__details">
          <summary>Errors ({errors.length})</summary>
          <ul>
            {errors.map((s, i) => (
              <li key={i}>{s}</li>
            ))}
          </ul>
        </details>
      )}
      <Graph3D data={graphData} />
    </section>
  );
}
