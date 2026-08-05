//! Ingest tab — drive + monitor a code/doc scan from the Control Center. Starts
//! a scan via the BFF (which spawns the `engram-ingest` CLI in a child process),
//! polls the job to terminal, and shows record counts. Terminal progress only
//! ("this may take a while") — no live per-file stream in this slice. On done the
//! Graph-tab counts rise (the scan writes to the agentzero store the BFF reads).

import { useEffect, useRef, useState, type CSSProperties } from "react";
import { FolderInput, Loader2, AlertTriangle, CheckCircle2 } from "lucide-react";

import {
  api,
  type IngestCounts,
  type IngestJob,
} from "../../lib/api.ts";

type Kind = "code" | "doc" | "auto";
const KINDS: Kind[] = ["auto", "code", "doc"];

export function IngestTab() {
  const [root, setRoot] = useState("");
  const [kind, setKind] = useState<Kind>("auto");
  const [job, setJob] = useState<IngestJob | null>(null);
  const [starting, setStarting] = useState(false);
  const [startError, setStartError] = useState<string | null>(null);
  const [counts, setCounts] = useState<IngestCounts | null>(null);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const refreshCounts = (): void => {
    api.ingestCounts().then(setCounts).catch(() => {});
  };

  useEffect(() => {
    refreshCounts();
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
        const j = await api.scanJob(jobId);
        setJob(j);
        if (j.status === "done") {
          stopPoll();
          refreshCounts();
        } else if (j.status === "error") {
          stopPoll();
        }
      } catch (e) {
        setJob({
          jobId,
          status: "error",
          error: e instanceof Error ? e.message : String(e),
        });
        stopPoll();
      }
    };
    void tick();
    pollRef.current = setInterval(tick, 1000);
  };

  const start = async (): Promise<void> => {
    if (!root.trim() || job?.status === "running") return;
    setStarting(true);
    setStartError(null);
    setJob(null);
    try {
      const { jobId } = await api.startScan(root.trim(), kind);
      setJob({ jobId, status: "running", startedAt: Date.now() });
      watch(jobId);
    } catch (e) {
      setStartError(e instanceof Error ? e.message : String(e));
    } finally {
      setStarting(false);
    }
  };

  const running = job?.status === "running";

  return (
    <div style={wrap}>
      <div style={toolbar}>
        <span style={title}>INGEST</span>
        <span style={hint}>scan a path into the agentzero store · runs in a separate process</span>
      </div>

      <div style={body}>
        <section style={card}>
          <div style={formRow}>
            <FolderInput style={{ width: 15, height: 15, opacity: 0.6 }} aria-hidden />
            <input
              style={pathInput}
              placeholder="/absolute/path/to/repo-or-docs"
              value={root}
              onChange={(e) => setRoot(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") void start();
              }}
            />
          </div>
          <div style={formRow}>
            <span style={kindLabel}>KIND</span>
            {KINDS.map((k) => (
              <button
                key={k}
                style={k === kind ? kindActive : kindBtn}
                onClick={() => setKind(k)}
              >
                {k}
              </button>
            ))}
            <button
              style={running || starting || !root.trim() ? startBtnDisabled : startBtn}
              onClick={() => void start()}
              disabled={running || starting || !root.trim()}
            >
              {starting ? "starting…" : running ? "scanning…" : "Start scan"}
            </button>
          </div>
          {startError && (
            <div style={errLine}>
              <AlertTriangle style={{ width: 13, height: 13 }} aria-hidden /> {startError}
            </div>
          )}
        </section>

        {job && <JobMonitor job={job} />}

        <CountsPanel counts={counts} />
      </div>
    </div>
  );
}

function JobMonitor({ job }: { job: IngestJob }) {
  if (job.status === "running") {
    return (
      <section style={{ ...card, borderColor: "var(--primary, #7df9ff)" }}>
        <div style={{ ...formRow, color: "var(--primary, #7df9ff)" }}>
          <Loader2 style={{ width: 14, height: 14, animation: "spin 1s linear infinite" }} aria-hidden />
          <span>Running — this may take a while…</span>
          <span style={muted}>job {job.jobId}</span>
        </div>
      </section>
    );
  }
  if (job.status === "error") {
    return (
      <section style={{ ...card, borderColor: "var(--destructive, #f87171)" }}>
        <div style={{ ...errLine, color: "var(--destructive, #f87171)" }}>
          <AlertTriangle style={{ width: 14, height: 14 }} aria-hidden /> Scan failed
        </div>
        <div style={{ fontFamily: mono, fontSize: 12, marginTop: 6, color: "var(--muted-foreground)" }}>
          {job.error ?? "unknown error"}
        </div>
      </section>
    );
  }
  // done
  const s = job.summary ?? {};
  return (
    <section style={{ ...card, borderColor: "var(--success, #34d399)" }}>
      <div style={{ ...formRow, color: "var(--success, #34d399)" }}>
        <CheckCircle2 style={{ width: 14, height: 14 }} aria-hidden /> Scan complete
      </div>
      <div style={summaryGrid}>
        <Stat label="scanned" v={s.scanned} />
        <Stat label="ingested" v={s.ingested} />
        <Stat label="unchanged" v={s.unchanged} />
        <Stat label="skipped" v={s.skipped} />
        <Stat label="entities" v={s.entities} />
        <Stat label="relationships" v={s.relationships} />
        <Stat label="errors" v={s.errors} />
      </div>
      <div style={{ ...muted, marginTop: 8 }}>
        Counts updated — switch to the Graph tab to see new entities.
      </div>
    </section>
  );
}

function CountsPanel({ counts }: { counts: IngestCounts | null }) {
  if (!counts) {
    return (
      <section style={card}>
        <div style={muted}>Loading counts…</div>
      </section>
    );
  }
  return (
    <section style={card}>
      <div style={{ ...kindLabel, marginBottom: 8 }}>STORE COUNTS</div>
      <div style={summaryGrid}>
        <Stat label="entities" v={counts.entities} />
        <Stat label="relationships" v={counts.relationships} />
        <Stat label="memories" v={counts.memories} />
        <Stat label="beliefs" v={counts.beliefs} />
        <Stat label="hierarchy nodes" v={counts.hierarchyNodes} />
        <Stat label="hierarchy rels" v={counts.hierarchyRelations} />
      </div>
    </section>
  );
}

function Stat({ label, v, zeroNote }: { label: string; v?: number | null; zeroNote?: string }) {
  const display = v === undefined || v === null ? (zeroNote ?? "—") : String(v);
  return (
    <div style={statCell}>
      <div style={statValue}>{display}</div>
      <div style={statLabel}>{label}</div>
    </div>
  );
}

const mono = "var(--font-mono)" as const;
const wrap: CSSProperties = {
  display: "flex",
  flexDirection: "column",
  width: "100%",
  height: "100%",
};
const toolbar: CSSProperties = {
  display: "flex",
  alignItems: "baseline",
  gap: "var(--spacing-3)",
  padding: "var(--spacing-2) var(--spacing-4)",
  borderBottom: "1px solid var(--border)",
  background: "var(--sidebar)",
  fontFamily: mono,
  flex: "0 0 auto",
};
const title: CSSProperties = { letterSpacing: "0.12em", color: "var(--muted-foreground)", fontSize: 11 };
const hint: CSSProperties = { color: "var(--muted-foreground)", fontSize: 11, opacity: 0.7 };
const body: CSSProperties = {
  flex: "1 1 auto",
  minHeight: 0,
  overflow: "auto",
  padding: "var(--spacing-4)",
  display: "flex",
  flexDirection: "column",
  gap: "var(--spacing-3)",
};
const card: CSSProperties = {
  background: "var(--sidebar)",
  border: "1px solid var(--border)",
  borderRadius: "var(--radius-md)",
  padding: "var(--spacing-3) var(--spacing-4)",
  fontFamily: mono,
};
const formRow: CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: "var(--spacing-2)",
  fontSize: 12,
};
const pathInput: CSSProperties = {
  flex: "1 1 auto",
  background: "var(--background)",
  border: "1px solid var(--border)",
  borderRadius: "var(--radius-sm)",
  padding: "4px var(--spacing-2)",
  color: "var(--foreground)",
  fontFamily: mono,
  fontSize: 12,
  outline: "none",
};
const btnBase: CSSProperties = {
  background: "transparent",
  border: "1px solid var(--border)",
  color: "var(--muted-foreground)",
  borderRadius: "var(--radius-sm)",
  cursor: "pointer",
  fontFamily: mono,
  fontSize: 11,
  padding: "2px var(--spacing-2)",
};
const kindBtn: CSSProperties = { ...btnBase };
const kindActive: CSSProperties = {
  ...btnBase,
  color: "var(--primary, #7df9ff)",
  borderColor: "var(--primary, #7df9ff)",
};
const startBtn: CSSProperties = {
  ...btnBase,
  marginLeft: "auto",
  color: "var(--foreground)",
  borderColor: "var(--primary, #7df9ff)",
};
const startBtnDisabled: CSSProperties = { ...startBtn, opacity: 0.5, cursor: "not-allowed" };
const kindLabel: CSSProperties = {
  color: "var(--muted-foreground)",
  fontSize: 10,
  letterSpacing: "0.08em",
};
const errLine: CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: 6,
  fontSize: 12,
  marginTop: 8,
  color: "var(--destructive, #f87171)",
};
const summaryGrid: CSSProperties = {
  display: "grid",
  gridTemplateColumns: "repeat(auto-fill, minmax(90px, 1fr))",
  gap: "var(--spacing-2)",
  marginTop: 10,
};
const statCell: CSSProperties = {
  background: "var(--background)",
  border: "1px solid var(--border)",
  borderRadius: "var(--radius-sm)",
  padding: "var(--spacing-2)",
};
const statValue: CSSProperties = { fontSize: 16, color: "var(--foreground)" };
const statLabel: CSSProperties = {
  fontSize: 10,
  letterSpacing: "0.06em",
  color: "var(--muted-foreground)",
  marginTop: 2,
};
const muted: CSSProperties = { color: "var(--muted-foreground)", fontSize: 11, fontFamily: mono };
