//! Graph tab — community-overview (T8). Renders the server-pre-aggregated
//! community meta-graph (/api/graph/communities) in deck.gl: ScatterplotLayer
//! meta-nodes + ArcLayer inter-community meta-edges, in a non-geospatial
//! CARTESIAN OrthographicView. This is the LOD overview — never raw nodes.
//! (Drill-down is S2.)

import { useEffect, useMemo, useState } from "react";
import { DeckGL } from "@deck.gl/react";
import {
  COORDINATE_SYSTEM,
  OrthographicView,
  type PickingInfo,
} from "@deck.gl/core";
import { ArcLayer, ScatterplotLayer } from "@deck.gl/layers";

import {
  api,
  type CommunityMetaEdge,
  type CommunityMetaNode,
  type CommunitiesResponse,
} from "../../lib/api.ts";

const ACCENT: [number, number, number] = [125, 249, 255]; // #7df9ff (cyan)
const VIOLET: [number, number, number] = [192, 139, 255]; // #c08bff

export function GraphOverview() {
  const [data, setData] = useState<CommunitiesResponse | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    api
      .communities()
      .then((d) => !cancelled && setData(d))
      .catch((e) => !cancelled && setError(e instanceof Error ? e.message : String(e)));
    return () => {
      cancelled = true;
    };
  }, []);

  // community id -> [x, y]; centroid for the initial view target.
  const { nodePos, target } = useMemo(() => {
    const nodes = data?.communities ?? [];
    const m = new Map<string, [number, number]>();
    let sx = 0;
    let sy = 0;
    for (const n of nodes) {
      const p: [number, number] = [n.x ?? 0, n.y ?? 0];
      m.set(n.id, p);
      sx += p[0];
      sy += p[1];
    }
    const t: [number, number, number] = nodes.length
      ? [sx / nodes.length, sy / nodes.length, 0]
      : [0, 0, 0];
    return { nodePos: m, target: t };
  }, [data]);

  const layers = useMemo(() => {
    if (!data) return [];
    return [
      new ArcLayer<CommunityMetaEdge>({
        id: "community-edges",
        data: data.edges,
        coordinateSystem: COORDINATE_SYSTEM.CARTESIAN,
        getSourcePosition: (d) => nodePos.get(d.source) ?? [0, 0],
        getTargetPosition: (d) => nodePos.get(d.target) ?? [0, 0],
        getSourceColor: [...ACCENT, 36],
        getTargetColor: [...VIOLET, 36],
        getWidth: 1,
        widthMinPixels: 0.4,
      }),
      new ScatterplotLayer<CommunityMetaNode>({
        id: "community-nodes",
        data: data.communities,
        coordinateSystem: COORDINATE_SYSTEM.CARTESIAN,
        getPosition: (d) => [d.x ?? 0, d.y ?? 0],
        getRadius: (d) => Math.sqrt(d.memberCount) * 1.6,
        radiusMinPixels: 3,
        radiusMaxPixels: 40,
        getFillColor: [...ACCENT, 160],
        stroked: true,
        getLineColor: [...ACCENT, 255],
        getLineWidth: 1,
        pickable: true,
      }),
    ];
  }, [data, nodePos]);

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
        background: "var(--background)",
      }}
    >
      <DeckGL
        views={[new OrthographicView({ id: "ortho", controller: true })]}
        initialViewState={{ ortho: { target, zoom: 0, minZoom: -6, maxZoom: 14 } }}
        layers={layers}
        getTooltip={({ object }: PickingInfo<CommunityMetaNode>) =>
          object ? `${object.name}\n${object.memberCount} members` : null
        }
      />
      <Legend count={data.communities.length} edges={data.edges.length} />
    </div>
  );
}

function Legend({ count, edges }: { count: number; edges: number }) {
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
      {count} communities · {edges} edges
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
