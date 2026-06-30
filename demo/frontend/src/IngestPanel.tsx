import { useMemo, useState } from "react";
import { Graph3D, type GraphData, type GraphSourceRef } from "./Graph3D";

const SCOPE = { tenant: "tenant-demo", workspace: "engram", environment: "local" };
const POLICY = {
  visibility: "workspace",
  retention: "durable",
  sensitivity: "low",
  allowedUses: ["retrieval"],
  deleteMode: "tombstone",
};

type Entity = {
  id: string;
  name: string;
  kind?: string;
  aliases?: string[];
  sourceRefs?: GraphSourceRef[];
};
type Relationship = {
  id: string;
  subject: { id?: string; name?: string };
  predicate: string;
  object: { id?: string; name?: string };
  confidence?: number;
};
type IngestResult = {
  entities: Entity[];
  relationships: Relationship[];
  chunkCount: number;
};

const DEFAULT_CODE =
  "fn write() { read(); flush(); }\nfn read() {}\nfn flush() {}\nstruct Store;\n";

export function IngestPanel() {
  const [text, setText] = useState(DEFAULT_CODE);
  const [isCode, setIsCode] = useState(true);
  const [result, setResult] = useState<IngestResult | null>(null);
  const [error, setError] = useState("");

  const graphData: GraphData | null = useMemo(() => {
    if (!result) return null;
    const degree = new Map<string, number>();
    for (const rel of result.relationships) {
      // Count only edges that will actually render (both endpoints present) so
      // node sizing matches the visible graph.
      if (rel.subject.id && rel.object.id) {
        degree.set(rel.subject.id, (degree.get(rel.subject.id) ?? 0) + 1);
        degree.set(rel.object.id, (degree.get(rel.object.id) ?? 0) + 1);
      }
    }
    return {
      entities: result.entities.map((entity) => ({
        id: entity.id,
        name: entity.name,
        kind: entity.kind ?? "unknown",
        degree: degree.get(entity.id) ?? 0,
        aliases: entity.aliases,
        sourceRefs: entity.sourceRefs,
      })),
      relationships: result.relationships
        .filter((rel) => rel.subject.id && rel.object.id)
        .map((rel, index) => ({
          id: rel.id ?? `edge-${index}`,
          subject: { id: rel.subject.id!, name: rel.subject.name },
          predicate: rel.predicate,
          object: { id: rel.object.id!, name: rel.object.name },
          confidence: rel.confidence,
        })),
    };
  }, [result]);

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
