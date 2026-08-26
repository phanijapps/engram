//! Memory tab — progressive-disclosure fact stream with hybrid recall search.
//! Facts render as dense one-liners (truncated text + kind + date/score). Click
//! any row → expands inline to full text + metadata. No traditional pagination —
//! compact density shows many items at once. Search uses the hybrid recall API
//! (vector + graph + associative + temporal fusion) on the Facts tab.

import { useEffect, useState, type CSSProperties } from "react";
import { Search, ChevronRight, ChevronDown } from "lucide-react";

import {
  api,
  type BeliefView,
  type MemoryView,
  type ProcedureView,
  type RecallItem,
} from "../../lib/api.ts";

type TabKey = "memory" | "beliefs" | "contradictions" | "procedures";

const TABS: { key: TabKey; label: string }[] = [
  { key: "memory", label: "Facts" },
  { key: "beliefs", label: "Beliefs" },
  { key: "contradictions", label: "Contradictions" },
  { key: "procedures", label: "Procedures" },
];

interface DisplayItem {
  id: string;
  kind: string;
  text: string;
  date?: string;
  source?: string;
  score?: number;
}

const EMPTYcopy: Record<TabKey, { title: string; body: string }> = {
  beliefs: { title: "No beliefs", body: "Run reflection via the Maintain tab to synthesize beliefs." },
  contradictions: { title: "No contradictions", body: "Run contradiction detection via the Maintain tab." },
  procedures: { title: "No procedures", body: "Write a memory with kind=procedure to populate." },
  memory: { title: "No memories", body: "The memory store is empty for this workspace." },
};

export function MemoryTab() {
  const [tab, setTab] = useState<TabKey>("memory");
  const [items, setItems] = useState<DisplayItem[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [isRecall, setIsRecall] = useState(false);

  useEffect(() => {
    let cancelled = false;

    // Phase 1 (instant): LIKE search or list — shows results immediately.
    const quickLoader =
      tab === "memory" ? api.memory(undefined, undefined, search || undefined)
      : tab === "beliefs" ? api.beliefs()
      : tab === "procedures" ? api.procedures()
      : api.contradictions();

    quickLoader
      .then((page) => {
        if (cancelled) return;
        setItems(page.items.map((it) => normalizeItem(tab, it)));
        setIsRecall(false);
        setLoading(false);
      })
      .catch((e) => {
        if (!cancelled) { setError(e instanceof Error ? e.message : String(e)); setLoading(false); }
      });

    // Phase 2 (slow, Facts tab only with search): hybrid recall upgrades results.
    if (search.trim() && tab === "memory") {
      const recallTimer = setTimeout(() => {
        api
          .recall(search)
          .then((result) => {
            if (cancelled) return;
            setItems((result.items ?? []).map(normalizeRecall));
            setIsRecall(true);
          })
          .catch(() => { /* silent — LIKE results already shown */ });
      }, 400);
      return () => { cancelled = true; clearTimeout(recallTimer); };
    }

    return () => { cancelled = true; };
  }, [tab, search]);

  return (
    <div className="page">
      <div className="page-container" style={wrapStyle}>
        <nav style={tabNavStyle}>
          {TABS.map((t) => (
            <button
              key={t.key}
              style={tab === t.key ? tabActiveStyle : tabStyle}
              onClick={() => { setTab(t.key); setSearch(""); setExpandedId(null); }}
            >
              {t.label}
            </button>
          ))}
        </nav>

        <div style={searchRowStyle}>
          <div style={searchBoxStyle}>
            <Search style={{ width: 13, height: 13, opacity: 0.5 }} aria-hidden />
            <input
              style={searchInputStyle}
              placeholder={tab === "memory" ? "hybrid search (vector + graph)…" : `search ${tab}…`}
              value={search}
              onChange={(e) => setSearch(e.target.value)}
            />
          </div>
          {!loading && !error && items.length > 0 && (
            <span style={countStyle}>
              {items.length} {isRecall ? "results" : tab}
              {isRecall
                ? <span style={{ opacity: 0.5, marginLeft: 4 }}>recall</span>
                : (search.trim() && tab === "memory")
                  ? <span style={{ opacity: 0.4, marginLeft: 4 }}>text · upgrading…</span>
                  : null
              }
            </span>
          )}
        </div>

        {error && <p style={muted}>Error: {error}</p>}
        {loading && <p style={muted}>Loading…</p>}
        {!loading && !error && items.length === 0 && (
          search ? <p style={muted}>No matches for "{search}".</p> : <EmptyState tab={tab} />
        )}
        {!loading && !error && items.length > 0 && (
          <div style={listStyle}>
            {items.map((item) => (
              <FactRow
                key={item.id}
                item={item}
                expanded={expandedId === item.id}
                onClick={() => setExpandedId(expandedId === item.id ? null : item.id)}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function FactRow({ item, expanded, onClick }: { item: DisplayItem; expanded: boolean; onClick: () => void }) {
  return (
    <div
      style={{ ...rowStyle, background: expanded ? "var(--sidebar)" : "transparent" }}
      onClick={onClick}
    >
      <div style={rowHeadStyle}>
        <span style={kindBadgeStyle(item.kind)}>{item.kind.slice(0, 4).toUpperCase()}</span>
        {expanded ? (
          <ChevronDown style={{ width: 11, height: 11, opacity: 0.4 }} aria-hidden />
        ) : (
          <ChevronRight style={{ width: 11, height: 11, opacity: 0.3 }} aria-hidden />
        )}
        <span style={rowTextStyle}>{expanded ? item.text : truncate(item.text, 70)}</span>
        <span style={rowMetaStyle}>
          {item.score !== undefined && `${(item.score * 100).toFixed(0)}%`}
        </span>
        {item.date && <span style={rowMetaStyle}>{item.date.slice(0, 10)}</span>}
      </div>
      {expanded && (
        <div style={detailStyle}>
          {item.source && <span style={detailMeta}>source: {item.source}</span>}
          {item.score !== undefined && <span style={detailMeta}>relevance: {item.score.toFixed(4)}</span>}
          <span style={detailMeta}>id: {item.id}</span>
        </div>
      )}
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

// --- normalizers (unify memory + recall shapes → DisplayItem) ---

function normalizeItem(tab: TabKey, raw: unknown): DisplayItem {
  if (tab === "memory") {
    const m = raw as MemoryView;
    return { id: m.id, kind: m.kind ?? "mem", text: m.text ?? "", date: m.createdAt, source: m.source };
  }
  if (tab === "beliefs") {
    const b = raw as BeliefView;
    return { id: b.id, kind: "belief", text: b.text ?? b.id ?? "", source: b.subject };
  }
  const p = raw as ProcedureView;
  return { id: p.id, kind: "proc", text: p.text ?? "" };
}

function normalizeRecall(raw: RecallItem): DisplayItem {
  return {
    id: raw.id,
    kind: raw.targetType ?? "result",
    text: raw.content ?? "",
    score: raw.score?.total,
    source: raw.provenance?.source,
  };
}

// --- helpers ---

function truncate(text: string, max: number): string {
  if (text.length <= max) return text;
  const cut = text.slice(0, max);
  const lastSpace = cut.lastIndexOf(" ");
  return (lastSpace > max * 0.6 ? cut.slice(0, lastSpace) : cut) + "…";
}

// --- styles ---

const mono = "var(--font-mono)" as const;
const muted: CSSProperties = { fontFamily: mono, color: "var(--muted-foreground)", padding: "var(--spacing-4)" };

const wrapStyle: CSSProperties = { display: "flex", flexDirection: "column", gap: "var(--spacing-2)" };
const tabNavStyle: CSSProperties = { display: "flex", gap: "var(--spacing-1)", borderBottom: "1px solid var(--border)" };
const tabBase: CSSProperties = {
  fontFamily: mono, fontSize: 12, letterSpacing: "0.08em", textTransform: "uppercase",
  background: "transparent", border: "none", borderBottom: "2px solid transparent",
  color: "var(--muted-foreground)", cursor: "pointer", padding: "var(--spacing-2) var(--spacing-3)",
};
const tabStyle: CSSProperties = { ...tabBase };
const tabActiveStyle: CSSProperties = { ...tabBase, color: "var(--foreground)", borderBottom: "2px solid var(--primary)" };

const searchRowStyle: CSSProperties = { display: "flex", alignItems: "center", gap: "var(--spacing-2)" };
const searchBoxStyle: CSSProperties = {
  display: "flex", alignItems: "center", gap: "var(--spacing-1)",
  background: "var(--background)", border: "1px solid var(--border)",
  borderRadius: "var(--radius-sm)", padding: "2px var(--spacing-2)",
  flex: "1 1 auto", maxWidth: 320,
};
const searchInputStyle: CSSProperties = {
  background: "transparent", border: "none", outline: "none",
  color: "var(--foreground)", fontFamily: mono, fontSize: 12, width: "100%",
};
const countStyle: CSSProperties = { fontFamily: mono, fontSize: 10, color: "var(--muted-foreground)", whiteSpace: "nowrap" };

const listStyle: CSSProperties = { display: "flex", flexDirection: "column" };
const rowStyle: CSSProperties = {
  cursor: "pointer", borderBottom: "1px solid var(--border)",
  padding: "var(--spacing-1) var(--spacing-2)", transition: "background 0.1s",
};
const rowHeadStyle: CSSProperties = { display: "flex", alignItems: "center", gap: "var(--spacing-2)" };
const rowTextStyle: CSSProperties = {
  flex: "1 1 auto", fontFamily: mono, fontSize: 12, color: "var(--foreground)",
  overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
};
const rowMetaStyle: CSSProperties = { fontFamily: mono, fontSize: 10, color: "var(--muted-foreground)", whiteSpace: "nowrap" };

function kindBadgeStyle(kind: string): CSSProperties {
  const colors: Record<string, string> = {
    obs: "var(--primary)",
    fac: "var(--primary)",
    pro: "var(--purple)",
    pre: "var(--warning)",
    epi: "var(--success)",
    mem: "var(--primary)",
    bel: "var(--destructive)",
    ent: "var(--blue)",
    chu: "var(--subtle-foreground)",
    res: "var(--subtle-foreground)",
  };
  const key = kind.slice(0, 3).toLowerCase();
  return {
    fontFamily: mono, fontSize: 9, fontWeight: 600, letterSpacing: "0.05em",
    color: colors[key] ?? "var(--muted-foreground)", minWidth: 28, textAlign: "center",
  };
}

const detailStyle: CSSProperties = {
  display: "flex", flexWrap: "wrap", gap: "var(--spacing-3)",
  padding: "var(--spacing-2) var(--spacing-2) var(--spacing-1) 40px",
};
const detailMeta: CSSProperties = { fontFamily: mono, fontSize: 10, color: "var(--muted-foreground)" };

const emptyStyle: CSSProperties = { textAlign: "center", padding: "var(--spacing-6) var(--spacing-3)", color: "var(--muted-foreground)" };
const emptyTitleStyle: CSSProperties = { fontFamily: mono, fontSize: 14, marginBottom: "var(--spacing-2)" };
const emptyBodyStyle: CSSProperties = { fontSize: 12, maxWidth: 420, margin: "0 auto", lineHeight: 1.6 };
