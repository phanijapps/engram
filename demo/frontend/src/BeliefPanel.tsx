import { useState } from "react";

const SCOPE = { tenant: "tenant-demo", workspace: "engram", environment: "local" };
const POLICY = {
  visibility: "workspace",
  retention: "durable",
  sensitivity: "low",
  allowedUses: ["retrieval"],
  deleteMode: "tombstone",
};
const ACTOR = { id: "actor-demo", kind: "agent", displayName: "Demo User" };

type Belief = {
  id: string;
  subject: { key: string };
  content: string;
  status: string;
  confidence: number;
  validFrom?: string;
  validUntil?: string;
};
type Contradiction = {
  id: string;
  kind: string;
  severity: number;
  status: string;
  targets: { targetId: string }[];
  reasoning?: string;
};

function ConfidenceBar({ value }: { value: number }) {
  const pct = Math.round(Math.max(0, Math.min(1, value)) * 100);
  return (
    <div className="graph3d__conf" aria-label={`confidence ${pct}%`}>
      <div className="graph3d__conf-fill" style={{ width: `${pct}%` }} />
    </div>
  );
}

function fmtTime(iso?: string): string {
  return iso ? new Date(iso).toLocaleString() : "—";
}

export function BeliefPanel() {
  const [beliefs, setBeliefs] = useState<Belief[]>([]);
  const [contradictions, setContradictions] = useState<Contradiction[]>([]);
  const [key, setKey] = useState("");
  const [content, setContent] = useState("");
  const [confidence, setConfidence] = useState(0.8);
  const [validFrom, setValidFrom] = useState("");
  const [validUntil, setValidUntil] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  const refresh = async () => {
    try {
      const [b, c] = await Promise.all([
        fetch("/belief/list", {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ scope: SCOPE }),
        }).then((r) => r.json() as Promise<Belief[]>),
        fetch("/belief/contradictions", {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ scope: SCOPE }),
        }).then((r) => r.json() as Promise<Contradiction[]>),
      ]);
      setBeliefs(b);
      setContradictions(c);
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
    }
  };

  const add = async () => {
    if (!key.trim() || !content.trim()) return;
    setBusy(true);
    setError("");
    try {
      const now = new Date().toISOString();
      await fetch("/belief/put", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          id: `belief-${Date.now()}`,
          scope: SCOPE,
          subject: { key: key.trim(), aliases: [] },
          content: content.trim(),
          status: "active",
          confidence,
          sources: [],
          policy: POLICY,
          provenance: {
            source: "demo:belief",
            actor: ACTOR,
            observedAt: now,
            confidence: 1,
            method: "manual",
          },
          createdAt: now,
          validFrom: validFrom ? new Date(validFrom).toISOString() : undefined,
          validUntil: validUntil ? new Date(validUntil).toISOString() : undefined,
        }),
      });
      setKey("");
      setContent("");
      await refresh();
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
    } finally {
      setBusy(false);
    }
  };

  const detect = async () => {
    setBusy(true);
    setError("");
    try {
      await fetch("/belief/detect", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ scope: SCOPE }),
      });
      await refresh();
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
    } finally {
      setBusy(false);
    }
  };

  const resolve = async (id: string, kind: "manual_ignore" | "target_won") => {
    setBusy(true);
    setError("");
    try {
      const now = new Date().toISOString();
      await fetch("/belief/resolve", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          id,
          scope: SCOPE,
          resolution: { kind, actor: ACTOR, resolvedAt: now },
        }),
      });
      await refresh();
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="panel app__belief">
      <h2>Beliefs &amp; contradictions</h2>
      <div className="ingest__row">
        <button onClick={refresh}>Refresh</button>
        <button onClick={detect} disabled={busy || beliefs.length === 0}>
          Detect contradictions
        </button>
      </div>
      {error && <div className="app__error">{error}</div>}

      <div className="belief__add">
        <input
          placeholder="subject key (e.g. svc-a)"
          value={key}
          onChange={(e) => setKey(e.target.value)}
        />
        <input
          placeholder="belief content (e.g. svc-a is up)"
          value={content}
          onChange={(e) => setContent(e.target.value)}
        />
        <label className="belief__conf">
          confidence {confidence.toFixed(2)}
          <input
            type="range"
            min={0}
            max={1}
            step={0.05}
            value={confidence}
            onChange={(e) => setConfidence(Number(e.target.value))}
          />
        </label>
        <label className="belief__time">
          valid from
          <input
            type="datetime-local"
            value={validFrom}
            onChange={(e) => setValidFrom(e.target.value)}
          />
        </label>
        <label className="belief__time">
          valid until
          <input
            type="datetime-local"
            value={validUntil}
            onChange={(e) => setValidUntil(e.target.value)}
          />
        </label>
        <button onClick={add} disabled={busy || !key.trim() || !content.trim()}>
          Add belief
        </button>
      </div>

      <div className="ontology__grid">
        <div>
          <h3>Beliefs ({beliefs.length})</h3>
          <ul className="results">
            {beliefs.map((b) => (
              <li key={b.id} className="result">
                <div className="result__body">
                  <code>{b.subject.key}</code> · {b.content}{" "}
                  <span className="graph3d__badge">{b.status}</span>
                </div>
                <div className="result__meta">
                  <ConfidenceBar value={b.confidence} />
                </div>
                <div className="result__meta">
                  valid {fmtTime(b.validFrom)} → {fmtTime(b.validUntil)}{" "}
                  <span title="display-only">(display-only)</span>
                </div>
              </li>
            ))}
          </ul>
        </div>
        <div>
          <h3>Contradictions ({contradictions.length})</h3>
          <ul className="results">
            {contradictions.map((c) => (
              <li key={c.id} className="result">
                <div className="result__body">
                  <code>{c.kind}</code> · severity {c.severity.toFixed(2)} ·{" "}
                  <span className="graph3d__badge">{c.status}</span>
                </div>
                <div className="result__meta">
                  {c.targets.length} target(s) · {c.reasoning ?? "—"}
                </div>
                {c.status === "open" && (
                  <div className="result__meta">
                    <button className="belief__resolve" onClick={() => resolve(c.id, "target_won")}>
                      resolve (target won)
                    </button>
                    <button className="belief__resolve" onClick={() => resolve(c.id, "manual_ignore")}>
                      ignore
                    </button>
                  </div>
                )}
              </li>
            ))}
          </ul>
        </div>
      </div>
    </section>
  );
}
