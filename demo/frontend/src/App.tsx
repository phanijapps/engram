import { useCallback, useEffect, useState } from "react";

// Demo-local defaults (single-user, local). The backend is a pure JSON
// pass-through to the Rust memory service, so these mirror the v1 contract
// fixtures. Slice 4 tightens typing to generated @engram/contracts types.
const SCOPE = {
  tenant: "tenant-demo",
  workspace: "engram",
  environment: "local",
} as const;

const baseActor = {
  id: "actor-demo",
  kind: "agent",
  displayName: "Demo User",
} as const;

const POLICY = {
  visibility: "workspace",
  retention: "durable",
  sensitivity: "low",
  allowedUses: ["retrieval", "evaluation", "debugging"],
  deleteMode: "tombstone",
} as const;

type MemoryItem = {
  targetId?: string;
  targetType?: string;
  score?: number;
  content?: { text?: string };
  [key: string]: unknown;
};

type RetrieveResponse = { items?: MemoryItem[]; createdAt?: string };

async function postJson<T>(path: string, body: unknown): Promise<T> {
  const res = await fetch(path, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) {
    throw new Error(`${path} -> ${res.status}: ${await res.text()}`);
  }
  return (await res.json()) as T;
}

export function App() {
  const [text, setText] = useState("");
  const [query, setQuery] = useState("engram");
  const [items, setItems] = useState<MemoryItem[]>([]);
  const [message, setMessage] = useState<string>("");
  const [error, setError] = useState<string>("");

  const retrieve = useCallback(async (q: string) => {
    if (!q.trim()) {
      setItems([]);
      return;
    }
    try {
      const response = await postJson<RetrieveResponse>("/memory/retrieve", {
        query: q,
        scope: SCOPE,
        requester: {
          actor: baseActor,
          roles: ["maintainer"],
          permissions: ["memory.retrieve"],
        },
        modes: ["keyword"],
        limit: 10,
        budget: { maxItems: 10, maxTokens: 4000 },
        includeExplanations: true,
      });
      setItems(response.items ?? []);
      setError("");
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
    }
  }, []);

  const write = async () => {
    if (!text.trim()) return;
    const now = new Date().toISOString();
    try {
      const response = await postJson<{ record: { id: string } }>(
        "/memory/write",
        {
          kind: "observation",
          content: {
            text,
            summary: text.slice(0, 80),
            language: "en",
            format: "text",
          },
          scope: SCOPE,
          requester: {
            actor: baseActor,
            roles: ["maintainer"],
            permissions: ["memory.write"],
          },
          provenance: {
            source: "demo-ui",
            actor: baseActor,
            observedAt: now,
            confidence: 1,
            method: "manual",
          },
          policy: POLICY,
          idempotencyKey: `ui-${now}`,
        }
      );
      setMessage(`wrote memory ${response.record.id}`);
      setText("");
      await retrieve(query);
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
    }
  };

  const forget = async (targetId: string) => {
    try {
      await postJson<{ status: string }>("/memory/forget", {
        targetType: "memory",
        targetId,
        scope: SCOPE,
        requester: {
          actor: baseActor,
          roles: ["maintainer"],
          permissions: ["memory.forget"],
        },
        mode: "delete",
        reason: "demo ui",
      });
      setMessage(`forgot ${targetId}`);
      await retrieve(query);
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
    }
  };

  useEffect(() => {
    void retrieve(query);
  }, [query, retrieve]);

  return (
    <div className="app">
      <header className="app__header">
        <h1>Engram</h1>
        <p className="app__tagline">
          Browser → Node → Rust memory, backed by the live N-API bridge.
        </p>
      </header>

      {error && <div className="app__error">{error}</div>}
      {message && <div className="app__message">{message}</div>}

      <div className="app__columns">
        <section className="panel">
          <h2>Write a memory</h2>
          <textarea
            placeholder="Type something for Engram to remember…"
            value={text}
            onChange={(e) => setText(e.target.value)}
            rows={5}
          />
          <button onClick={write} disabled={!text.trim()}>
            Write memory
          </button>
        </section>

        <section className="panel">
          <h2>Retrieve</h2>
          <input
            placeholder="Query…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
          <ul className="results">
            {items.length === 0 && (
              <li className="results__empty">No memories matched.</li>
            )}
            {items.map((item, index) => (
              <li key={item.targetId ?? index} className="result">
                <div className="result__body">
                  {item.content?.text ?? item.targetId ?? JSON.stringify(item).slice(0, 80)}
                </div>
                <div className="result__meta">
                  {item.targetId && (
                    <>
                      <code>{item.targetId}</code>
                      {typeof item.score === "number" && (
                        <span> · score {item.score.toFixed(3)}</span>
                      )}
                    </>
                  )}
                </div>
                {item.targetId && (
                  <button className="result__forget" onClick={() => forget(item.targetId!)}>
                    Forget
                  </button>
                )}
              </li>
            ))}
          </ul>
        </section>
      </div>
    </div>
  );
}
