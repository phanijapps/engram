//! LearningHealthBar — the zbot-style bottom status strip for the Observatory.
//! Surfaces graph/memory/belief/hierarchy health at a glance. Unpopulated surfaces
//! (belief/hierarchy today: 0 rows) render dim with a details button that opens a
//! slideover carrying the honest empty-state + the out-of-band populate pointer.

import { useEffect, useState, type CSSProperties } from "react";
import { api, type GraphStats } from "../../lib/api.ts";
import { Slideover } from "./Slideover.tsx";

export function LearningHealthBar({ refreshSignal = 0 }: { refreshSignal?: number } = {}) {
  const [stats, setStats] = useState<GraphStats | null>(null);
  const [beliefOpen, setBeliefOpen] = useState(false);
  const [hierOpen, setHierOpen] = useState(false);

  useEffect(() => {
    let cancelled = false;
    api
      .stats()
      .then((s) => !cancelled && setStats(s))
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [refreshSignal]);

  return (
    <div style={stripStyle}>
      <span style={brandStyle}>LEARNING HEALTH</span>
      <Item label="Entities" value={fmt(stats?.entities)} ok={true} />
      <Item label="Relationships" value={fmt(stats?.relationships)} ok={true} />
      <Item label="Facts" value={fmt(stats?.memories)} ok={!!stats && stats.memories > 0} />
      <Item
        label="Beliefs"
        value={fmt(stats?.beliefs)}
        ok={!!stats && stats.beliefs > 0}
        onDetails={() => setBeliefOpen(true)}
      />
      <Item
        label="Hierarchy"
        value={stats ? `${fmt(stats.hierarchyNodes)}n` : "—"}
        ok={!!stats && stats.hierarchyNodes > 0}
        onDetails={() => setHierOpen(true)}
      />

      <Slideover
        open={beliefOpen}
        onClose={() => setBeliefOpen(false)}
        title="Belief Network"
        subtitle="Beliefs · contradictions · propagation"
      >
        <EmptyDetail
          what="beliefs"
          how="Run reflection via engram-mcp — the reflection synthesizer derives beliefs (and contradictions) from memories."
        />
      </Slideover>
      <Slideover
        open={hierOpen}
        onClose={() => setHierOpen(false)}
        title="Hierarchy"
        subtitle="Layers · aggregates · inter-cluster edges"
      >
        <EmptyDetail
          what="hierarchy"
          how="Run hierarchy_build via engram-mcp to synthesize aggregate layers over the knowledge graph."
        />
      </Slideover>
    </div>
  );
}

function Item({
  label,
  value,
  ok,
  onDetails,
}: {
  label: string;
  value: string;
  ok: boolean;
  onDetails?: () => void;
}) {
  return (
    <div style={itemStyle}>
      <span style={ok ? dotOkStyle : dotEmptyStyle} />
      <span style={labelStyle}>{label}</span>
      <span style={ok ? valueStyle : valueWarnStyle}>{value}</span>
      {onDetails && (
        <button style={detailsBtnStyle} onClick={onDetails} aria-label={`${label} details`}>
          ↗
        </button>
      )}
    </div>
  );
}

function EmptyDetail({ what, how }: { what: string; how: string }) {
  return (
    <div style={emptyStyle}>
      <div style={emptyTitleStyle}>No {what} synthesized</div>
      <div style={emptyBodyStyle}>{how}</div>
    </div>
  );
}

function fmt(n: number | undefined): string {
  if (n === undefined) return "—";
  if (n >= 1000) return `${(n / 1000).toFixed(n >= 10000 ? 0 : 1)}k`;
  return String(n);
}

const mono = "var(--font-mono)" as const;
const stripStyle: CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: "var(--spacing-4)",
  flexWrap: "wrap",
  padding: "var(--spacing-2) var(--spacing-4)",
  borderTop: "1px solid var(--border)",
  background: "var(--sidebar)",
  fontFamily: mono,
  fontSize: 11,
};
const brandStyle: CSSProperties = {
  letterSpacing: "0.12em",
  color: "var(--muted-foreground)",
};
const itemStyle: CSSProperties = { display: "flex", alignItems: "center", gap: "var(--spacing-1)" };
const dotBase: CSSProperties = { width: 6, height: 6, borderRadius: "50%" };
const dotOkStyle: CSSProperties = { ...dotBase, background: "var(--primary, #7df9ff)" };
const dotEmptyStyle: CSSProperties = { ...dotBase, background: "transparent", border: "1px solid var(--border)" };
const labelStyle: CSSProperties = { color: "var(--muted-foreground)" };
const valueStyle: CSSProperties = { color: "var(--foreground)" };
const valueWarnStyle: CSSProperties = { color: "var(--muted-foreground)", opacity: 0.7 };
const detailsBtnStyle: CSSProperties = {
  background: "transparent",
  border: "1px solid var(--border)",
  color: "var(--muted-foreground)",
  borderRadius: "var(--radius-sm)",
  cursor: "pointer",
  fontSize: 10,
  lineHeight: 1,
  padding: "1px 4px",
};
const emptyStyle: CSSProperties = { textAlign: "center", padding: "var(--spacing-4)" };
const emptyTitleStyle: CSSProperties = { color: "var(--foreground)", fontSize: 13, marginBottom: "var(--spacing-2)" };
const emptyBodyStyle: CSSProperties = { color: "var(--muted-foreground)", fontSize: 12, lineHeight: 1.6 };
