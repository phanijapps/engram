//! Ask tab — agentic RAG. The LLM calls tools (recall, list_memories, etc.)
//! to find context, reasons over the results, and answers. Shows the full tool
//! call trace so you can see what the agent did.

import { useState, type CSSProperties, type ReactNode } from "react";
import { Sparkles, Loader2, AlertTriangle, Wrench } from "lucide-react";

import { api } from "../../lib/api.ts";

/** Minimal markdown renderer (React nodes, no innerHTML/XSS risk). */
function Markdown({ text }: { text: string }) {
  const lines = text.split("\n");
  const out: ReactNode[] = [];
  let list: ReactNode[] = [];
  const flush = () => { if (list.length) { out.push(<ul key={`u${out.length}`} style={mdUl}>{list}</ul>); list = []; } };
  lines.forEach((ln, i) => {
    const t = ln.trim();
    if (!t) { flush(); return; }
    if (t.startsWith("### ")) { flush(); out.push(<h4 key={i} style={mdH4}>{parts(t.slice(4))}</h4>); }
    else if (/^#{1,2}\s/.test(t)) { flush(); out.push(<h3 key={i} style={mdH3}>{parts(t.replace(/^#+\s/, ""))}</h3>); }
    else if (/^[-*]\s/.test(t)) { list.push(<li key={i} style={mdLi}>{parts(t.slice(2))}</li>); }
    else if (/^\d+\.\s/.test(t)) { list.push(<li key={i} style={mdLi}>{parts(t.replace(/^\d+\.\s/, ""))}</li>); }
    else { flush(); out.push(<p key={i} style={mdP}>{parts(t)}</p>); }
  });
  flush();
  return <>{out}</>;
}

function parts(text: string): ReactNode[] {
  const result: ReactNode[] = [];
  let rest = text;
  let key = 0;
  while (rest) {
    let best = -1;
    let bestMatch: RegExpExecArray | null = null;
    for (const re of [/\*\*(.+?)\*\*/, /\*([^*]+?)\*/, /`([^`]+)`/]) {
      const m = new RegExp(re.source, "s").exec(rest);
      if (m && (best === -1 || m.index < best)) { best = m.index; bestMatch = m; }
    }
    if (bestMatch && best >= 0) {
      if (best > 0) result.push(rest.slice(0, best));
      const [full, inner] = bestMatch;
      if (full.startsWith("**")) result.push(<strong key={key++}>{inner}</strong>);
      else if (full.startsWith("`")) result.push(<code key={key++} style={codeStyle}>{inner}</code>);
      else result.push(<em key={key++}>{inner}</em>);
      rest = rest.slice(best + full.length);
    } else { result.push(rest); break; }
  }
  return result;
}

const mdH3: CSSProperties = { margin: "var(--spacing-3) 0 var(--spacing-1)", fontSize: 15, color: "var(--foreground)" };
const mdH4: CSSProperties = { margin: "var(--spacing-2) 0 var(--spacing-1)", fontSize: 13, color: "var(--foreground)" };
const mdP: CSSProperties = { margin: "0 0 var(--spacing-2)", lineHeight: 1.6, color: "var(--foreground)", fontSize: 13 };
const mdUl: CSSProperties = { margin: "0 0 var(--spacing-2)", paddingLeft: "var(--spacing-4)" };
const mdLi: CSSProperties = { margin: "0 0 4px", lineHeight: 1.5, fontSize: "13px", color: "var(--foreground)" };
const codeStyle: CSSProperties = { fontFamily: "var(--font-mono)", fontSize: 12, background: "var(--background)", padding: "1px 4px", borderRadius: "3px" };

interface TraceEntry {
  tool: string;
  args: Record<string, unknown>;
  result: unknown;
  error?: string;
}
interface AskResult {
  answer: string;
  trace: TraceEntry[];
  rounds: number;
  maxedOut?: boolean;
  sources?: unknown[];
}

export function AskTab() {
  const [question, setQuestion] = useState("");
  const [result, setResult] = useState<AskResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const ask = async (): Promise<void> => {
    if (!question.trim() || loading) return;
    setLoading(true);
    setError(null);
    setResult(null);
    try {
      const r = await api.ask(question.trim());
      setResult(r);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div style={wrap}>
      <div style={toolbar}>
        <span style={title}>ASK ENGRAM</span>
        <span style={hint}>agentic RAG · LLM calls tools autonomously</span>
      </div>

      <div style={body}>
        <div style={questionRow}>
          <input
            style={input}
            placeholder="ask anything — the agent will search for you…"
            value={question}
            onChange={(e) => setQuestion(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter") void ask(); }}
          />
          <button
            style={loading || !question.trim() ? btnDisabled : btn}
            onClick={() => void ask()}
            disabled={loading || !question.trim()}
          >
            {loading ? <Loader2 style={{ width: 14, height: 14, animation: "spin 1s linear infinite" }} /> : <Sparkles style={{ width: 14, height: 14 }} />}
            {loading ? "agent working…" : "Ask"}
          </button>
        </div>

        {loading && (
          <div style={loadingCard}>
            <Loader2 style={{ width: 14, height: 14, animation: "spin 1s linear infinite" }} />
            <span>Agent is calling tools + reasoning… (this takes a few rounds)</span>
          </div>
        )}

        {error && (
          <div style={errorCard}>
            <AlertTriangle style={{ width: 14, height: 14 }} /> {error}
          </div>
        )}

        {result && (
          <>
            {/* Tool call trace */}
            {result.trace.length > 0 && (
              <div style={traceSection}>
                <div style={traceHead}>
                  <Wrench style={{ width: 11, height: 11 }} />
                  <span style={traceLabel}>TOOL CALLS</span>
                  <span style={muted}>· {result.trace.length} calls · {result.rounds} rounds</span>
                </div>
                {result.trace.map((t, i) => (
                  <div key={i} style={traceRow}>
                    <span style={toolName}>{t.tool}</span>
                    <span style={toolArgs}>({formatArgs(t.args)})</span>
                    <span style={toolResult}>
                      {t.error ? `❌ ${t.error.slice(0, 60)}` : `✓ ${summarizeResult(t.result)}`}
                    </span>
                  </div>
                ))}
              </div>
            )}

            {/* Answer */}
            <div style={answerCard}>
              <div style={answerHead}>
                <Sparkles style={{ width: 12, height: 12 }} />
                <span style={answerLabel}>ANSWER</span>
              </div>
              <div style={answerContent}>
                {result.answer
                  ? <Markdown text={result.answer} />
                  : <p style={answerText}>(no text answer — the agent may have only called tools)</p>}
              </div>
            </div>
          </>
        )}

        {!loading && !error && !result && (
          <div style={placeholder}>
            <Sparkles style={{ width: 24, height: 24, opacity: 0.3 }} />
            <div>Type a question. The LLM will autonomously call tools</div>
            <div style={{ opacity: 0.6 }}>(recall, list_memories, graph_overview) to find the answer.</div>
          </div>
        )}
      </div>
    </div>
  );
}

function formatArgs(args: Record<string, unknown>): string {
  const entries = Object.entries(args);
  if (entries.length === 0) return "";
  return entries.map(([k, v]) => `${k}: ${typeof v === "string" ? `"${v.slice(0, 40)}"` : JSON.stringify(v)}`).join(", ");
}

function summarizeResult(result: unknown): string {
  if (result && typeof result === "object" && "count" in result) {
    return `${(result as { count: number }).count} items`;
  }
  if (result && typeof result === "object" && "ok" in result) {
    return `ok (${(result as { id?: string }).id ?? ""})`;
  }
  if (result && typeof result === "object" && "communities" in result) {
    return `${(result as { communities: unknown[] }).communities.length} communities`;
  }
  return "done";
}

const mono = "var(--font-mono)" as const;

const wrap: CSSProperties = { display: "flex", flexDirection: "column", width: "100%", height: "100%" };
const toolbar: CSSProperties = {
  display: "flex", alignItems: "baseline", gap: "var(--spacing-3)",
  padding: "var(--spacing-2) var(--spacing-4)", borderBottom: "1px solid var(--border)",
  background: "var(--sidebar)", fontFamily: mono, flex: "0 0 auto",
};
const title: CSSProperties = { letterSpacing: "0.12em", color: "var(--muted-foreground)", fontSize: 11 };
const hint: CSSProperties = { color: "var(--muted-foreground)", fontSize: 11, opacity: 0.6 };
const body: CSSProperties = {
  flex: "1 1 auto", minHeight: 0, overflow: "auto", padding: "var(--spacing-4)",
  display: "flex", flexDirection: "column", gap: "var(--spacing-3)",
};
const questionRow: CSSProperties = { display: "flex", gap: "var(--spacing-2)" };
const input: CSSProperties = {
  flex: "1 1 auto", background: "var(--background)", border: "1px solid var(--border)",
  borderRadius: "var(--radius-sm)", padding: "var(--spacing-2) var(--spacing-3)",
  color: "var(--foreground)", fontFamily: mono, fontSize: 13, outline: "none",
};
const btnBase: CSSProperties = {
  display: "inline-flex", alignItems: "center", gap: 6, fontFamily: mono, fontSize: 12,
  border: "1px solid var(--primary)", borderRadius: "var(--radius-sm)",
  cursor: "pointer", padding: "var(--spacing-2) var(--spacing-3)", color: "var(--foreground)",
  background: "transparent", whiteSpace: "nowrap",
};
const btn: CSSProperties = {
  ...btnBase,
  background: "var(--primary)",
  color: "var(--primary-foreground)",
  fontWeight: 600,
};
const btnDisabled: CSSProperties = { ...btnBase, opacity: 0.4, cursor: "not-allowed" };

const loadingCard: CSSProperties = {
  display: "flex", alignItems: "center", gap: "var(--spacing-2)", fontFamily: mono, fontSize: 12,
  color: "var(--primary)", padding: "var(--spacing-3)", background: "var(--sidebar)",
  border: "1px solid var(--border)", borderRadius: "var(--radius-md)",
};
const errorCard: CSSProperties = {
  display: "flex", alignItems: "center", gap: 6, fontFamily: mono, fontSize: 12,
  color: "var(--destructive)", padding: "var(--spacing-3)", background: "var(--sidebar)",
  border: "1px solid var(--border)", borderRadius: "var(--radius-md)",
};

const traceSection: CSSProperties = { display: "flex", flexDirection: "column", gap: 2 };
const traceHead: CSSProperties = { display: "flex", alignItems: "center", gap: 4, marginBottom: 4 };
const traceLabel: CSSProperties = { fontFamily: mono, fontSize: 10, letterSpacing: "0.08em", color: "var(--muted-foreground)" };
const traceRow: CSSProperties = {
  display: "flex", alignItems: "center", gap: "var(--spacing-2)", padding: "2px var(--spacing-2)",
  borderBottom: "1px solid var(--border)", fontFamily: mono, fontSize: 11,
};
const toolName: CSSProperties = { color: "var(--primary)", fontWeight: 600, minWidth: 80 };
const toolArgs: CSSProperties = { color: "var(--muted-foreground)", flex: "0 1 auto", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" };
const toolResult: CSSProperties = { marginLeft: "auto", color: "var(--muted-foreground)", whiteSpace: "nowrap" };

const answerCard: CSSProperties = {
  background: "var(--sidebar)", border: "1px solid var(--primary)",
  borderRadius: "var(--radius-md)", padding: "var(--spacing-4)",
};
const answerHead: CSSProperties = { display: "flex", alignItems: "center", gap: 6, marginBottom: "var(--spacing-2)" };
const answerLabel: CSSProperties = { fontFamily: mono, fontSize: 10, letterSpacing: "0.1em", color: "var(--primary)" };
const answerContent: CSSProperties = { color: "var(--foreground)" };
const answerText: CSSProperties = { margin: 0, color: "var(--foreground)", fontSize: 14, lineHeight: 1.6 };
const muted: CSSProperties = { opacity: 0.5, fontFamily: mono, fontSize: 10 };
const placeholder: CSSProperties = { textAlign: "center", padding: "var(--spacing-8)", color: "var(--muted-foreground)", fontFamily: mono, fontSize: 12, display: "flex", flexDirection: "column", alignItems: "center", gap: "var(--spacing-2)" };
