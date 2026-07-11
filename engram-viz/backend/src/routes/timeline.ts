//! GET /api/timeline — repo-wide temporal view of symbol introductions.
//!
//! The N-API surface does not expose the temporal_* analytics directly, so this
//! route derives a timeline from entity `validFrom` stamps plus the Louvain
//! community overview. Data may be sparse when a repo was indexed in a single
//! pass (one introduction spike) — that is expected and surfaced honestly.

import { Hono } from "hono";
import { engine } from "../lib/engine.ts";

export const timelineRoute = new Hono();

interface TimelineBucket {
  date: string;
  count: number;
}

function dayOf(iso: string | undefined): string | null {
  if (!iso || iso.length < 10) return null;
  return iso.slice(0, 10);
}

timelineRoute.get("/", (c) => {
  const entities = engine.entities();
  const communities = engine.communities();

  // Group code entities (those in the call graph) by introduction day.
  const byDay = new Map<string, number>();
  const recent: {
    id: string;
    name: string;
    kind: string;
    validFrom?: string;
  }[] = [];

  const callGraphNodeIds = new Set<string>();
  for (const r of engine.relationships()) {
    if (r.predicate !== "calls") continue;
    if (r.subject.id) callGraphNodeIds.add(r.subject.id);
    if (r.object.id) callGraphNodeIds.add(r.object.id);
  }

  for (const e of entities) {
    const day = dayOf(e.validFrom ?? e.createdAt);
    if (day) byDay.set(day, (byDay.get(day) ?? 0) + 1);
    if (callGraphNodeIds.has(e.id) && (e.validFrom ?? e.createdAt)) {
      recent.push({
        id: e.id,
        name: e.name,
        kind: e.kind,
        validFrom: e.validFrom ?? e.createdAt,
      });
    }
  }

  const timeline: TimelineBucket[] = [...byDay.entries()]
    .map(([date, count]) => ({ date, count }))
    .sort((a, b) => a.date.localeCompare(b.date));

  recent.sort((a, b) =>
    (b.validFrom ?? "").localeCompare(a.validFrom ?? ""),
  );

  // Community overview.
  const communitySizes = new Map<number, number>();
  for (const label of Object.values(communities)) {
    communitySizes.set(label, (communitySizes.get(label) ?? 0) + 1);
  }
  const sizes = [...communitySizes.values()].sort((a, b) => b - a);

  return c.json({
    timeline,
    recentSymbols: recent.slice(0, 30),
    overview: {
      communityCount: communitySizes.size,
      largestCommunitySize: sizes[0] ?? 0,
      classifiedSymbols: Object.keys(communities).length,
    },
  });
});
