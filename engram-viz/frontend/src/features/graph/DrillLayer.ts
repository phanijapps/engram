//! Drill layer — when a community meta-node is clicked, its member entities are
//! fetched (bounded) and rendered as a ScatterplotLayer "exploded" around the
//! community's overview coordinate (small concentric rings). Drill members are
//! violet to distinguish them from the cyan overview nodes. Member ids contain
//! slashes but never reach a path param here (the layer carries the object).

import { COORDINATE_SYSTEM, type Layer } from "@deck.gl/core";
import { ScatterplotLayer } from "@deck.gl/layers";

import type { GraphEntityView } from "../../lib/api.ts";

const VIOLET: [number, number, number] = [192, 139, 255]; // #c08bff — drill accent
const WHITE: [number, number, number] = [255, 255, 255];

interface MemberNode extends GraphEntityView {
  __i: number;
}

/** Deterministic concentric-ring positions around a center (mirrors the overview
 * layout at a small scale, so the drill reads as one cluster). */
export function memberPositions(center: [number, number], count: number): [number, number][] {
  const out: [number, number][] = [];
  const perRing = Math.max(6, Math.ceil(Math.sqrt(count) * 1.6));
  const ringCount = Math.max(1, Math.ceil(count / perRing));
  for (let i = 0; i < count; i++) {
    const ring = Math.min(ringCount - 1, Math.floor(i / perRing));
    const ringStart = ring * perRing;
    const inRing = Math.min(count, ringStart + perRing) - ringStart;
    const posInRing = i - ringStart;
    const a = (posInRing / inRing) * Math.PI * 2 + ring * 2.39996323;
    const r = 16 + ring * 12;
    out.push([center[0] + Math.cos(a) * r, center[1] + Math.sin(a) * r]);
  }
  return out;
}

/** Build the drill ScatterplotLayer for a community's members, clustered at
 * `center`. Picking is handled by the DeckGL `onClick` (single discriminator),
 * not a per-layer handler. */
export function makeDrillLayer(
  center: [number, number],
  members: GraphEntityView[],
  selectedId: string | null,
): Layer {
  const pos = memberPositions(center, members.length);
  return new ScatterplotLayer<MemberNode>({
    id: "drill-members",
    data: members.map((m, i) => ({ ...m, __i: i })),
    coordinateSystem: COORDINATE_SYSTEM.CARTESIAN,
    getPosition: (d) => pos[d.__i] ?? center,
    getRadius: 5,
    radiusMinPixels: 4,
    radiusMaxPixels: 8,
    getFillColor: (d) => (d.id === selectedId ? VIOLET : [...VIOLET, 190]),
    stroked: true,
    getLineColor: [...WHITE, 210],
    getLineWidth: 1,
    pickable: true,
  });
}
