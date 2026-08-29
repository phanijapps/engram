//! Graph3DView — the ACTUAL graph in immersive 3D using @react-three/fiber.
//! Simple, robust: compute a deterministic spherical layout (no d3-force —
//! that was producing NaN positions in some cases), render emissive spheres
//! + glowing lines, orbit controls.

import { useMemo, useRef, useState } from "react";
import { Canvas, useFrame } from "@react-three/fiber";
import { OrbitControls, Text } from "@react-three/drei";
import * as THREE from "three";
import type { SymbolGraphEdge, SymbolGraphNode } from "../../lib/api.ts";

const KIND_COLORS: Record<string, string> = {
  function: "#f5d28d",
  method: "#f5d28d",
  class: "#8ee5b8",
  struct: "#8ee5b8",
  interface: "#8ee5b8",
  trait: "#8ee5b8",
  enum: "#9adff0",
  type_alias: "#9adff0",
  module: "#a4b6ff",
  file: "#a4b6ff",
  endpoint: "#c9a8ff",
  api: "#c9a8ff",
  repository: "#dccba5",
};
const kindColor = (kind: string): string => KIND_COLORS[kind] ?? "#a4b6ff";

interface PositionedNode {
  id: string;
  name: string;
  kind: string;
  degree: number;
  radius: number;
  pos: [number, number, number];
}
interface PositionedLink {
  sourceId: string;
  targetId: string;
  from: [number, number, number];
  to: [number, number, number];
  predicate: string;
}

/** Fibonacci sphere layout — deterministic, evenly distributed, always valid. */
function sphericalLayout(nodes: SymbolGraphNode[], edges: SymbolGraphEdge[]) {
  const N = nodes.length;
  const R = 40 + Math.sqrt(N) * 12;
  const golden = Math.PI * (3 - Math.sqrt(5));
  const maxDegree = Math.max(...nodes.map((n) => n.degree), 1);

  const positioned: PositionedNode[] = nodes.map((n, i) => {
    const y = 1 - (i / Math.max(N - 1, 1)) * 2;
    const radius = Math.sqrt(1 - y * y);
    const theta = golden * i;
    const x = Math.cos(theta) * radius;
    const z = Math.sin(theta) * radius;
    // Scale by degree — high-degree nodes closer to center
    const pull = 1 - Math.min(n.degree / maxDegree, 0.7);
    const r = R * (0.3 + pull * 0.7);
    return {
      id: n.id,
      name: n.name,
      kind: n.kind,
      degree: n.degree,
      radius: 2 + Math.sqrt(n.degree / maxDegree) * 10,
      pos: [x * r, y * r, z * r] as [number, number, number],
    };
  });

  const byId = new Map(positioned.map((p) => [p.id, p]));
  const links: PositionedLink[] = [];
  for (const e of edges) {
    const s = byId.get(e.source);
    const t = byId.get(e.target);
    if (s && t) {
      links.push({ sourceId: e.source, targetId: e.target, from: s.pos, to: t.pos, predicate: e.predicate });
    }
  }
  return { nodes: positioned, links, radius: R };
}

/** A glowing sphere node. */
function Node({ node, selected, onSelect }: { node: PositionedNode; selected: boolean; onSelect: (id: string) => void }) {
  const [hovered, setHovered] = useState(false);
  const matRef = useRef<THREE.MeshStandardMaterial>(null);

  useFrame(() => {
    if (matRef.current) {
      const target = selected ? 1.0 : hovered ? 0.7 : 0.35;
      matRef.current.emissiveIntensity += (target - matRef.current.emissiveIntensity) * 0.1;
    }
  });

  const color = kindColor(node.kind);
  return (
    <group position={node.pos}>
      <mesh
        scale={hovered || selected ? 1.5 : 1}
        onClick={(e) => { e.stopPropagation(); onSelect(node.id); }}
        onPointerOver={(e) => { e.stopPropagation(); setHovered(true); }}
        onPointerOut={() => setHovered(false)}
      >
        <sphereGeometry args={[node.radius, 20, 20]} />
        <meshStandardMaterial
          ref={matRef}
          color={selected ? "#ffffff" : color}
          emissive={color}
          emissiveIntensity={0.35}
          metalness={0.4}
          roughness={0.3}
        />
      </mesh>
      {(hovered || selected || node.degree > 5) && (
        <Text
          position={[0, node.radius + 4, 0]}
          fontSize={Math.max(5, node.radius * 0.7)}
          color={selected ? "#ffffff" : "#aaa"}
          anchorX="center"
          anchorY="bottom"
          outlineWidth={1}
          outlineColor="#000"
          maxWidth={200}
        >
          {node.name}
        </Text>
      )}
      <pointLight color={color} intensity={selected ? 2 : hovered ? 1 : 0} distance={30} />
    </group>
  );
}

/** A glowing edge line. */
function Edge({ link }: { link: PositionedLink }) {
  const geometry = useMemo(() => {
    const g = new THREE.BufferGeometry().setFromPoints([
      new THREE.Vector3(...link.from),
      new THREE.Vector3(...link.to),
    ]);
    return g;
  }, [link.from, link.to]);

  const isRoute = link.predicate === "routes_to";
  return (
    <lineSegments geometry={geometry}>
      <lineBasicMaterial
        color={isRoute ? "#c9a8ff" : "#7df9ff"}
        transparent
        opacity={isRoute ? 0.5 : 0.15}
      />
    </lineSegments>
  );
}

export interface Graph3DProps {
  nodes: SymbolGraphNode[];
  edges: SymbolGraphEdge[];
  selectedEntityId: string | null;
  onSelect: (id: string) => void;
}

export function Graph3DView({ nodes, edges, selectedEntityId, onSelect }: Graph3DProps) {
  const { nodes: positioned, links, radius } = useMemo(
    () => sphericalLayout(nodes, edges),
    [nodes, edges],
  );

  console.log(`[Graph3DView] ${positioned.length} nodes, ${links.length} links, radius=${radius.toFixed(0)}, camera at z=${(radius * 3).toFixed(0)}`);
  if (positioned.length > 0) {
    console.log(`[Graph3DView] first node:`, positioned[0].name, `at`, positioned[0].pos.map((v) => v.toFixed(1)));
  }

  if (positioned.length === 0) {
    return (
      <div style={{ display: "flex", alignItems: "center", justifyContent: "center", height: "100%", fontFamily: "monospace", fontSize: 13, color: "#666" }}>
        No nodes to display — scan a repository first.
      </div>
    );
  }

  return (
    <div style={{ width: "100%", height: "100%", background: "#07080d", position: "relative" }}>
      <Canvas
        camera={{ position: [0, radius * 0.5, radius * 3], fov: 55, near: 0.1, far: radius * 20 }}
        style={{ background: "#07080d" }}
        gl={{ antialias: true, alpha: false }}
      >
        <ambientLight intensity={0.5} />
        <directionalLight position={[radius * 2, radius * 2, radius * 2]} intensity={1.5} />
        <directionalLight position={[-radius, -radius, -radius]} intensity={0.5} color="#7df9ff" />
        <pointLight position={[0, 0, 0]} intensity={0.5} color="#7df9ff" distance={radius * 4} />

        {links.map((link, i) => (
          <Edge key={i} link={link} />
        ))}
        {positioned.map((node) => (
          <Node
            key={node.id}
            node={node}
            selected={node.id === selectedEntityId}
            onSelect={onSelect}
          />
        ))}

        {/* Origin marker — a small white sphere so there's always something visible */}
        <mesh position={[0, 0, 0]}>
          <sphereGeometry args={[2, 8, 8]} />
          <meshBasicMaterial color="#ffffff" />
        </mesh>

        <OrbitControls
          dampingFactor={0.1}
          rotateSpeed={0.5}
          zoomSpeed={1.5}
          minDistance={10}
          maxDistance={radius * 10}
        />
      </Canvas>

      {/* Legend */}
      <div style={{
        position: "absolute", left: 12, bottom: 12,
        fontFamily: "var(--font-mono)", fontSize: 10, color: "var(--muted-foreground)",
        background: "var(--sidebar)", border: "1px solid var(--border)",
        borderRadius: "var(--radius-md)", padding: "var(--spacing-2) var(--spacing-3)",
        pointerEvents: "none",
      }}>
        {positioned.length} symbols · {links.length} edges · drag to orbit · scroll to zoom
      </div>
    </div>
  );
}
