//! Entity-detail panel (drill right-rail). Shows the drilled community's stats
//! and, when a member is selected, its detail: kind, community, outgoing degree,
//! provenance. Reads the drill store; closes via clearDrill.

import type { CSSProperties } from "react";

import { useGraphStore } from "../../store/graph.ts";
import type { EntityDetail as EntityDetailType } from "../../lib/api.ts";

interface Prov {
  source?: string;
  method?: string;
  observedAt?: string;
  confidence?: number;
  actor?: { displayName?: string };
}

export function EntityDetailPanel() {
  const community = useGraphStore((s) => s.community);
  const members = useGraphStore((s) => s.members);
  const selectedEntityId = useGraphStore((s) => s.selectedEntityId);
  const detail = useGraphStore((s) => s.detail);
  const detailLoading = useGraphStore((s) => s.detailLoading);
  const clearDrill = useGraphStore((s) => s.clearDrill);
  if (!community) return null;

  return (
    <aside style={panelStyle}>
      <div style={headerStyle}>
        <div>
          <div style={titleStyle}>{community.name}</div>
          <div style={subStyle}>
            {community.memberCount} members · {members.length} shown
          </div>
        </div>
        <button style={closeStyle} onClick={clearDrill} aria-label="close drill">
          ✕
        </button>
      </div>

      <div style={hintStyle}>Select a violet node to inspect an entity.</div>

      {selectedEntityId &&
        (detailLoading ? (
          <p style={muted}>Loading detail…</p>
        ) : detail ? (
          <DetailBody d={detail} />
        ) : (
          <p style={muted}>No detail available.</p>
        ))}
    </aside>
  );
}

function DetailBody({ d }: { d: EntityDetailType }) {
  const p = (d.provenance ?? null) as Prov | null;
  return (
    <div style={bodyStyle}>
      <Row label="kind" value={d.kind} />
      <Row label="name" value={d.name} />
      <Row label="community" value={d.community === null ? "—" : `c${d.community}`} />
      <Row label="degree" value={String(d.degree)} />
      {p && (
        <>
          {p.source && <Row label="source" value={p.source} />}
          {p.method && <Row label="method" value={p.method} />}
          {p.observedAt && (
            <Row label="observed" value={p.observedAt.slice(0, 10)} />
          )}
          {p.confidence !== undefined && (
            <Row label="confidence" value={p.confidence.toFixed(2)} />
          )}
        </>
      )}
    </div>
  );
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div style={rowStyle}>
      <span style={labelStyle}>{label}</span>
      <span style={valueStyle} title={value}>
        {value}
      </span>
    </div>
  );
}

const mono = "var(--font-mono)" as const;
const muted = { fontFamily: mono, color: "var(--muted-foreground)" } as const;

const panelStyle: CSSProperties = {
  position: "absolute",
  top: "var(--spacing-3)",
  right: "var(--spacing-3)",
  width: 300,
  maxHeight: "calc(100% - calc(var(--spacing-3) * 2))",
  overflowY: "auto",
  background: "var(--sidebar)",
  border: "1px solid var(--border)",
  borderRadius: "var(--radius-md)",
  padding: "var(--spacing-3)",
  zIndex: 5,
  fontFamily: mono,
};
const headerStyle: CSSProperties = {
  display: "flex",
  justifyContent: "space-between",
  alignItems: "flex-start",
  gap: "var(--spacing-2)",
};
const titleStyle: CSSProperties = {
  color: "var(--foreground)",
  fontSize: 14,
  fontWeight: 600,
};
const subStyle: CSSProperties = { color: "var(--muted-foreground)", fontSize: 11 };
const closeStyle: CSSProperties = {
  background: "transparent",
  border: "1px solid var(--border)",
  color: "var(--muted-foreground)",
  borderRadius: "var(--radius-sm)",
  cursor: "pointer",
  fontSize: 12,
  lineHeight: 1,
  padding: "2px 6px",
};
const hintStyle: CSSProperties = {
  color: "var(--muted-foreground)",
  fontSize: 11,
  margin: "var(--spacing-2) 0",
};
const bodyStyle: CSSProperties = {
  borderTop: "1px solid var(--border)",
  paddingTop: "var(--spacing-2)",
  display: "flex",
  flexDirection: "column",
  gap: "var(--spacing-1)",
};
const rowStyle: CSSProperties = {
  display: "flex",
  justifyContent: "space-between",
  gap: "var(--spacing-2)",
  fontSize: 12,
};
const labelStyle: CSSProperties = { color: "var(--muted-foreground)" };
const valueStyle: CSSProperties = {
  color: "var(--foreground)",
  overflow: "hidden",
  textOverflow: "ellipsis",
  whiteSpace: "nowrap",
  maxWidth: 180,
};
