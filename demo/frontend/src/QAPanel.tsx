import { useState } from "react";

const SCOPE = { tenant: "tenant-demo", workspace: "engram", environment: "local" };

type QaSource = { kind: "memory" | "belief"; id: string; text: string; source: string };
type QaResult = {
  answer: string;
  sources: QaSource[];
  llm: "ok" | "unavailable" | "error";
};

export function QAPanel() {
  const [question, setQuestion] = useState("");
  const [result, setResult] = useState<QaResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  const ask = async () => {
    if (!question.trim()) return;
    setBusy(true);
    setError("");
    setResult(null);
    try {
      const res = await fetch("/qa/ask", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ question: question.trim(), scope: SCOPE }),
      });
      if (!res.ok) throw new Error(`${res.status} ${await res.text()}`);
      setResult((await res.json()) as QaResult);
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="panel app__qa">
      <h2>Ask over knowledge &amp; memory</h2>
      <div className="scan__row">
        <input
          placeholder="ask a question (e.g. is svc-a up?)"
          value={question}
          onChange={(e) => setQuestion(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") ask();
          }}
        />
        <button onClick={ask} disabled={busy || !question.trim()}>
          {busy ? "Answering…" : "Ask"}
        </button>
      </div>
      {error && <div className="app__error">{error}</div>}
      {result && (
        <>
          {result.llm === "unavailable" && (
            <div className="app__error">
              LLM unavailable (set .env creds) — showing grounded evidence only
            </div>
          )}
          {result.llm === "error" && (
            <div className="app__error">LLM synthesis failed — showing grounded evidence only</div>
          )}
          <div className="qa__answer">{result.answer}</div>
          {result.sources.length > 0 && (
            <details className="scan__details" open>
              <summary>Sources ({result.sources.length})</summary>
              <ul className="results">
                {result.sources.map((s, i) => (
                  <li key={`${s.kind}-${s.id}-${i}`} className="result">
                    <div className="result__body">
                      <span className="graph3d__badge">{s.kind}</span>{" "}
                      <code>{s.source}</code>
                    </div>
                    <div className="result__meta">{s.text}</div>
                  </li>
                ))}
              </ul>
            </details>
          )}
        </>
      )}
    </section>
  );
}
