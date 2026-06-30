import { useState } from "react";
import { Graph3D, buildGraphData, type RawEntity, type RawRelationship } from "./Graph3D";

const SCOPE = { tenant: "tenant-demo", workspace: "engram", environment: "local" };
const POLICY = {
  visibility: "workspace",
  retention: "durable",
  sensitivity: "low",
  allowedUses: ["retrieval"],
  deleteMode: "tombstone",
};

type IngestResult = {
  entities: RawEntity[];
  relationships: RawRelationship[];
  chunkCount: number;
};

const DEFAULT_CODE =
  "fn write() { read(); flush(); }\nfn read() {}\nfn flush() {}\nstruct Store;\n";

export function IngestPanel() {
  const [text, setText] = useState(DEFAULT_CODE);
  const [isCode, setIsCode] = useState(true);
  const [result, setResult] = useState<IngestResult | null>(null);
  const [error, setError] = useState("");

  const graphData = result
    ? buildGraphData(result.entities, result.relationships)
    : null;

  const ingest = async () => {
    try {
      const response = await fetch("/ingest/extract", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          sourceKind: "filesystem",
          sourceName: "demo-ingest",
          scope: SCOPE,
          documentKind: isCode ? "code" : "text",
          document: { path: "snippet" },
          text,
          policy: POLICY,
          actor: { id: "actor-demo", kind: "agent", displayName: "Demo User" },
        }),
      });
      if (!response.ok) throw new Error(`${response.status} ${await response.text()}`);
      setResult((await response.json()) as IngestResult);
      setError("");
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
    }
  };

  return (
    <section className="panel app__ingest">
      <h2>Ingest &amp; extract graph</h2>
      <div className="ingest__controls">
        <textarea value={text} onChange={(e) => setText(e.target.value)} rows={6} />
        <div className="ingest__row">
          <label>
            <input
              type="checkbox"
              checked={isCode}
              onChange={(e) => setIsCode(e.target.checked)}
            />{" "}
            code document
          </label>
          <button onClick={ingest} disabled={!text.trim()}>
            Ingest &amp; extract
          </button>
        </div>
      </div>
      {error && <div className="app__error">{error}</div>}
      {result && (
        <div className="ingest__meta">
          {result.entities.length} entities · {result.relationships.length} edges ·{" "}
          {result.chunkCount} chunks (real Rust)
        </div>
      )}
      <Graph3D data={graphData} />
    </section>
  );
}
