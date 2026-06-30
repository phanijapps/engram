import { useState } from "react";

const SCOPE = { tenant: "tenant-demo", workspace: "engram", environment: "local" };

type Klass = {
  id: string;
  label: string;
  description?: string;
  parentClassIds?: string[];
};
type Property = {
  id: string;
  label: string;
  domainClassId?: string;
  rangeClassId?: string;
};
type Sample = {
  ontology: { id: string; name: string };
  classes: Klass[];
  properties: Property[];
  axioms: unknown[];
  scheme: { id: string; name: string };
  concepts: { id: string; prefLabel?: { value?: string } }[];
};
type Finding = { id: string; severity: string; code: string; message: string };

export function OntologyPanel() {
  const [sample, setSample] = useState<Sample | null>(null);
  const [graphId, setGraphId] = useState("");
  const [findings, setFindings] = useState<Finding[] | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  const load = async () => {
    setBusy(true);
    setError("");
    setFindings(null);
    try {
      const res = await fetch("/ontology/it-org", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ scope: SCOPE }),
      });
      if (!res.ok) throw new Error(`${res.status} ${await res.text()}`);
      const body = (await res.json()) as { sample: Sample };
      setSample(body.sample);
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
    } finally {
      setBusy(false);
    }
  };

  const validate = async () => {
    if (!graphId.trim() || !sample) return;
    setBusy(true);
    setError("");
    try {
      const res = await fetch("/ontology/validate", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          graphId: graphId.trim(),
          ontologyId: sample.ontology.id,
          scope: SCOPE,
        }),
      });
      if (!res.ok) throw new Error(`${res.status} ${await res.text()}`);
      setFindings(((await res.json()) as Finding[]) ?? []);
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
    } finally {
      setBusy(false);
    }
  };

  const classLabel = (id?: string) =>
    id ? sample?.classes.find((c) => c.id === id)?.label ?? id : "—";

  return (
    <section className="panel app__ontology">
      <h2>Ontology &amp; taxonomy</h2>
      <div className="ingest__row">
        <button onClick={load} disabled={busy}>
          {sample ? "Reload IT-org ontology" : "Load IT-org ontology"}
        </button>
        {sample && (
          <>
            <input
              placeholder="graph id to validate (from Ingest details)"
              value={graphId}
              onChange={(e) => setGraphId(e.target.value)}
            />
            <button onClick={validate} disabled={busy || !graphId.trim()}>
              Validate graph
            </button>
          </>
        )}
      </div>
      {error && <div className="app__error">{error}</div>}
      {sample && (
        <div className="ingest__meta">
          ontology <code>{sample.ontology.id}</code> · {sample.classes.length} classes ·{" "}
          {sample.properties.length} properties · {sample.axioms.length} axioms · taxonomy{" "}
          <code>{sample.scheme.id}</code> ({sample.concepts.length} concepts)
        </div>
      )}
      {sample && (
        <div className="ontology__grid">
          <div>
            <h3>Classes</h3>
            <ul className="results">
              {sample.classes.map((c) => (
                <li key={c.id} className="result">
                  <div className="result__body">
                    {c.label}
                    {c.parentClassIds && c.parentClassIds.length > 0 && (
                      <span className="result__meta">
                        {" "}
                        extends {c.parentClassIds.map(classLabel).join(", ")}
                      </span>
                    )}
                  </div>
                  {c.description && <div className="result__meta">{c.description}</div>}
                </li>
              ))}
            </ul>
          </div>
          <div>
            <h3>Properties</h3>
            <ul className="results">
              {sample.properties.map((p) => (
                <li key={p.id} className="result">
                  <div className="result__body">
                    {p.label}{" "}
                    <span className="result__meta">
                      {classLabel(p.domainClassId)} → {classLabel(p.rangeClassId)}
                    </span>
                  </div>
                </li>
              ))}
            </ul>
            <h3>Taxonomy</h3>
            <ul className="results">
              {sample.concepts.map((c) => (
                <li key={c.id} className="result">
                  <div className="result__body">{c.prefLabel?.value ?? c.id}</div>
                </li>
              ))}
            </ul>
          </div>
        </div>
      )}
      {findings && (
        <details className="scan__details" open={findings.length > 0}>
          <summary>Validation findings ({findings.length})</summary>
          {findings.length === 0 ? (
            <div className="result__meta">No undeclared predicates — graph conforms.</div>
          ) : (
            <ul className="results">
              {findings.map((f) => (
                <li key={f.id} className="result">
                  <div className="result__body">
                    <code>{f.severity}</code> · {f.message}
                  </div>
                  <div className="result__meta">{f.code}</div>
                </li>
              ))}
            </ul>
          )}
        </details>
      )}
    </section>
  );
}
