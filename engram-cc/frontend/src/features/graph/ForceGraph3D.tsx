//! Graph3DView — the ACTUAL graph in immersive 3D using @react-three/fiber
//! (three.js, React-declarative — the same stack the old GlobeGraph used,
//! proven to render in this app). Nodes are colored spheres sized by degree,
//! edges are thin lines. OrbitControls for drag/zoom. Click a node to select.
//!
//! The force layout runs via d3-force (2D positions computed once, z jittered
//! for depth). This is simpler and more reliable than the 3d-force-graph
//! imperative library.

import { useEffect, useMemo, useRef, useState } from "react";
import { Canvas } from "@react-three/fiber";
import { OrbitControls, Text } from "@react-three/drei";
import * as THREE from "three";
import {
  forceCollide,
  forceLink,
  forceManyBody,
  forceSimulation,
  forceX,
  forceY,
  type Simulation,
  type SimulationLinkDatum,
  type SimulationNodeDatum,
} from "d3-force";
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

type FNode = SimulationNodeDatum & {
  id: string;
  name: string;
  kind: string;
  degree: number;
  r: number;
  z: number;
};
type FLink = SimulationLinkDatum<FNode> & { predicate: string };

export interface Graph3DProps {
  nodes: SymbolGraphNode[];
  edges: SymbolGraphEdge[];
  selectedEntityId: string | null;
  onSelect: (id: string) => void;
}

/** Runs the 2D force simulation + adds z-axis jitter, returns positioned nodes. */
function useForceLayout(nodes: SymbolGraphNode[], edges: SymbolGraphEdge[]) {
  const [layout, setLayout] = useState<{ nodes: FNode[]; links: FLink[] } | null>(null);

  useEffect(() => {
    if (nodes.length === 0) return;
    const maxDegree = Math.max(...nodes.map((n) => n.degree), 1);
    const fnodes: FNode[] = nodes.map((n, i) => ({
      id: n.id,
      name: n.name,
      kind: n.kind,
      degree: n.degree,
      r: 2 + Math.sqrt(n.degree / maxDegree) * 8,
      z: (Math.random() - 0.5) * 120,
      x: Math.cos((i / nodes.length) * Math.PI * 2) * 80,
      y: Math.sin((i / nodes.length) * Math.PI * 2) * 80,
    }));
    const byId = new Map(fnodes.map((n) => [n.id, n]));
    const flinks: FLink[] = edges
      .filter((e) => byId.has(e.source) && byId.has(e.target))
      .map((e) => ({ source: e.source, target: e.target, predicate: e.predicate }));

    const sim = forceSimulation<FNode, FLink>(fnodes)
      .force("link", forceLink<FNode, FLink>(flinks).id((d) => d.id).distance(30).strength(0.1))
      .force("charge", forceManyBody<FNode>().strength(-100).distanceMax(400))
      .force("collide", forceCollide<FNode>((d) => d.r + 2).iterations(2))
      .force("cx", forceX(0).strength(0.05))
      .force("cy", forceY(0).strength(0.05))
      .stop();

    // Run synchronously to a settled state
    for (let i = 0; i < 200; i++) sim.tick();

    setLayout({ nodes: [...fnodes], links: flinks });
    return () => { sim.stop(); };
  }, [nodes, edges]);

  return layout;
}

/** A single node sphere. */
function NodeSphere({
  node,
  selected,
  onSelect,
}: {
  node: FNode;
  selected: boolean;
  onSelect: (id: string) => void;
}) {
  const meshRef = useRef<THREE.Mesh>(null);
  const [hovered, setHovered] = useState(false);
  const color = kindColor(node.kind);

  return (
    <group position={[node.x ?? 0, node.y ?? 0, node.z]}>
      <mesh
        ref={meshRef}
        onClick={() => onSelect(node.id)}
        onPointerOver={() => setHovered(true)}
        onPointerOut={() => setHovered(false)}
        scale={hovered || selected ? 1.4 : 1}
      >
        <sphereGeometry args={[node.r, 16, 16]} />
        <meshStandardMaterial
          color={selected ? "#ffffff" : color}
          emissive={color}
          emissiveIntensity={selected ? 0.8 : hovered ? 0.5 : 0.2}
          metalness={0.3}
          roughness={0.5}
        />
      </mesh>
      {(hovered || selected || node.degree > 8) && (
        <Text
          position={[0, node.r + 4, 0]}
          fontSize={Math.max(4, node.r * 0.8)}
          color={selected ? "#ffffff" : "#cccccc"}
          anchorX="center"
          anchorY="bottom"
          outlineWidth={0.5}
          outlineColor="#000000"
        >
          {node.name}
        </Text>
      )}
    </group>
  );
}

/** A single edge line. */
function EdgeLine({ link }: { link: FLink & { _source?: FNode; _target?: FNode } }) {
  const s = (link.source as unknown as FNode) ?? link._source;
  const t = (link.target as unknown as FNode) ?? link._target;
  if (!s || !t) return null;

  const geometry = useMemo(() => {
    const g = new THREE.BufferGeometry().setFromPoints([
      new THREE.Vector3(s.x ?? 0, s.y ?? 0, s.z),
      new THREE.Vector3(t.x ?? 0, t.y ?? 0, t.z),
    ]);
    return g;
  }, [s.x, s.y, s.z, t.x, t.y, t.z]);

  const isHighlight = link.predicate === "routes_to";

  return (
    <lineSegments geometry={geometry}>
      <lineBasicMaterial
        color={isHighlight ? "#c9a8ff" : "#7df9ff"}
        transparent
        opacity={isHighlight ? 0.4 : 0.12}
      />
    </lineSegments>
  );
}

export function Graph3DView({ nodes, edges, selectedEntityId, onSelect }: Graph3DProps) {
  const layout = useForceLayout(nodes, edges);

  if (!layout) {
    return (
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          height: "100%",
          fontFamily: "var(--font-mono)",
          fontSize: 13,
          color: "var(--muted-foreground)",
        }}
      >
        Computing 3D layout…
      </div>
    );
  }

  const bounds = 150 + Math.sqrt(layout.nodes.length) * 20;

  return (
    <div style={{ width: "100%", height: "100%", background: "#07080d" }}>
      <Canvas
        camera={{ position: [0, 0, bounds * 2.2], fov: 60 }}
        style={{ background: "#07080d" }}
      >
        <ambientLight intensity={0.4} />
        <directionalLight position={[bounds, bounds, bounds]} intensity={1.2} />
        <directionalLight position={[-bounds, -bounds, -bounds]} intensity={0.3} color="#7df9ff" />

        {layout.links.map((link, i) => (
          <EdgeLine key={i} link={link} />
        ))}
        {layout.nodes.map((node) => (
          <NodeSphere
            key={node.id}
            node={node}
            selected={node.id === selectedEntityId}
            onSelect={onSelect}
          />
        ))}

        <OrbitControls
          enablePan={true}
          enableZoom={true}
          enableRotate={true}
          dampingFactor={0.1}
          rotateSpeed={0.5}
          zoomSpeed={1.2}
          minDistance={20}
          maxDistance={bounds * 6}
        />
      </Canvas>
    </div>
  );
}
