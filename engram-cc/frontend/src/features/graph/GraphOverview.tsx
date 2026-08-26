//! Observatory graph — community-overview (T8/T0) + drill (S2 T3). Renders the
//! server-pre-aggregated community meta-graph (/api/graph/communities) with a
//! d3-force simulation on canvas (ForceGraph): communities as force-directed
//! nodes seeded from the server's layout coords, meta-edges as links weighted
//! by strength. Clicking a community node drills into its member entities,
//! exploded as a violet cluster anchored to it; selecting a member opens the
//! entity-detail panel.

import { useEffect, useState } from "react";

import {
  api,
  type CommunitiesResponse,
} from "../../lib/api.ts";
import { useGraphStore } from "../../store/graph.ts";
import { ForceGraph } from "./ForceGraph.tsx";
import { EntityDetailPanel } from "./EntityDetail.tsx";

export function GraphOverview({
  limit,
  refreshSignal = 0,
  highlight = "",
}: {
  limit?: number;
  refreshSignal?: number;
  highlight?: string;
} = {}) {
  const [data, setData] = useState<CommunitiesResponse | null>(null);
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

  if (error) return <Status text={`Error: ${error}`} />;
  if (!data) return <Status text="Loading community overview…" />;
  if (!data.built || data.communities.length === 0)
    return <Status text="No communities — too few relationships to cluster." />;

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
      <Legend
        count={data.communities.length}
        total={data.totalCommunities}
        edges={data.edges.length}
      />
      <EntityDetailPanel />
    </div>
  );
}

function Legend({
  count,
  total,
  edges,
}: {
  count: number;
  total?: number;
  edges: number;
}) {
  const label =
    total && total > count ? `${count} of ${total} communities` : `${count} communities`;
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
      {label} · {edges} edges
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
