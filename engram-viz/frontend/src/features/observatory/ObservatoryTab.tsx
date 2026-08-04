//! Observatory tab (S4) — engram's graph/belief/hierarchy health at a glance.
//! Reuses viz-foundation's deck.gl community-overview canvas (no new rendering
//! dep) + a LearningHealthBar over /graph/stats. Belief/hierarchy tables are empty
//! today → honest empty-states with the out-of-band populate pointer (reflection /
//! hierarchy_build run via engram-mcp, never from this read-only viz).

import { useEffect, useState, type CSSProperties } from "react";

import { api, type GraphStats } from "../../lib/api.ts";
import { GraphOverview } from "../graph/GraphOverview.tsx";

export function ObservatoryTab() {
  const [stats, setStats] = useState<GraphStats | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    api
      .stats()
      .then((s) => !cancelled && setStats(s))
      .catch((e) => !cancelled && setError(e instanceof Error ? e.message : String(e)));
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <div style={{ position: "relative", width: "100%", height: "100%" }}>
      <GraphOverview />
      <LearningHealthBar stats={stats} error={error} />
    </div>
  );
}

interface Segment {
  label: string;
  value: string;
  populated: boolean;
  emptyNote?: string;
}

function buildSegments(s: GraphStats): Segment[] {
  return [
    {
      label: "Graph",
      value: `${fmt(s.entities)} entities · ${fmt(s.relationships)} rels`,
      populated: s.entities > 0,
    },
    {
      label: "Memory",
      value: `${fmt(s.memories)} facts`,
      populated: s.memories > 0,
    },
    {
      label: "Beliefs",
      value: `${fmt(s.beliefs)}`,
      populated: s.beliefs > 0,
      emptyNote: "Synthesize via reflection (engram-mcp).",
    },
    {
      label: "Hierarchy",
      value: `${fmt(s.hierarchyNodes)} nodes · ${fmt(s.hierarchyRelations)} rels`,
      populated: s.hierarchyNodes > 0,
      emptyNote: "Build via hierarchy_build (engram-mcp).",
    },
  ];
}

function LearningHealthBar({
  stats,
  error,
}: {
  stats: GraphStats | null;
  error: string | null;
}) {
  return (
    <div style={barStyle}>
      <div style={titleStyle}>LEARNING HEALTH</div>
      {error && <div style={rowValueStyle}>stats unavailable</div>}
      {!error && !stats && <div style={rowValueStyle}>loading…</div>}
      {stats &&
        buildSegments(stats).map((seg) => (
          <div key={seg.label} style={rowStyle}>
            <span style={seg.populated ? dotOkStyle : dotEmptyStyle} />
            <span style={rowLabelStyle}>{seg.label}</span>
            <span style={rowValueStyle}>{seg.value}</span>
            {!seg.populated && seg.emptyNote && (
              <span style={noteStyle}>{seg.emptyNote}</span>
            )}
          </div>
        ))}
    </div>
  );
}

function fmt(n: number): string {
  if (n >= 1000) return `${(n / 1000).toFixed(n >= 10000 ? 0 : 1)}k`;
  return String(n);
}

const mono = "var(--font-mono)" as const;
const barStyle: CSSProperties = {
  position: "absolute",
  top: "var(--spacing-3)",
  left: "var(--spacing-3)",
  minWidth: 260,
  maxWidth: 340,
  background: "var(--sidebar)",
  border: "1px solid var(--border)",
  borderRadius: "var(--radius-md)",
  padding: "var(--spacing-3)",
  zIndex: 5,
  fontFamily: mono,
  display: "flex",
  flexDirection: "column",
  gap: "var(--spacing-2)",
};
const titleStyle: CSSProperties = {
  fontSize: 10,
  letterSpacing: "0.12em",
  color: "var(--muted-foreground)",
};
const rowStyle: CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: "var(--spacing-2)",
  fontSize: 12,
  flexWrap: "wrap",
};
const dotBase: CSSProperties = { width: 7, height: 7, borderRadius: "50%", flex: "0 0 auto" };
const dotOkStyle: CSSProperties = { ...dotBase, background: "var(--primary, #7df9ff)" };
const dotEmptyStyle: CSSProperties = { ...dotBase, background: "transparent", border: "1px solid var(--border)" };
const rowLabelStyle: CSSProperties = { color: "var(--foreground)", minWidth: 64 };
const rowValueStyle: CSSProperties = { color: "var(--muted-foreground)" };
const noteStyle: CSSProperties = {
  color: "var(--muted-foreground)",
  fontSize: 10,
  width: "100%",
  paddingLeft: 15,
};
