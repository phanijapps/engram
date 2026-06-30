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

type LLMStatus = { entities: number; relationships: number } | "unavailable" | "error";

type IngestResult = {
  entities: RawEntity[];
  relationships: RawRelationship[];
  chunkCount: number;
  llm?: LLMStatus;
};

const DEFAULT_CODE =
  "fn write() { read(); flush(); }\nfn read() {}\nfn flush() {}\nstruct Store;\n";

function llmHint(llm: LLMStatus | undefined): string {
  if (!llm) return "";
  if (llm === "unavailable") return "LLM unavailable (set .env creds) — deterministic only";
  if (llm === "error") return "LLM call failed — fell back to deterministic";
  return `LLM added ${llm.entities} entities · ${llm.relationships} edges`;
}

export function IngestPanel() {
  const [text, setText] = useState(DEFAULT_CODE);
  const [isCode, setIsCode] = useState(true);
  const [enhance, setEnhance] = useState(false);
  const [result, setResult] = useState<IngestResult | null>(null);
  const [error, setError] = useState("");

  const graphData = result
    ? buildGraphData(result.entities, result.relationships)
    : null;

  const ingest = async () => {
    try {
      const documentKind = isCode ? "code" : "text";
      const actor = { id: "actor-demo", kind: "agent", displayName: "Demo User" };
      const response = enhance
        ? await fetch("/llm/extract", {
            method: "POST",
            headers: { "content-type": "application/json" },
            body: JSON.stringify({
              text,
              documentKind,
              scope: SCOPE,
              policy: POLICY,
              sourceName: "demo-ingest",
              actor,
            }),
          })
        : await fetch("/ingest/extract", {
            method: "POST",
            headers: { "content-type": "application/json" },
            body: JSON.stringify({
              sourceKind: "filesystem",
              sourceName: "demo-ingest",
              scope: SCOPE,
              documentKind,
              document: { path: "snippet" },
              text,
              policy: POLICY,
              actor,
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
          <label
            title="Enhance the deterministic graph with LLM-extracted entities/relationships (needs .env creds)"
          >
            <input
              type="checkbox"
              checked={enhance}
              onChange={(e) => setEnhance(e.target.checked)}
            />{" "}
            LLM enhance
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
          {result.llm && <span className="ingest__llm"> · {llmHint(result.llm)}</span>}
        </div>
      )}
      <Graph3D data={graphData} />
    </section>
  );
}
