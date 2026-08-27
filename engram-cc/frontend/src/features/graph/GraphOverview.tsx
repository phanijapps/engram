//! Graph viewport — two views over the same scope:
//!  - "graph": the ACTUAL graph — real symbol nodes + resolved call edges
//!    (/api/graph/subgraph, degree-ranked + bounded), rendered by SymbolGraph.
//!  - "communities": the community meta-graph (/api/graph/communities) with
//!    d3-force communities + drill (the original overview).
//! Clicking a symbol selects it into the entity-detail panel.

import { useEffect, useState, type CSSProperties } from "react";

import {
  api,
  type CommunitiesResponse,
  type SubgraphResponse,
} from "../../lib/api.ts";
import { useGraphStore } from "../../store/graph.ts";
import { ForceGraph } from "./ForceGraph.tsx";
import { SymbolGraph } from "./SymbolGraph.tsx";
import { EntityDetailPanel } from "./EntityDetail.tsx";

export type GraphView = "graph" | "communities";

export function GraphOverview({
  limit,
  refreshSignal = 0,
  highlight = "",
  defaultView = "communities",
}: {
  limit?: number;
  refreshSignal?: number;
  highlight?: string;
  defaultView?: GraphView;
} = {}) {
  const [view, setView] = useState<GraphView>(defaultView);
  const [data, setData] = useState<CommunitiesResponse | null>(null);
  const [subgraph, setSubgraph] = useState<SubgraphResponse | null>(null);
  const [error, setError] = useState<string | null>(null);

  // drill store
  const drillCommunity = useGraphStore((s) => s.drillCommunity);
  const selectEntity = useGraphStore((s) => s.selectEntity);
  const drill = useGraphStore((s) => s.community);
  const members = useGraphStore((s) => s.members);
  const memberEdges = useGraphStore((s) => s.memberEdges);
  const selectedEntityId = useGraphStore((s) => s.selectedEntityId);

  useEffect(() => {
    let cancelled = false;
    setData(null);
    setError(null);
    api
      .communities(limit)
      .then((d) => !cancelled && setData(d))
      .catch((e) => !cancelled && setError(e instanceof Error ? e.message : String(e)));
    return () => {
      cancelled = true;
    };
  }, [limit, refreshSignal]);

  // The actual graph: fetched lazily on first switch (keeps the default
  // communities load unchanged) and on refresh.
  useEffect(() => {
    if (view !== "graph") return;
    let cancelled = false;
    setSubgraph(null);
    setError(null);
    api
      .subgraph(limit)
      .then((d) => !cancelled && setSubgraph(d))
      .catch((e) => !cancelled && setError(e instanceof Error ? e.message : String(e)));
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [view, refreshSignal]);

  if (error) return <Status text={`Error: ${error}`} />;

  return (
    <div
      style={{
        position: "relative",
        width: "100%",
        height: "100%",
        /* dark viewport — the force-graph scene colors are tuned for a dark canvas */
        background: "#07080d",
      }}
    >
      <ViewToggle view={view} onChange={setView} />
      {view === "graph" ? (
        subgraph ? (
          subgraph.nodes.length === 0 ? (
            <Status text="No resolved call edges — scan a repository first." />
          ) : (
            <SymbolGraph
              nodes={subgraph.nodes}
              edges={subgraph.edges}
              highlight={highlight}
              selectedEntityId={selectedEntityId}
              onSelect={(id) => void selectEntity(id)}
            />
          )
        ) : (
          <Status text="Loading graph…" />
        )
      ) : data ? (
        !data.built || data.communities.length === 0 ? (
          <Status text="No communities — too few relationships to cluster." />
        ) : (
          <ForceGraph
            communities={data.communities}
            edges={data.edges}
            highlight={highlight}
            drill={drill}
            members={members}
            memberEdges={memberEdges}
            selectedEntityId={selectedEntityId}
            onCommunityClick={(node) => void drillCommunity(node)}
            onMemberClick={(id) => void selectEntity(id)}
          />
        )
      ) : (
        <Status text="Loading community overview…" />
      )}
      {view === "graph" && subgraph && subgraph.nodes.length > 0 && (
        <Legend
          label={`${subgraph.nodes.length} symbols · ${subgraph.edges.length} edges`}
          sub={subgraph.totalNodes > subgraph.nodes.length
            ? `top ${subgraph.nodes.length} of ${subgraph.totalNodes} by degree · resolved ${subgraph.predicates.join("/")} only`
            : `resolved ${subgraph.predicates.join("/")} only`}
        />
      )}
      {view === "communities" && data && data.built && data.communities.length > 0 && (
        <Legend
          label={
            data.totalCommunities && data.totalCommunities > data.communities.length
              ? `${data.communities.length} of ${data.totalCommunities} communities`
              : `${data.communities.length} communities`
          }
          sub={`${data.edges.length} meta-edges`}
        />
      )}
      <EntityDetailPanel />
    </div>
  );
}

function ViewToggle({ view, onChange }: { view: GraphView; onChange: (v: GraphView) => void }) {
  const items: { key: GraphView; label: string }[] = [
    { key: "graph", label: "GRAPH" },
    { key: "communities", label: "COMMUNITIES" },
  ];
  return (
    <div style={toggleWrap}>
      {items.map((it) => (
        <button
          key={it.key}
          type="button"
          style={view === it.key ? toggleBtnActive : toggleBtn}
          onClick={() => onChange(it.key)}
          data-viewtoggle={it.key}
        >
          {it.label}
        </button>
      ))}
    </div>
  );
}

const toggleWrap: CSSProperties = {
  position: "absolute",
  top: "var(--spacing-3)",
  right: "var(--spacing-3)",
  display: "flex",
  gap: 4,
  zIndex: 20,
};
const toggleBtn: CSSProperties = {
  fontFamily: "var(--font-mono)",
  fontSize: 10,
  letterSpacing: "0.08em",
  color: "var(--muted-foreground)",
  background: "var(--sidebar)",
  border: "1px solid var(--border)",
  borderRadius: "var(--radius-sm)",
  padding: "4px 10px",
  cursor: "pointer",
};
const toggleBtnActive: CSSProperties = {
  ...toggleBtn,
  color: "var(--primary)",
  borderColor: "var(--primary)",
  fontWeight: 600,
};

function Legend({ label, sub }: { label: string; sub: string }) {
  return (
    <div
      style={{
        position: "absolute",
        left: "var(--spacing-3)",
        bottom: "var(--spacing-3)",
        fontFamily: "var(--font-mono)",
        fontSize: "11px",
        letterSpacing: "0.04em",
        color: "var(--muted-foreground)",
        background: "var(--sidebar)",
        border: "1px solid var(--border)",
        borderRadius: "var(--radius-md)",
        padding: "var(--spacing-2) var(--spacing-3)",
        pointerEvents: "none",
      }}
    >
      {label} <span style={{ opacity: 0.7 }}>· {sub}</span>
    </div>
  );
}

function Status({ text }: { text: string }) {
  return (
    <div className="page">
      <div className="page-container">
        <p style={{ fontFamily: "var(--font-mono)", color: "var(--muted-foreground)" }}>
          {text}
        </p>
      </div>
    </div>
  );
}
