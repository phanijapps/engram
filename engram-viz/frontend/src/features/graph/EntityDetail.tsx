//! Entity-detail panel (drill right-rail). When a community is drilled it shows
//! what's IN it — top entity kinds + a clickable member list (so "Community 65"
//! becomes legible) — and on member select, that entity's detail (kind, community,
//! outgoing degree, provenance). Reads the drill store; closes via clearDrill.

import type { CSSProperties } from "react";

import { useGraphStore } from "../../store/graph.ts";
import type { EntityDetail as EntityDetailType, GraphEntityView } from "../../lib/api.ts";

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
  const memberEdges = useGraphStore((s) => s.memberEdges);
  const selectedEntityId = useGraphStore((s) => s.selectedEntityId);
  const detail = useGraphStore((s) => s.detail);
  const detailLoading = useGraphStore((s) => s.detailLoading);
  const selectEntity = useGraphStore((s) => s.selectEntity);
  const clearDrill = useGraphStore((s) => s.clearDrill);
  if (!community) return null;

  const kinds = topKinds(members);

  return (
    <aside style={panelStyle}>
      <div style={headerStyle}>
        <div>
          <div style={titleStyle}>{community.name}</div>
          <div style={subStyle}>
            {community.memberCount} members · {members.length} shown · {memberEdges.length} links
          </div>
        </div>
        <button style={closeStyle} onClick={clearDrill} aria-label="close drill">
          ✕
        </button>
      </div>

      {kinds.length > 0 && (
        <div style={kindsStyle}>
          {kinds.map(([kind, n]) => (
            <span key={kind} style={kindChipStyle}>
              {kind} <b style={kindCountStyle}>{n}</b>
            </span>
          ))}
        </div>
      )}

      <div style={listHeadStyle}>MEMBERS</div>
      <div style={listStyle}>
        {members.length === 0 && <div style={muted}>loading…</div>}
        {members.map((m) => (
          <button
            key={m.id}
            style={m.id === selectedEntityId ? memberActiveStyle : memberStyle}
            onClick={() => selectEntity(m.id)}
            title={m.id}
          >
            <span style={memberKindStyle}>{m.kind}</span>
            <span style={memberNameStyle}>{m.name}</span>
          </button>
        ))}
      </div>

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

/** Top entity kinds in the member sample (what the community "is"). */
function topKinds(members: GraphEntityView[]): Array<[string, number]> {
  const counts = new Map<string, number>();
  for (const m of members) counts.set(m.kind, (counts.get(m.kind) ?? 0) + 1);
  return [...counts.entries()].sort((a, b) => b[1] - a[1]).slice(0, 6);
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
          {p.observedAt && <Row label="observed" value={p.observedAt.slice(0, 10)} />}
          {p.confidence !== undefined && (
            <Row label="confidence" value={d.provenance && typeof p.confidence === "number" ? p.confidence.toFixed(2) : ""} />
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
const muted: CSSProperties = { fontFamily: mono, color: "var(--muted-foreground)", fontSize: 11 };

const panelStyle: CSSProperties = {
  position: "absolute",
  top: "var(--spacing-3)",
  right: "var(--spacing-3)",
  width: 320,
  maxHeight: "calc(100% - calc(var(--spacing-3) * 2))",
  overflowY: "auto",
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
const headerStyle: CSSProperties = {
  display: "flex",
  justifyContent: "space-between",
  alignItems: "flex-start",
  gap: "var(--spacing-2)",
};
const titleStyle: CSSProperties = { color: "var(--foreground)", fontSize: 14, fontWeight: 600 };
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
const kindsStyle: CSSProperties = { display: "flex", flexWrap: "wrap", gap: "var(--spacing-1)" };
const kindChipStyle: CSSProperties = {
  fontSize: 10,
  color: "var(--muted-foreground)",
  background: "var(--background)",
  border: "1px solid var(--border)",
  borderRadius: "var(--radius-sm)",
  padding: "1px 6px",
};
const kindCountStyle: CSSProperties = { color: "var(--primary, #7df9ff)" };
const listHeadStyle: CSSProperties = { fontSize: 10, letterSpacing: "0.1em", color: "var(--muted-foreground)" };
const listStyle: CSSProperties = {
  display: "flex",
  flexDirection: "column",
  gap: 2,
  maxHeight: 220,
  overflowY: "auto",
  borderTop: "1px solid var(--border)",
  paddingTop: "var(--spacing-2)",
};
const memberBase: CSSProperties = {
  display: "flex",
  gap: "var(--spacing-2)",
  alignItems: "baseline",
  textAlign: "left",
  background: "transparent",
  border: "none",
  borderBottom: "1px solid transparent",
  color: "var(--foreground)",
  cursor: "pointer",
  padding: "2px 4px",
  borderRadius: "var(--radius-sm)",
  fontSize: 11,
};
const memberStyle: CSSProperties = { ...memberBase };
const memberActiveStyle: CSSProperties = {
  ...memberBase,
  background: "rgba(192,139,255,0.14)",
  borderLeft: "2px solid #c08bff",
};
const memberKindStyle: CSSProperties = {
  color: "var(--primary, #7df9ff)",
  fontSize: 9,
  textTransform: "uppercase",
  minWidth: 52,
  flex: "0 0 auto",
};
const memberNameStyle: CSSProperties = {
  overflow: "hidden",
  textOverflow: "ellipsis",
  whiteSpace: "nowrap",
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
  maxWidth: 190,
};
