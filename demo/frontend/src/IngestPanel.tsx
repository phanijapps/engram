import { useEffect, useRef, useState } from "react";
import cytoscape from "cytoscape";

const SCOPE = { tenant: "tenant-demo", workspace: "engram", environment: "local" };
const POLICY = {
  visibility: "workspace",
  retention: "durable",
  sensitivity: "low",
  allowedUses: ["retrieval"],
  deleteMode: "tombstone",
};

type Entity = { id: string; name: string; kind?: string };
type Relationship = {
  subject: { id?: string; name?: string };
  predicate: string;
  object: { id?: string; name?: string };
};
type IngestResult = { entities: Entity[]; relationships: Relationship[]; chunkCount: number };

const DEFAULT_CODE =
  "fn write() { read(); flush(); }\nfn read() {}\nfn flush() {}\nstruct Store;\n";

export function IngestPanel() {
  const [text, setText] = useState(DEFAULT_CODE);
  const [isCode, setIsCode] = useState(true);
  const [result, setResult] = useState<IngestResult | null>(null);
  const [error, setError] = useState("");
  const cyRef = useRef<HTMLDivElement>(null);
  const cy = useRef<cytoscape.Core | null>(null);

  useEffect(() => {
    if (!cyRef.current) return;
    cy.current = cytoscape({
      container: cyRef.current,
      elements: [],
      style: [
        {
          selector: "node",
          style: {
            label: "data(label)",
            color: "#e7ecf5",
            "background-color": "#6ea8ff",
            "text-wrap": "wrap",
            "text-max-width": "120px",
            "font-size": "11px",
          },
        },
        {
          selector: "node[kind = 'class']",
          style: { "background-color": "#b07cff" },
        },
        {
          selector: "node[kind = 'concept']",
          style: { "background-color": "#7bd88f" },
        },
        {
          selector: "edge",
          style: {
            label: "data(label)",
            "curve-style": "bezier",
            "target-arrow-shape": "triangle",
            "line-color": "#9aa6bd",
            "target-arrow-color": "#9aa6bd",
            color: "#9aa6bd",
            "font-size": "9px",
            width: 2,
          },
        },
      ],
    });
    return () => {
      cy.current?.destroy();
      cy.current = null;
    };
  }, []);

  useEffect(() => {
    const instance = cy.current;
    if (!instance) return;
    instance.elements().remove();
    if (!result) return;
    for (const entity of result.entities) {
      instance.add({
        data: { id: entity.id, label: entity.name, kind: entity.kind ?? "entity" },
      });
    }
    result.relationships.forEach((relationship, index) => {
      const source = relationship.subject.id;
      const target = relationship.object.id;
      if (source && target) {
        instance.add({
          data: { id: `edge-${index}`, source, target, label: relationship.predicate },
        });
      }
    });
    instance.layout({ name: "cose", animate: false, padding: 24 }).run();
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
      <div ref={cyRef} className="ingest__graph" />
    </section>
  );
}
