//! Memory tab (S3) — a zbot-style command-deck over engram's memory/belief/
//! contradiction/procedure surfaces. Sub-tabs switch the active surface; each
//! loads a keyset-paginated list from the BFF. Beliefs/contradictions/procedures
//! are empty in the agentzero store today → honest empty-states with the
//! out-of-band populate pointer (never fabricated). Hybrid search is deferred
//! (the store's retrieval is Unsupported/RequiresReindex — see backlog).

import { useEffect, useState, type CSSProperties } from "react";

import { api, type BeliefView, type MemoryView, type ProcedureView } from "../../lib/api.ts";

type TabKey = "memory" | "beliefs" | "contradictions" | "procedures";

const TABS: { key: TabKey; label: string }[] = [
  { key: "memory", label: "Facts" },
  { key: "beliefs", label: "Beliefs" },
  { key: "contradictions", label: "Contradictions" },
  { key: "procedures", label: "Procedures" },
];

const EMPTYcopy: Record<TabKey, { title: string; body: string }> = {
  beliefs: {
    title: "No beliefs",
    body: "Beliefs are synthesized by engram's reflection engine. Run reflection via engram-mcp to populate this surface.",
  },
  contradictions: {
    title: "No contradictions",
    body: "Contradictions are derived from beliefs. With no beliefs synthesized, there are none to surface.",
  },
  procedures: {
    title: "No procedures",
    body: "Procedures are captured as learned workflows. None have been stored in this workspace yet.",
  },
  memory: { title: "No memories", body: "The memory store is empty for this workspace." },
};

export function MemoryTab() {
  const [tab, setTab] = useState<TabKey>("memory");
  const [items, setItems] = useState<unknown[]>([]);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    setItems([]);
    setNextCursor(null);
    const loader =
      tab === "memory"
        ? api.memory()
        : tab === "beliefs"
          ? api.beliefs()
          : tab === "procedures"
            ? api.procedures()
            : api.contradictions();
    loader
      .then((page) => {
        if (cancelled) return;
        setItems(page.items);
        setNextCursor(page.nextCursor);
        setLoading(false);
      })
      .catch((e) => {
        if (!cancelled) {
          setError(e instanceof Error ? e.message : String(e));
          setLoading(false);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [tab]);

  return (
    <div className="page">
      <div className="page-container" style={wrapStyle}>
        <nav style={tabNavStyle}>
          {TABS.map((t) => (
            <button
              key={t.key}
              style={tab === t.key ? tabActiveStyle : tabStyle}
              onClick={() => setTab(t.key)}
            >
              {t.label}
            </button>
          ))}
        </nav>

        {error && <p style={muted}>Error: {error}</p>}
        {loading && <p style={muted}>Loading…</p>}
        {!loading && !error && items.length === 0 && <EmptyState tab={tab} />}
        {!loading && !error && items.length > 0 && (
          <>
            <div style={gridStyle}>
              {items.map((it, i) => (
                <Card key={i} tab={tab} item={it} />
              ))}
            </div>
            {nextCursor && (
              <button style={moreStyle} onClick={() => loadMore(tab, nextCursor, setItems, setNextCursor, setError)}>
                Load more
              </button>
            )}
          </>
        )}
      </div>
    </div>
  );
}

async function loadMore(
  tab: TabKey,
  cursor: string,
  setItems: (fn: (prev: unknown[]) => unknown[]) => void,
  setNextCursor: (c: string | null) => void,
  setError: (e: string | null) => void,
) {
  try {
    const loader =
      tab === "memory"
        ? api.memory(cursor)
        : tab === "beliefs"
          ? api.beliefs(cursor)
          : tab === "procedures"
            ? api.procedures(cursor)
            : api.contradictions();
    const page = await loader;
    setItems((prev) => [...prev, ...page.items]);
    setNextCursor(page.nextCursor);
  } catch (e) {
    setError(e instanceof Error ? e.message : String(e));
  }
}

function Card({ tab, item }: { tab: TabKey; item: unknown }) {
  if (tab === "memory") {
    const m = item as MemoryView;
    return (
      <div style={cardStyle}>
        <div style={cardHeadStyle}>
          <span style={kindStyle}>{m.kind}</span>
          {m.createdAt && <span style={metaStyle}>{m.createdAt.slice(0, 10)}</span>}
        </div>
        <p style={textStyle}>{m.text}</p>
        <div style={footStyle}>
          {m.source && <span style={metaStyle}>{m.source}</span>}
          {m.confidence !== undefined && (
            <span style={metaStyle}>conf {m.confidence.toFixed(2)}</span>
          )}
        </div>
      </div>
    );
  }
  if (tab === "beliefs") {
    const b = item as BeliefView;
    return (
      <div style={cardStyle}>
        <p style={textStyle}>{b.text ?? b.id}</p>
        {b.subject && <span style={metaStyle}>{b.subject}</span>}
      </div>
    );
  }
  const p = item as ProcedureView;
  return (
    <div style={cardStyle}>
      <p style={textStyle}>{p.text}</p>
    </div>
  );
}

function EmptyState({ tab }: { tab: TabKey }) {
  const c = EMPTYcopy[tab];
  return (
    <div style={emptyStyle}>
      <div style={emptyTitleStyle}>{c.title}</div>
      <div style={emptyBodyStyle}>{c.body}</div>
    </div>
  );
}

const mono = "var(--font-mono)" as const;
const muted: CSSProperties = { fontFamily: mono, color: "var(--muted-foreground)" };

const wrapStyle: CSSProperties = { display: "flex", flexDirection: "column", gap: "var(--spacing-3)" };
const tabNavStyle: CSSProperties = { display: "flex", gap: "var(--spacing-1)", borderBottom: "1px solid var(--border)" };
const tabBase: CSSProperties = {
  fontFamily: mono,
  fontSize: 12,
  letterSpacing: "0.08em",
  textTransform: "uppercase",
  background: "transparent",
  border: "none",
  borderBottom: "2px solid transparent",
  color: "var(--muted-foreground)",
  cursor: "pointer",
  padding: "var(--spacing-2) var(--spacing-3)",
};
const tabStyle: CSSProperties = { ...tabBase };
const tabActiveStyle: CSSProperties = {
  ...tabBase,
  color: "var(--foreground)",
  borderBottom: "2px solid var(--primary, #7df9ff)",
};
const gridStyle: CSSProperties = {
  display: "grid",
  gridTemplateColumns: "repeat(auto-fill, minmax(320px, 1fr))",
  gap: "var(--spacing-3)",
};
const cardStyle: CSSProperties = {
  background: "var(--sidebar)",
  border: "1px solid var(--border)",
  borderRadius: "var(--radius-md)",
  padding: "var(--spacing-3)",
  display: "flex",
  flexDirection: "column",
  gap: "var(--spacing-2)",
};
const cardHeadStyle: CSSProperties = { display: "flex", justifyContent: "space-between", alignItems: "center" };
const kindStyle: CSSProperties = {
  fontFamily: mono,
  fontSize: 10,
  letterSpacing: "0.08em",
  textTransform: "uppercase",
  color: "var(--primary, #7df9ff)",
};
const textStyle: CSSProperties = { margin: 0, color: "var(--foreground)", fontSize: 13, lineHeight: 1.5 };
const footStyle: CSSProperties = { display: "flex", gap: "var(--spacing-3)" };
const metaStyle: CSSProperties = { fontFamily: mono, fontSize: 10, color: "var(--muted-foreground)" };
const moreStyle: CSSProperties = {
  alignSelf: "flex-start",
  fontFamily: mono,
  fontSize: 11,
  background: "transparent",
  border: "1px solid var(--border)",
  color: "var(--muted-foreground)",
  borderRadius: "var(--radius-sm)",
  cursor: "pointer",
  padding: "var(--spacing-1) var(--spacing-3)",
};
const emptyStyle: CSSProperties = {
  textAlign: "center",
  padding: "var(--spacing-6) var(--spacing-3)",
  color: "var(--muted-foreground)",
};
const emptyTitleStyle: CSSProperties = { fontFamily: mono, fontSize: 14, marginBottom: "var(--spacing-2)" };
const emptyBodyStyle: CSSProperties = { fontSize: 12, maxWidth: 420, margin: "0 auto", lineHeight: 1.6 };
