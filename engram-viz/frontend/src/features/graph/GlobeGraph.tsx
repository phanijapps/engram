//! Graph tab — the animated 3D globe (zbot's observatory-v2 aesthetic). Renders
//! the community meta-graph from /api/graph/communities on a Fibonacci sphere:
//! each community is a sphere sized by membership (cyan, lit); inter-community
//! meta-edges are one LineSegments geometry (single draw call). The globe
//! auto-rotates slowly (pauses on hover) and is orbit/zoom-controlled. This is a
//! VISUAL overview; the Observatory is the interactive 2D explorer.

import { Canvas, useFrame } from "@react-three/fiber";
import { Html, OrbitControls } from "@react-three/drei";
import { useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import * as THREE from "three";

import { api, type CommunitiesResponse, type CommunityMetaNode } from "../../lib/api.ts";

const ACCENT = "#7df9ff";
const RADIUS = 5; // globe radius

/** Fibonacci lattice on a sphere — deterministic, even distribution, same-type
 * neighborhoods (mirrors zbot's EntityGraph positioning). */
function fibSphere(n: number, r: number): THREE.Vector3[] {
  const out: THREE.Vector3[] = [];
  const golden = Math.PI * (3 - Math.sqrt(5));
  for (let i = 0; i < n; i++) {
    const y = 1 - (i / Math.max(1, n - 1)) * 2;
    const rad = Math.sqrt(Math.max(0, 1 - y * y));
    const theta = golden * i;
    out.push(new THREE.Vector3(Math.cos(theta) * rad * r, y * r, Math.sin(theta) * rad * r));
  }
  return out;
}

function Globe({ data }: { data: CommunitiesResponse }) {
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

  // All edges in one BufferGeometry → one draw call.
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

  useFrame((_, dt) => {
    if (group.current && !hovered) group.current.rotation.y += dt * 0.06;
  });

  return (
    <group ref={group}>
      <lineSegments geometry={edgeGeo}>
        <lineBasicMaterial color={ACCENT} transparent opacity={0.16} />
      </lineSegments>
      {data.communities.map((c, i) => {
        const isHover = hovered?.id === c.id;
        return (
          <mesh
            key={c.id}
            position={positions[i]}
            onPointerOver={(e) => {
              e.stopPropagation();
              setHovered(c);
              document.body.style.cursor = "pointer";
            }}
            onPointerOut={() => {
              setHovered(null);
              document.body.style.cursor = "auto";
            }}
          >
            <sphereGeometry
              args={[Math.max(0.05, Math.sqrt(c.memberCount) * 0.011), 14, 14]}
            />
            <meshStandardMaterial
              color={isHover ? "#ffffff" : ACCENT}
              emissive={isHover ? ACCENT : "#000000"}
              emissiveIntensity={isHover ? 0.6 : 0}
              roughness={0.4}
              metalness={0.3}
            />
            {isHover && (
              <Html distanceFactor={12} position={[0, 0.3, 0]} style={tooltipStyle}>
                <div>
                  <b>{c.name}</b>
                  <br />
                  {c.memberCount} members
                </div>
              </Html>
            )}
          </mesh>
        );
      })}
    </group>
  );
}

export function GlobeGraph() {
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

  if (error) return <Status text={`Error: ${error}`} />;
  if (!data) return <Status text="Loading globe…" />;
  if (!data.built || data.communities.length === 0)
    return <Status text="No communities to render." />;

  return (
    <div
      style={{
        position: "relative",
        width: "100%",
        height: "100%",
        background: "var(--background)",
      }}
    >
      <Canvas camera={{ position: [0, 0, 16], fov: 50 }} dpr={[1, 2]}>
        <ambientLight intensity={0.6} />
        <pointLight position={[12, 12, 12]} intensity={0.8} />
        <pointLight position={[-12, -8, -6]} intensity={0.3} />
        <Globe data={data} />
        <OrbitControls enablePan={false} minDistance={8} maxDistance={32} />
      </Canvas>
      <Legend
        count={data.communities.length}
        total={data.totalCommunities}
        edges={data.edges.length}
      />
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
  const label = total && total > count ? `${count} of ${total} communities` : `${count} communities`;
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
      {label} · {edges} edges · drag to orbit
    </div>
  );
}

const tooltipStyle: CSSProperties = {
  fontFamily: "var(--font-mono)",
  fontSize: "11px",
  color: "var(--foreground)",
  background: "var(--sidebar)",
  border: "1px solid var(--border)",
  borderRadius: "var(--radius-sm)",
  padding: "2px 8px",
  whiteSpace: "nowrap",
  pointerEvents: "none",
  transform: "translate(-50%, -100%)",
};

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
