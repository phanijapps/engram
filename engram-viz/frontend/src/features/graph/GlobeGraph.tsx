//! Graph tab — the animated 3D globe, in zbot's Apple-Vision aesthetic (ported
//! from observatory-v2). Community meta-nodes are emissive spheres on a Fibonacci
//! lattice, cream-tinted per community; inter-community meta-edges are one
//! additive-blend LineSegments geometry. Frosted-glass HUD cards (stats / pills /
//! footer), hover billboard labels, and a click pick-card. Data from
//! /api/graph/communities + /api/graph/stats.

import { Canvas, useFrame } from "@react-three/fiber";
import { Billboard, Html, OrbitControls } from "@react-three/drei";
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import * as THREE from "three";
import { Layers, Network, Activity, Globe as GlobeIcon, X } from "lucide-react";

import {
  api,
  type CommunitiesResponse,
  type CommunityMetaNode,
  type GraphStats,
} from "../../lib/api.ts";
import "./globe.css";

const RADIUS = 5;
const CREAM = "#f5ecd9";
// Cream-tinted categorical palette (zbot's entity-type palette, extended).
const PALETTE = [
  "#a4b6ff",
  "#f5d28d",
  "#8ee5b8",
  "#ff9d80",
  "#c9a8ff",
  "#9adff0",
  "#f0c46b",
  "#dccba5",
];
const colorFor = (label: number) => PALETTE[((label % PALETTE.length) + PALETTE.length) % PALETTE.length];

/** Fibonacci lattice on a sphere — even distribution. */
function fibSphere(n: number, r: number): THREE.Vector3[] {
  const out: THREE.Vector3[] = [];
  const golden = Math.PI * (3 - Math.sqrt(5));
  for (let i = 0; i < n; i++) {
    const y = 1 - (i / Math.max(1, n - 1)) * 2;
    const rxz = Math.sqrt(Math.max(0, 1 - y * y));
    const theta = golden * i;
    out.push(new THREE.Vector3(Math.cos(theta) * rxz * r, y * r, Math.sin(theta) * rxz * r));
  }
  return out;
}

function Globe({
  data,
  onPick,
}: {
  data: CommunitiesResponse;
  onPick: (c: CommunityMetaNode) => void;
}) {
  const group = useRef<THREE.Group>(null);
  const [hovered, setHovered] = useState<CommunityMetaNode | null>(null);

  const positions = useMemo(
    () => fibSphere(data.communities.length, RADIUS),
    [data.communities.length],
  );
  const posById = useMemo(() => {
    const m = new Map<string, THREE.Vector3>();
    data.communities.forEach((c, i) => m.set(c.id, positions[i]));
    return m;
  }, [data.communities, positions]);

  const edgeGeo = useMemo(() => {
    const pts: number[] = [];
    for (const e of data.edges) {
      const s = posById.get(e.source);
      const t = posById.get(e.target);
      if (s && t) pts.push(s.x, s.y, s.z, t.x, t.y, t.z);
    }
    const g = new THREE.BufferGeometry();
    g.setAttribute("position", new THREE.Float32BufferAttribute(pts, 3));
    return g;
  }, [data.edges, posById]);

  const maxMembers = useMemo(
    () => data.communities.reduce((m, c) => Math.max(m, c.memberCount), 1),
    [data.communities],
  );

  // Slow constant rotation (zbot's pace) — pauses while a node is hovered.
  useFrame((_, dt) => {
    if (group.current && !hovered) group.current.rotation.y += dt * 0.009;
  });

  return (
    <group ref={group}>
      <lineSegments geometry={edgeGeo}>
        <lineBasicMaterial
          color={CREAM}
          transparent
          opacity={0.16}
          depthWrite={false}
          blending={THREE.AdditiveBlending}
        />
      </lineSegments>
      {data.communities.map((c, i) => {
        const p = positions[i];
        if (!p) return null;
        const isHover = hovered?.id === c.id;
        const norm = Math.min(1, Math.sqrt(c.memberCount) / Math.sqrt(maxMembers));
        const size = 0.04 + norm * 0.08;
        const color = colorFor(Number(c.id.replace(/^c/, "")) || 0);
        return (
          <group key={c.id} position={[p.x, p.y, p.z]}>
            <mesh
              onPointerOver={(e) => {
                e.stopPropagation();
                setHovered(c);
                document.body.style.cursor = "pointer";
              }}
              onPointerOut={(e) => {
                e.stopPropagation();
                setHovered(null);
                document.body.style.cursor = "default";
              }}
              onClick={(e) => {
                e.stopPropagation();
                onPick(c);
              }}
            >
              <sphereGeometry args={[size, 16, 16]} />
              <meshStandardMaterial
                color={color}
                emissive={color}
                emissiveIntensity={isHover ? 1.2 : 0.5}
                roughness={0.45}
              />
            </mesh>
            {isHover && (
              <Billboard position={[0, size * 2.4, 0]}>
                <Html center distanceFactor={9} style={{ pointerEvents: "none" }}>
                  <div className="gg-node-label">
                    <span className="gg-node-label-name">{c.name}</span>
                    <span className="gg-node-label-meta">{c.memberCount} members</span>
                  </div>
                </Html>
              </Billboard>
            )}
          </group>
        );
      })}
    </group>
  );
}

export function GlobeGraph() {
  const [data, setData] = useState<CommunitiesResponse | null>(null);
  const [stats, setStats] = useState<GraphStats | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [picked, setPicked] = useState<CommunityMetaNode | null>(null);

  useEffect(() => {
    let cancelled = false;
    Promise.all([api.communities(), api.stats()])
      .then(([c, s]) => {
        if (cancelled) return;
        setData(c);
        setStats(s);
      })
      .catch((e) => !cancelled && setError(e instanceof Error ? e.message : String(e)));
    return () => {
      cancelled = true;
    };
  }, []);

  if (error) return <div className="graph-globe"><div className="gg-empty">Error: {error}</div></div>;
  if (!data) return <div className="graph-globe"><div className="gg-empty">Loading globe…</div></div>;
  if (!data.built || data.communities.length === 0)
    return <div className="graph-globe"><div className="gg-empty">No communities to render.</div></div>;

  const total = data.totalCommunities ?? data.communities.length;

  return (
    <div className="graph-globe">
      <div className="graph-globe__canvas">
        <Canvas camera={{ position: [0, 0, 16], fov: 50 }} dpr={[1, 2]}>
          <ambientLight intensity={0.55} />
          <pointLight position={[12, 12, 12]} intensity={0.8} />
          <pointLight position={[-12, -8, -6]} intensity={0.3} />
          <Globe data={data} onPick={setPicked} />
          <OrbitControls enablePan={false} minDistance={8} maxDistance={32} />
        </Canvas>
      </div>

      {/* top-left: title + stat grid */}
      <div className="gg-hud gg-hud--tl">
        <div className="gg-eyebrow">engram · graph</div>
        <div className="gg-stats">
          <Stat value={fmt(total)} label="Communities" />
          <Stat value={fmt(data.edges.length)} label="Edges" />
          <Stat value={fmt(stats?.entities)} label="Entities" />
          <Stat value={fmt(stats?.memories)} label="Facts" />
        </div>
      </div>

      {/* top-right: scope + density */}
      <div className="gg-hud gg-hud--tr">
        <div className="gg-eyebrow">scope</div>
        <div style={{ fontFamily: "var(--font-mono)", fontSize: 13, color: "var(--cream)", marginTop: 6 }}>
          agentzero
        </div>
        <div style={{ fontFamily: "var(--font-mono)", fontSize: 10, color: "var(--cream-dim)", marginTop: 8 }}>
          showing {data.communities.length} of {total}
        </div>
      </div>

      {/* bottom: pills */}
      <div className="gg-hud--bottom">
        <Pill icon={<Network size={14} />} label="Graph snapshot" sub={`${fmt(stats?.entities)} entities · ${fmt(stats?.relationships)} rels`} />
        <Pill icon={<Layers size={14} />} label="Communities" sub={`${fmt(total)} clusters · ${fmt(data.edges.length)} links`} />
        <Pill icon={<Activity size={14} />} label="Snapshot" sub="up to date" />
      </div>

      {/* footer: dense stat strip */}
      <div className="gg-footer">
        <FooterStat label="Entities" value={fmt(stats?.entities)} />
        <FooterStat label="Relationships" value={fmt(stats?.relationships)} />
        <FooterStat label="Communities" value={fmt(total)} />
        <FooterStat label="Facts" value={fmt(stats?.memories)} />
        <FooterStat label="Beliefs" value={fmt(stats?.beliefs)} />
        <FooterStat label="Hierarchy" value={fmt(stats?.hierarchyNodes)} />
      </div>

      {/* picked community card */}
      {picked && (
        <div className="gg-pick">
          <button className="gg-pick-close" onClick={() => setPicked(null)} aria-label="close">
            <X size={14} />
          </button>
          <div className="gg-pick-eyebrow">community · {picked.id}</div>
          <div className="gg-pick-name">{picked.name}</div>
          <div className="gg-pick-meta">
            <span className="gg-pick-chip">{fmt(picked.memberCount)} members</span>
            <span className="gg-pick-chip">drill in Observatory ↗</span>
          </div>
        </div>
      )}

      {/* legend hint, bottom-left above footer */}
      <div style={{ position: "absolute", left: 18, bottom: 50, display: "flex", alignItems: "center", gap: 6, color: "var(--cream-dim)", fontFamily: "var(--font-mono)", fontSize: 10, pointerEvents: "none" }}>
        <GlobeIcon size={12} /> drag to orbit · click a node
      </div>
    </div>
  );
}

function Stat({ value, label }: { value: string; label: string }) {
  return (
    <div className="gg-stat">
      <span className="gg-stat-value">{value}</span>
      <span className="gg-stat-label">{label}</span>
    </div>
  );
}
function Pill({ icon, label, sub }: { icon: ReactNode; label: string; sub: string }) {
  return (
    <div className="gg-pill">
      <span className="gg-pill-icon">{icon}</span>
      <span className="gg-pill-body">
        <span className="gg-pill-label">{label}</span>
        <span className="gg-pill-sub">{sub}</span>
      </span>
    </div>
  );
}
function FooterStat({ label, value }: { label: string; value: string }) {
  return (
    <div className="gg-footer-stat">
      <span className="gg-footer-stat-label">{label}</span>
      <span className="gg-footer-stat-value">{value}</span>
    </div>
  );
}

function fmt(n: number | undefined | null): string {
  if (n === undefined || n === null) return "—";
  if (n >= 1000) return `${(n / 1000).toFixed(n >= 10000 ? 0 : 1)}k`;
  return String(n);
}
