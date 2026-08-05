//! Maintain tab — run pi-mono LLM maintenance (reflect-llm / contradict-llm) +
//! deterministic consolidate from the Control Center, and browse beliefs +
//! contradictions. The op runs in a child process via the BFF (the LLM call is
//! long + network-bound; same nested-executor-safe pattern as Ingest). LLM ops
//! route scope data to a cloud model → a confirm gate + disclosure before run.

import { useEffect, useRef, useState, type CSSProperties } from "react";
import { Loader2, AlertTriangle, CheckCircle2 } from "lucide-react";

import {
  api,
  type MaintainJob,
  type MaintainOp,
} from "../../lib/api.ts";

interface OpDef {
  op: MaintainOp;
  label: string;
  llm: boolean;
  desc: string;
}
const OPS: OpDef[] = [
  { op: "reflect-llm", label: "Reflect", llm: true, desc: "LLM — synthesize beliefs from memories" },
  { op: "contradict-llm", label: "Contradict", llm: true, desc: "LLM — detect semantic contradictions" },
  { op: "consolidate", label: "Consolidate", llm: false, desc: "Deterministic reflection + decay (no LLM)" },
];

export function MaintainTab() {
  const [op, setOp] = useState<MaintainOp>("reflect-llm");
  const [job, setJob] = useState<MaintainJob | null>(null);
  const [starting, setStarting] = useState(false);
  const [beliefs, setBeliefs] = useState<unknown[] | null>(null);
  const [contras, setContras] = useState<unknown[] | null>(null);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const selected = OPS.find((o) => o.op === op)!;

  const refresh = (): void => {
    api.maintainBeliefs().then(setBeliefs).catch(() => {});
    api.maintainContradictions().then(setContras).catch(() => {});
  };
  useEffect(() => {
    refresh();
    return () => {
      if (pollRef.current) clearInterval(pollRef.current);
    };
  }, []);

  const stopPoll = (): void => {
    if (pollRef.current) {
      clearInterval(pollRef.current);
      pollRef.current = null;
    }
  };
  const watch = (jobId: string): void => {
    stopPoll();
    const tick = async (): Promise<void> => {
      try {
        const j = await api.maintainJob(jobId);
        setJob(j);
        if (j.status === "done") {
          stopPoll();
          refresh();
        } else if (j.status === "error") {
          stopPoll();
        }
      } catch (e) {
        setJob({ jobId, op, status: "error", error: e instanceof Error ? e.message : String(e) });
        stopPoll();
      }
    };
    void tick();
    pollRef.current = setInterval(tick, 1000);
  };

  const start = async (): Promise<void> => {
    if (job?.status === "running") return;
    if (selected.llm) {
      const what = op === "reflect-llm" ? "memories" : "beliefs";
      const ok = window.confirm(
        `"${selected.label}" routes the scope's ${what} to a cloud LLM (Anthropic by default; set PI_PROVIDER=ollama for a local model). Continue?`,
      );
      if (!ok) return;
    }
    setStarting(true);
    setJob(null);
    try {
      const { jobId } = await api.runMaintain(op);
      setJob({ jobId, op, status: "running", startedAt: Date.now() });
      watch(jobId);
    } catch (e) {
      setJob({
        jobId: "",
        op,
        status: "error",
        error: e instanceof Error ? e.message : String(e),
      });
    } finally {
      setStarting(false);
    }
  };

  const running = job?.status === "running";

  return (
    <div style={wrap}>
      <div style={toolbar}>
        <span style={title}>MAINTAIN</span>
        <span style={hint}>run reflection / contradiction / consolidation over the agentzero scope</span>
      </div>

      <div style={body}>
        <section style={card}>
          <div style={formRow}>
            {OPS.map((o) => (
              <button
                key={o.op}
                style={o.op === op ? opActive : opBtn}
                onClick={() => setOp(o.op)}
                title={o.desc}
              >
                {o.label}
                {o.llm ? " ✨" : ""}
              </button>
            ))}
            <button
              style={running || starting ? runDisabled : runBtn}
              onClick={() => void start()}
              disabled={running || starting}
            >
              {starting ? "starting…" : running ? "running…" : "Run"}
            </button>
          </div>
          <div style={{ ...hint, marginTop: 6 }}>{selected.desc}</div>
          {selected.llm && (
            <div style={warnLine}>
              <AlertTriangle style={{ width: 12, height: 12 }} aria-hidden /> Routes scope data to a
              cloud LLM — you'll confirm before run.
            </div>
          )}
        </section>

        {job && <JobMonitor job={job} />}

        <BeliefsPanel beliefs={beliefs} />
        <ContradictionsPanel contras={contras} />
      </div>
    </div>
  );
}

function JobMonitor({ job }: { job: MaintainJob }) {
  if (job.status === "running") {
    return (
      <section style={{ ...card, borderColor: "var(--primary, #7df9ff)" }}>
        <div style={{ ...formRow, color: "var(--primary, #7df9ff)" }}>
          <Loader2 style={{ width: 14, height: 14, animation: "spin 1s linear infinite" }} aria-hidden />
          <span>Running — this may take a while…</span>
        </div>
      </section>
    );
  }
  if (job.status === "error") {
    return (
      <section style={{ ...card, borderColor: "var(--destructive, #f87171)" }}>
        <div style={{ ...warnLine, color: "var(--destructive, #f87171)" }}>
          <AlertTriangle style={{ width: 14, height: 14 }} aria-hidden /> {job.op} failed
        </div>
        <div style={{ fontFamily: mono, fontSize: 12, marginTop: 6, color: "var(--muted-foreground)" }}>
          {job.error ?? "unknown error"}
        </div>
      </section>
    );
  }
  const r = job.result ?? {};
  const stats = [
    ["memories read", r.memoriesRead],
    ["beliefs read", r.beliefsRead],
    ["beliefs written", r.beliefsWritten],
    ["contradictions", r.contradictionsWritten],
    ["skipped", r.skipped],
  ].filter(([, v]) => v !== undefined && v !== null);
  return (
    <section style={{ ...card, borderColor: "var(--success, #34d399)" }}>
      <div style={{ ...formRow, color: "var(--success, #34d399)" }}>
        <CheckCircle2 style={{ width: 14, height: 14 }} aria-hidden /> {job.op} complete
      </div>
      {stats.length > 0 && (
        <div style={summaryGrid}>
          {stats.map(([k, v]) => (
            <Stat key={k as string} label={k as string} v={v as number} />
          ))}
        </div>
      )}
    </section>
  );
}

function BeliefsPanel({ beliefs }: { beliefs: unknown[] | null }) {
  return (
    <Panel title="BELIEFS" items={beliefs} render={(b) => {
      const x = b as { subject?: { key?: string }; content?: string; provenance?: { method?: string }; confidence?: number };
      return (
        <>
          <span style={{ color: "var(--foreground)" }}>{x.subject?.key ?? "?"}</span>
          <span style={muted}> · {x.provenance?.method ?? "?"}{x.confidence !== undefined ? ` · ${(x.confidence * 100).toFixed(0)}%` : ""}</span>
          <div style={{ ...muted, marginTop: 2 }}>{x.content ?? ""}</div>
        </>
      );
    }} />
  );
}

function ContradictionsPanel({ contras }: { contras: unknown[] | null }) {
  return (
    <Panel title="CONTRADICTIONS" items={contras} render={(c) => {
      const x = c as { kind?: string; severity?: number; reasoning?: string; targets?: Array<{ targetId?: string }> };
      return (
        <>
          <span style={{ color: "var(--foreground)" }}>{x.kind ?? "?"}</span>
          <span style={muted}>{x.severity !== undefined ? ` · ${(x.severity * 100).toFixed(0)}%` : ""} · {(x.targets ?? []).length} targets</span>
          <div style={{ ...muted, marginTop: 2 }}>{x.reasoning ?? ""}</div>
        </>
      );
    }} />
  );
}

function Panel({ title, items, render }: { title: string; items: unknown[] | null; render: (item: unknown) => React.ReactNode }) {
  return (
    <section style={card}>
      <div style={{ ...kindLabel, marginBottom: 8 }}>
        {title} {items !== null && <span style={muted}>· {items.length}</span>}
      </div>
      {items === null ? (
        <div style={muted}>Loading…</div>
      ) : items.length === 0 ? (
        <div style={muted}>None yet — run Reflect / Contradict to populate.</div>
      ) : (
        <div style={{ display: "flex", flexDirection: "column", gap: "var(--spacing-2)" }}>
          {items.slice(0, 50).map((it, i) => (
            <div key={i} style={rowCard}>{render(it)}</div>
          ))}
          {items.length > 50 && <div style={muted}>… {items.length - 50} more</div>}
        </div>
      )}
    </section>
  );
}

function Stat({ label, v }: { label: string; v?: number }) {
  return (
    <div style={statCell}>
      <div style={statValue}>{v}</div>
      <div style={statLabel}>{label}</div>
    </div>
  );
}

const mono = "var(--font-mono)" as const;
const wrap: CSSProperties = { display: "flex", flexDirection: "column", width: "100%", height: "100%" };
const toolbar: CSSProperties = {
  display: "flex", alignItems: "baseline", gap: "var(--spacing-3)",
  padding: "var(--spacing-2) var(--spacing-4)", borderBottom: "1px solid var(--border)",
  background: "var(--sidebar)", fontFamily: mono, flex: "0 0 auto",
};
const title: CSSProperties = { letterSpacing: "0.12em", color: "var(--muted-foreground)", fontSize: 11 };
const hint: CSSProperties = { color: "var(--muted-foreground)", fontSize: 11, opacity: 0.7 };
const body: CSSProperties = {
  flex: "1 1 auto", minHeight: 0, overflow: "auto", padding: "var(--spacing-4)",
  display: "flex", flexDirection: "column", gap: "var(--spacing-3)",
};
const card: CSSProperties = {
  background: "var(--sidebar)", border: "1px solid var(--border)",
  borderRadius: "var(--radius-md)", padding: "var(--spacing-3) var(--spacing-4)", fontFamily: mono,
};
const formRow: CSSProperties = { display: "flex", alignItems: "center", gap: "var(--spacing-2)", fontSize: 12 };
const btnBase: CSSProperties = {
  background: "transparent", border: "1px solid var(--border)", color: "var(--muted-foreground)",
  borderRadius: "var(--radius-sm)", cursor: "pointer", fontFamily: mono, fontSize: 11, padding: "2px var(--spacing-2)",
};
const opBtn: CSSProperties = { ...btnBase };
const opActive: CSSProperties = { ...btnBase, color: "var(--primary, #7df9ff)", borderColor: "var(--primary, #7df9ff)" };
const runBtn: CSSProperties = { ...btnBase, marginLeft: "auto", color: "var(--foreground)", borderColor: "var(--primary, #7df9ff)" };
const runDisabled: CSSProperties = { ...runBtn, opacity: 0.5, cursor: "not-allowed" };
const kindLabel: CSSProperties = { color: "var(--muted-foreground)", fontSize: 10, letterSpacing: "0.08em" };
const warnLine: CSSProperties = {
  display: "flex", alignItems: "center", gap: 6, fontSize: 11, marginTop: 8, color: "var(--muted-foreground)",
};
const summaryGrid: CSSProperties = {
  display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(110px, 1fr))", gap: "var(--spacing-2)", marginTop: 10,
};
const statCell: CSSProperties = { background: "var(--background)", border: "1px solid var(--border)", borderRadius: "var(--radius-sm)", padding: "var(--spacing-2)" };
const statValue: CSSProperties = { fontSize: 16, color: "var(--foreground)" };
const statLabel: CSSProperties = { fontSize: 10, letterSpacing: "0.06em", color: "var(--muted-foreground)", marginTop: 2 };
const rowCard: CSSProperties = { background: "var(--background)", border: "1px solid var(--border)", borderRadius: "var(--radius-sm)", padding: "var(--spacing-2)", fontSize: 11 };
const muted: CSSProperties = { color: "var(--muted-foreground)", fontSize: 11 };
