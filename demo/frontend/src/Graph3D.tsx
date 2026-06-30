import { useMemo, useState } from "react";
import { Canvas, type ThreeEvent } from "@react-three/fiber";
import { Line, OrbitControls, Html } from "@react-three/drei";

// Minimalist, enterprise-ready 3D knowledge graph.
//
// Consumes the existing /ingest/extract shape (entities + relationships) and
// renders it on a deterministic Fibonacci-sphere layout so the same input
// always yields the same graph. Nodes are navigable: click a node to open the
// detail panel (with source links); click an edge to inspect the relationship.
// Aesthetic is restraint-first: neutral dark, single accent, minimal chrome.

export type GraphSourceRef = {
  targetType?: string;
  targetId?: string;
  uri?: string;
  quote?: string;
  location?: { path?: string; line?: number | null; column?: number | null } | null;
};

export type GraphEntity = {
  id: string;
  name: string;
  kind: string;
  degree: number;
  aliases?: string[];
  sourceRefs?: GraphSourceRef[];
};

export type GraphRelationship = {
  id: string;
  subject: { id: string; name?: string };
  predicate: string;
  object: { id: string; name?: string };
  confidence?: number;
};

export type GraphData = {
  entities: GraphEntity[];
  relationships: GraphRelationship[];
};

const PALETTE: Record<string, string> = {
  function: "#6ea8ff",
  method: "#6ea8ff",
  class: "#b07cff",
  struct: "#b07cff",
  trait: "#b07cff",
  module: "#b07cff",
  concept: "#7bd88f",
  file: "#e0b34a",
  repository: "#e0b34a",
};
const DEFAULT_NODE = "#9aa6bd";
const ACCENT = "#6ea8ff";
const GOLDEN = Math.PI * (3 - Math.sqrt(5)); // golden angle
const SPHERE_RADIUS = 4;

function fibSphere(i: number, n: number): [number, number, number] {
  if (n <= 1) return [0, 0, 0];
  const y = 1 - (2 * i) / (n - 1);
  const r = Math.sqrt(Math.max(0, 1 - y * y)) * SPHERE_RADIUS;
  const theta = GOLDEN * i;
  return [Math.cos(theta) * r, y * SPHERE_RADIUS, Math.sin(theta) * r];
}

function nodeColor(kind: string): string {
  return PALETTE[kind] ?? DEFAULT_NODE;
}
function nodeSize(degree: number): number {
  return 0.13 + Math.min(0.2, degree * 0.025);
}

type Selection =
  | { type: "entity"; id: string }
  | { type: "relationship"; id: string }
  | null;

function sourceHref(ref: GraphSourceRef): string | null {
  if (ref.uri) return ref.uri;
  if (ref.location?.path) return ref.location.path;
  return null;
}

function GraphNode({
  entity,
  position,
  selected,
  dimmed,
  onSelect,
  onHover,
}: {
  entity: GraphEntity;
  position: [number, number, number];
  selected: boolean;
  dimmed: boolean;
  onSelect: () => void;
  onHover: (hovered: boolean) => void;
}) {
  const [hovered, setHovered] = useState(false);
  const color = nodeColor(entity.kind);
  const size = nodeSize(entity.degree);
  const active = selected || hovered;
  return (
    <group position={position}>
      <mesh
        onClick={(e: ThreeEvent<MouseEvent>) => {
          e.stopPropagation();
          onSelect();
        }}
        onPointerOver={(e: ThreeEvent<PointerEvent>) => {
          e.stopPropagation();
          setHovered(true);
          onHover(true);
        }}
        onPointerOut={() => {
          setHovered(false);
          onHover(false);
        }}
      >
        <sphereGeometry args={[size, 24, 24]} />
        <meshStandardMaterial
          color={color}
          emissive={active ? ACCENT : "#000"}
          emissiveIntensity={selected ? 0.7 : hovered ? 0.35 : 0}
          transparent={dimmed}
          opacity={dimmed ? 0.25 : 1}
        />
      </mesh>
      {active && (
        <Html center distanceFactor={10} position={[0, size + 0.25, 0]} style={{ pointerEvents: "none" }}>
          <div className="graph3d__label">{entity.name}</div>
        </Html>
      )}
    </group>
  );
}

function GraphEdge({
  relationship,
  from,
  to,
  highlighted,
  dimmed,
  onSelect,
  onHover,
}: {
  relationship: GraphRelationship;
  from: [number, number, number];
  to: [number, number, number];
  highlighted: boolean;
  dimmed: boolean;
  onSelect: () => void;
  onHover: (hovered: boolean) => void;
}) {
  const [hovered, setHovered] = useState(false);
  const active = highlighted || hovered;
  return (
    <Line
      points={[from, to]}
      color={active ? ACCENT : dimmed ? "#26304a" : "#3a4768"}
      lineWidth={active ? 2 : 1}
      onClick={(e: ThreeEvent<MouseEvent>) => {
        e.stopPropagation();
        onSelect();
      }}
      onPointerOver={(e: ThreeEvent<PointerEvent>) => {
        e.stopPropagation();
        setHovered(true);
        onHover(true);
      }}
      onPointerOut={() => {
        setHovered(false);
        onHover(false);
      }}
    />
  );
}

function DetailOverlay({
  data,
  selection,
  onClear,
}: {
  data: GraphData;
  selection: Selection;
  onClear: () => void;
}) {
  if (!selection) return null;
  if (selection.type === "entity") {
    const entity = data.entities.find((e) => e.id === selection.id);
    if (!entity) return null;
    return (
      <div className="graph3d__detail">
        <div className="graph3d__detail-head">
          <span className="graph3d__kind">{entity.kind}</span>
          <button className="graph3d__close" onClick={onClear} aria-label="close">
            ×
          </button>
        </div>
        <div className="graph3d__name">{entity.name}</div>
        <dl className="graph3d__fields">
          <div>
            <dt>id</dt>
            <dd>
              <code>{entity.id}</code>
            </dd>
          </div>
          <div>
            <dt>degree</dt>
            <dd>{entity.degree}</dd>
          </div>
          {entity.aliases && entity.aliases.length > 0 && (
            <div>
              <dt>aliases</dt>
              <dd>{entity.aliases.join(", ")}</dd>
            </div>
          )}
        </dl>
        {entity.sourceRefs && entity.sourceRefs.length > 0 && (
          <div className="graph3d__sources">
            <div className="graph3d__sources-head">sources</div>
            <ul>
              {entity.sourceRefs.map((ref, i) => {
                const href = sourceHref(ref);
                const label =
                  ref.uri ?? ref.location?.path ?? ref.targetId ?? `reference ${i + 1}`;
                return (
                  <li key={i}>
                    {href ? (
                      <a href={href} target="_blank" rel="noreferrer">
                        {label}
                      </a>
                    ) : (
                      <span>{label}</span>
                    )}
                  </li>
                );
              })}
            </ul>
          </div>
        )}
      </div>
    );
  }
  const rel = data.relationships.find((r) => r.id === selection.id);
  if (!rel) return null;
  return (
    <div className="graph3d__detail">
      <div className="graph3d__detail-head">
        <span className="graph3d__kind">relationship</span>
        <button className="graph3d__close" onClick={onClear} aria-label="close">
          ×
        </button>
      </div>
      <div className="graph3d__name">{rel.predicate}</div>
      <dl className="graph3d__fields">
        <div>
          <dt>from</dt>
          <dd>{rel.subject.name ?? rel.subject.id}</dd>
        </div>
        <div>
          <dt>to</dt>
          <dd>{rel.object.name ?? rel.object.id}</dd>
        </div>
        {typeof rel.confidence === "number" && (
          <div>
            <dt>confidence</dt>
            <dd>{rel.confidence.toFixed(2)}</dd>
          </div>
        )}
      </dl>
    </div>
  );
}

export function Graph3D({ data }: { data: GraphData | null }) {
  const [selection, setSelection] = useState<Selection>(null);
  const [hoveredEntity, setHoveredEntity] = useState<string | null>(null);

  const positions = useMemo(() => {
    const map = new Map<string, [number, number, number]>();
    const entities = data?.entities ?? [];
    entities.forEach((entity, i) => map.set(entity.id, fibSphere(i, entities.length)));
    return map;
  }, [data]);

  const entities = data?.entities ?? [];
  const relationships = data?.relationships ?? [];

  const selectedEntityId =
    selection?.type === "entity" ? selection.id : hoveredEntity;
  const focusedEdgeIds = new Set(
    selectedEntityId
      ? relationships
          .filter(
            (r) => r.subject.id === selectedEntityId || r.object.id === selectedEntityId
          )
          .map((r) => r.id)
      : []
  );

  const hasFocus = selectedEntityId !== null;

  return (
    <div className="graph3d">
      <Canvas
        camera={{ position: [0, 0, 11], fov: 50 }}
        onPointerMissed={() => setSelection(null)}
        dpr={[1, 2]}
        gl={{ antialias: true, alpha: true }}
      >
        <ambientLight intensity={0.65} />
        <directionalLight position={[5, 6, 7]} intensity={0.8} />
        <directionalLight position={[-6, -3, -4]} intensity={0.3} color="#9ab4ff" />
        {relationships.map((rel) => {
          const from = positions.get(rel.subject.id);
          const to = positions.get(rel.object.id);
          if (!from || !to) return null;
          const highlighted = focusedEdgeIds.has(rel.id);
          return (
            <GraphEdge
              key={rel.id}
              relationship={rel}
              from={from}
              to={to}
              highlighted={highlighted}
              dimmed={hasFocus && !highlighted}
              onSelect={() => setSelection({ type: "relationship", id: rel.id })}
              onHover={() => {}}
            />
          );
        })}
        {entities.map((entity) => {
          const position = positions.get(entity.id);
          if (!position) return null;
          const selected = selection?.type === "entity" && selection.id === entity.id;
          return (
            <GraphNode
              key={entity.id}
              entity={entity}
              position={position}
              selected={selected}
              dimmed={hasFocus && entity.id !== selectedEntityId}
              onSelect={() => setSelection({ type: "entity", id: entity.id })}
              onHover={(h) => setHoveredEntity(h ? entity.id : null)}
            />
          );
        })}
        <OrbitControls
          enableDamping
          dampingFactor={0.08}
          autoRotate
          autoRotateSpeed={0.4}
          minDistance={4}
          maxDistance={28}
        />
      </Canvas>
      <DetailOverlay
        data={{ entities, relationships }}
        selection={selection}
        onClear={() => setSelection(null)}
      />
      {entities.length === 0 && (
        <div className="graph3d__empty">Ingest a document to populate the graph.</div>
      )}
    </div>
  );
}
