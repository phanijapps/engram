import { useState } from "react";

type Hit = { id: string; text: string; score: number };

const DEFAULT_CORPUS = `Engram is a contract-first agentic memory layer with a Rust core and TypeScript bindings. Memory records carry content, scope, provenance, and policy.

The knowledge graph extractor turns ingested code into entities and calls edges deterministically, without a model.

SQLite persists memories and knowledge durably so the demo survives restarts. Taxonomy concept schemes organize controlled vocabularies.

FastEmbed embeds passages and queries with the BGE-small model so sqlite-vec can return semantically related chunks.`;

export function SearchPanel() {
  const [corpus, setCorpus] = useState(DEFAULT_CORPUS);
  const [indexed, setIndexed] = useState<number | null>(null);
  const [query, setQuery] = useState("how are documents embedded for search?");
  const [hits, setHits] = useState<Hit[]>([]);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  const index = async () => {
    setBusy(true);
    setError("");
    try {
      const res = await fetch("/retrieval/index", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ text: corpus }),
      });
      if (!res.ok) throw new Error(`${res.status} ${await res.text()}`);
      const result = (await res.json()) as { indexed: number };
      setIndexed(result.indexed);
      setHits([]);
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
    }
    setBusy(false);
  };

  const search = async () => {
    if (!query.trim()) return;
    setBusy(true);
    setError("");
    try {
      const res = await fetch("/retrieval/search", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ query, topK: 5 }),
      });
      if (!res.ok) throw new Error(`${res.status} ${await res.text()}`);
      setHits((await res.json()) as Hit[]);
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
    }
    setBusy(false);
  };

  return (
    <section className="panel app__search">
      <h2>Semantic search (FastEmbed + sqlite-vec)</h2>
      <textarea
        value={corpus}
        onChange={(e) => setCorpus(e.target.value)}
        rows={6}
        placeholder="Paste a corpus to index…"
      />
      <div className="ingest__row">
        <span className="ingest__meta">
          {indexed !== null && `${indexed} chunks indexed (real Rust + FastEmbed)`}
        </span>
        <button onClick={index} disabled={busy || !corpus.trim()}>
          Index corpus
        </button>
      </div>
      <div className="search__query">
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Ask a question…"
          onKeyDown={(e) => {
            if (e.key === "Enter") void search();
          }}
        />
        <button onClick={search} disabled={busy || !query.trim() || indexed === null}>
          Search
        </button>
      </div>
      {error && <div className="app__error">{error}</div>}
      <ul className="results">
        {hits.length === 0 && indexed !== null && (
          <li className="results__empty">Run a search to see semantic matches.</li>
        )}
        {hits.map((hit, i) => (
          <li key={hit.id + i} className="result">
            <div className="result__body">{hit.text}</div>
            <div className="result__meta">
              <code>{hit.id}</code>
              <span> · score {hit.score.toFixed(3)}</span>
            </div>
          </li>
        ))}
      </ul>
    </section>
  );
}
