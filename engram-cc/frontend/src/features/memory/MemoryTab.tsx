//! Memory tab — progressive-disclosure fact stream with hybrid recall search.
//! Click any row → slide-out panel with the FULL content (procedures render
//! as numbered steps, facts as readable paragraphs, beliefs with confidence).

import { useEffect, useState, type CSSProperties, type ReactNode } from "react";
import { Search, ChevronRight, X, Clock, User, Tag, CheckCircle2, XCircle } from "lucide-react";

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
  fullText?: string;
  steps?: string[];
  date?: string;
  source?: string;
  score?: number;
  confidence?: number;
  successCount?: number;
  failureCount?: number;
}

const EMPTY: Record<TabKey, { title: string; body: string }> = {
  beliefs: { title: "No beliefs", body: "Run reflection via the Maintain tab to synthesize beliefs." },
  contradictions: { title: "No contradictions", body: "Run contradiction detection via the Maintain tab." },
  procedures: { title: "No procedures", body: "Store procedures with `remember kind=procedure`." },
  memory: { title: "No memories", body: "The memory store is empty for this workspace." },
};

export function MemoryTab() {
  const [tab, setTab] = useState<TabKey>("memory");
  const [items, setItems] = useState<DisplayItem[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [selected, setSelected] = useState<DisplayItem | null>(null);
  const [isRecall, setIsRecall] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);

    const loader =
      tab === "memory" ? api.memory(undefined, undefined, search || undefined)
      : tab === "beliefs" ? api.beliefs()
      : tab === "procedures" ? api.procedures()
      : api.contradictions();

    loader
      .then((page) => {
        if (cancelled) return;
        setItems(page.items.map((it) => normalizeItem(tab, it)));
        setIsRecall(false);
        setLoading(false);
      })
      .catch((e) => {
        if (!cancelled) { setError(e instanceof Error ? e.message : String(e)); setLoading(false); }
      });

    if (search.trim() && tab === "memory") {
      const timer = setTimeout(() => {
        api.recall(search)
          .then((result) => {
            if (cancelled) return;
            setItems((result.items ?? []).map(normalizeRecall));
            setIsRecall(true);
          })
          .catch(() => { /* silent */ });
      }, 400);
      return () => { cancelled = true; clearTimeout(timer); };
    }

    return () => { cancelled = true; };
  }, [tab, search]);

  return (
    <div style={{ position: "relative", height: "100%" }}>
      <div className="page">
        <div className="page-container" style={wrapStyle}>
          <nav style={tabNavStyle}>
            {TABS.map((t) => (
              <button
                key={t.key}
                style={tab === t.key ? tabActiveStyle : tabStyle}
                onClick={() => { setTab(t.key); setSearch(""); setSelected(null); }}
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
                <FactRow key={item.id} item={item} onClick={() => setSelected(item)} />
              ))}
            </div>
          )}
        </div>
      </div>

      {selected && <DetailSlideout item={selected} onClose={() => setSelected(null)} />}
    </div>
  );
}

// ===== Row (compact one-liner) =====

function FactRow({ item, onClick }: { item: DisplayItem; onClick: () => void }) {
  return (
    <div style={rowStyle} onClick={onClick}>
      <span style={kindBadge(item.kind)}>{item.kind.slice(0, 4).toUpperCase()}</span>
      <ChevronRight style={{ width: 11, height: 11, opacity: 0.3, flexShrink: 0 }} aria-hidden />
      <span style={rowTextStyle}>{truncate(item.text, 80)}</span>
      <span style={rowMetaStyle}>
        {item.score !== undefined && `${(item.score * 100).toFixed(0)}%`}
        {item.confidence !== undefined && `${(item.confidence * 100).toFixed(0)}%`}
        {item.successCount !== undefined && ` ✓${item.successCount}`}
      </span>
      {item.date && <span style={rowMetaStyle}>{item.date.slice(0, 10)}</span>}
    </div>
  );
}

// ===== Slide-out Detail Panel =====

function DetailSlideout({ item, onClose }: { item: DisplayItem; onClose: () => void }) {
  return (
    <>
      <div style={backdropStyle} onClick={onClose} />
      <div style={panelStyle}>
        <div style={panelHeaderStyle}>
          <span style={kindBadge(item.kind)}>{item.kind.toUpperCase()}</span>
          <span style={panelTitleStyle}>{item.id}</span>
          <button style={closeBtnStyle} onClick={onClose} aria-label="Close">
            <X style={{ width: 14, height: 14 }} />
          </button>
        </div>

        <div style={panelBodyStyle}>
          {/* Metadata row */}
          <div style={metaRowStyle}>
            {item.date && (
              <span style={metaTagStyle}>
                <Clock style={{ width: 10, height: 10 }} /> {item.date.slice(0, 10)}
              </span>
            )}
            {item.source && (
              <span style={metaTagStyle}>
                <User style={{ width: 10, height: 10 }} /> {item.source}
              </span>
            )}
            {item.confidence !== undefined && (
              <span style={metaTagStyle}>
                <Tag style={{ width: 10, height: 10 }} /> confidence {(item.confidence * 100).toFixed(0)}%
              </span>
            )}
            {item.score !== undefined && (
              <span style={metaTagStyle}>
                <Tag style={{ width: 10, height: 10 }} /> relevance {(item.score * 100).toFixed(0)}%
              </span>
            )}
            {item.successCount !== undefined && (
              <span style={metaTagStyle}>
                <CheckCircle2 style={{ width: 10, height: 10, color: "var(--success)" }} /> {item.successCount}
                <XCircle style={{ width: 10, height: 10, color: "var(--destructive)", marginLeft: 6 }} /> {item.failureCount}
              </span>
            )}
          </div>

          {/* Content */}
          {item.steps && item.steps.length > 0 ? (
            <ol style={stepsStyle}>
              {item.steps.map((step, i) => (
                <li key={i} style={stepStyle}>
                  <span style={stepNumStyle}>{i + 1}</span>
                  <span>{step}</span>
                </li>
              ))}
            </ol>
          ) : (
            <div style={textStyle}>
              {item.fullText || item.text}
            </div>
          )}
        </div>
      </div>
    </>
  );
}

function EmptyState({ tab }: { tab: TabKey }) {
  const c = EMPTY[tab];
  return (
    <div style={emptyStyle}>
      <div style={emptyTitleStyle}>{c.title}</div>
      <div style={emptyBodyStyle}>{c.body}</div>
    </div>
  );
}

// ===== Normalizers =====

function normalizeItem(tab: TabKey, raw: unknown): DisplayItem {
  if (tab === "memory") {
    const m = raw as MemoryView;
    return { id: m.id, kind: m.kind ?? "mem", text: m.text ?? "", fullText: m.text, date: m.createdAt, source: m.source };
  }
  if (tab === "beliefs") {
    const b = raw as BeliefView;
    return { id: b.id, kind: "belief", text: b.text ?? b.id ?? "", fullText: b.text, source: b.subject, confidence: b.confidence };
  }
  const p = raw as ProcedureView;
  // Procedures from the API return joined text; split back for step display
  const rawSteps = (p as { steps?: string[] }).steps;
  return {
    id: p.id,
    kind: "proc",
    text: p.text ?? p.name ?? p.id,
    fullText: p.text,
    steps: rawSteps,
    successCount: p.successCount,
    failureCount: p.failureCount,
  };
}

function normalizeRecall(raw: RecallItem): DisplayItem {
  return {
    id: raw.id,
    kind: raw.targetType ?? "result",
    text: raw.content ?? "",
    fullText: raw.content,
    score: raw.score?.total,
    source: raw.provenance?.source,
  };
}

// ===== Helpers =====

function truncate(text: string, max: number): string {
  if (text.length <= max) return text;
  const cut = text.slice(0, max);
  const lastSpace = cut.lastIndexOf(" ");
  return (lastSpace > max * 0.6 ? cut.slice(0, lastSpace) : cut) + "…";
}

// ===== Styles =====

const mono = "var(--font-mono)" as const;
const muted: CSSProperties = { fontFamily: mono, color: "var(--muted-foreground)", padding: "var(--spacing-4)" };

const wrapStyle: CSSProperties = { display: "flex", flexDirection: "column", gap: "var(--spacing-2)" };
const tabNavStyle: CSSProperties = { display: "flex", gap: "var(--spacing-1)", borderBottom: "1px solid var(--border)" };
const tabBase: CSSProperties = {
  fontFamily: mono, fontSize: 12, letterSpacing: "0.08em", textTransform: "uppercase",
  background: "transparent", borderWidth: 0, borderBottomWidth: 2, borderBottomStyle: "solid", borderBottomColor: "transparent",
  color: "var(--muted-foreground)", cursor: "pointer", padding: "var(--spacing-2) var(--spacing-3)",
};
const tabStyle: CSSProperties = { ...tabBase };
const tabActiveStyle: CSSProperties = { ...tabBase, color: "var(--foreground)", borderBottomColor: "var(--primary)" };

const searchRowStyle: CSSProperties = { display: "flex", alignItems: "center", gap: "var(--spacing-2)" };
const searchBoxStyle: CSSProperties = {
  display: "flex", alignItems: "center", gap: "var(--spacing-1)",
  background: "var(--background)", borderWidth: 1, borderStyle: "solid", borderColor: "var(--border)",
  borderRadius: "var(--radius-sm)", padding: "2px var(--spacing-2)",
  flex: "1 1 auto", maxWidth: 320,
};
const searchInputStyle: CSSProperties = {
  background: "transparent", borderWidth: 0, outline: "none",
  color: "var(--foreground)", fontFamily: mono, fontSize: 12, width: "100%",
};
const countStyle: CSSProperties = { fontFamily: mono, fontSize: 10, color: "var(--muted-foreground)", whiteSpace: "nowrap" };

const listStyle: CSSProperties = { display: "flex", flexDirection: "column" };
const rowStyle: CSSProperties = {
  display: "flex", alignItems: "center", gap: "var(--spacing-2)",
  cursor: "pointer", borderBottomWidth: 1, borderBottomStyle: "solid", borderBottomColor: "var(--border)",
  padding: "var(--spacing-1) var(--spacing-2)", transition: "background 0.1s",
};
const rowTextStyle: CSSProperties = {
  flex: "1 1 auto", fontFamily: mono, fontSize: 12, color: "var(--foreground)",
  overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
};
const rowMetaStyle: CSSProperties = { fontFamily: mono, fontSize: 10, color: "var(--muted-foreground)", whiteSpace: "nowrap" };

function kindBadge(kind: string): CSSProperties {
  const colors: Record<string, string> = {
    obs: "var(--primary)", fac: "var(--primary)", pro: "#c9a8ff",
    pre: "#f5a97f", epi: "var(--success)", mem: "var(--primary)",
    bel: "#ed8796", ent: "#8aadf4", chu: "#a5adcb", res: "#a5adcb",
  };
  const key = kind.slice(0, 3).toLowerCase();
  return {
    fontFamily: mono, fontSize: 9, fontWeight: 600, letterSpacing: "0.05em",
    color: colors[key] ?? "var(--muted-foreground)", minWidth: 28, textAlign: "center",
    flexShrink: 0,
  };
}

// Slide-out panel styles
const backdropStyle: CSSProperties = {
  position: "fixed", inset: 0, background: "rgba(0,0,0,0.5)", zIndex: 100,
  backdropFilter: "blur(2px)",
};
const panelStyle: CSSProperties = {
  position: "fixed", top: 0, right: 0, bottom: 0,
  width: "min(480px, 90vw)",
  background: "var(--sidebar)",
  borderLeft: "1px solid var(--border)",
  zIndex: 101,
  display: "flex", flexDirection: "column",
  boxShadow: "-8px 0 32px rgba(0,0,0,0.3)",
  animation: "slideIn 0.2s ease-out",
};
const panelHeaderStyle: CSSProperties = {
  display: "flex", alignItems: "center", gap: "var(--spacing-2)",
  padding: "var(--spacing-3) var(--spacing-4)",
  borderBottomWidth: 1, borderBottomStyle: "solid", borderBottomColor: "var(--border)",
};
const panelTitleStyle: CSSProperties = {
  flex: "1 1 auto", fontFamily: mono, fontSize: 13, color: "var(--foreground)",
  overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
};
const closeBtnStyle: CSSProperties = {
  background: "transparent", borderWidth: 1, borderStyle: "solid", borderColor: "var(--border)",
  borderRadius: "var(--radius-sm)", color: "var(--muted-foreground)",
  cursor: "pointer", padding: "4px", display: "flex", alignItems: "center",
};
const panelBodyStyle: CSSProperties = {
  flex: "1 1 auto", overflowY: "auto", padding: "var(--spacing-4)",
  display: "flex", flexDirection: "column", gap: "var(--spacing-3)",
};
const metaRowStyle: CSSProperties = { display: "flex", flexWrap: "wrap", gap: "var(--spacing-2)" };
const metaTagStyle: CSSProperties = {
  display: "inline-flex", alignItems: "center", gap: 4,
  fontFamily: mono, fontSize: 10, color: "var(--muted-foreground)",
  background: "var(--background)", borderWidth: 1, borderStyle: "solid", borderColor: "var(--border)",
  borderRadius: "var(--radius-sm)", padding: "2px 8px",
};
const stepsStyle: CSSProperties = {
  listStyle: "none", display: "flex", flexDirection: "column", gap: "var(--spacing-2)",
  counterReset: "none",
};
const stepStyle: CSSProperties = {
  display: "flex", gap: "var(--spacing-2)", alignItems: "flex-start",
  padding: "var(--spacing-2)", background: "var(--background)",
  borderRadius: "var(--radius-sm)", borderWidth: 1, borderStyle: "solid", borderColor: "var(--border)",
  fontFamily: mono, fontSize: 12, color: "var(--foreground)", lineHeight: 1.5,
};
const stepNumStyle: CSSProperties = {
  flexShrink: 0, width: 20, height: 20, display: "flex", alignItems: "center", justifyContent: "center",
  background: "var(--primary)", color: "var(--background)",
  borderRadius: "50%", fontFamily: mono, fontSize: 10, fontWeight: 600,
};
const textStyle: CSSProperties = {
  fontFamily: mono, fontSize: 13, color: "var(--foreground)",
  lineHeight: 1.8, whiteSpace: "pre-wrap", wordBreak: "break-word",
  padding: "var(--spacing-2)", background: "var(--background)",
  borderRadius: "var(--radius-sm)", borderWidth: 1, borderStyle: "solid", borderColor: "var(--border)",
};

const emptyStyle: CSSProperties = { textAlign: "center", padding: "var(--spacing-6) var(--spacing-3)", color: "var(--muted-foreground)" };
const emptyTitleStyle: CSSProperties = { fontFamily: mono, fontSize: 14, marginBottom: "var(--spacing-2)" };
const emptyBodyStyle: CSSProperties = { fontSize: 12, maxWidth: 420, margin: "0 auto", lineHeight: 1.6 };

// Keyframes for slide animation
const styleSheet = document.createElement("style");
styleSheet.textContent = `
  @keyframes slideIn {
    from { transform: translateX(100%); opacity: 0; }
    to { transform: translateX(0); opacity: 1; }
  }
`;
document.head.appendChild(styleSheet);
