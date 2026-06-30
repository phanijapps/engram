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

export type GraphProvenance = {
  source?: string;
  method?: string;
  confidence?: number;
  observedAt?: string;
};

export type GraphEntity = {
  id: string;
  name: string;
  kind: string;
  degree: number;
  confidence?: number;
  provenance?: GraphProvenance;
  aliases?: string[];
  sourceRefs?: GraphSourceRef[];
};

export type GraphRelationship = {
  id: string;
  subject: { id: string; name?: string };
  predicate: string;
  object: { id: string; name?: string };
  confidence?: number;
  method?: string;
  provenance?: GraphProvenance;
};

export type GraphData = {
  entities: GraphEntity[];
  relationships: GraphRelationship[];
};

// Raw shapes returned by /ingest/extract, /ingest/scan, /llm/extract (camelCase
// from Rust). Entities/relationships already carry `provenance` + confidence;
// these types opt into the fields this visualization consumes.
export type RawEntity = {
  id: string;
  name: string;
  kind?: string;
  aliases?: string[];
  sourceRefs?: GraphSourceRef[];
  provenance?: GraphProvenance;
};
export type RawRelationship = {
  id?: string;
  subject: { id?: string; name?: string };
  predicate: string;
  object: { id?: string; name?: string };
  confidence?: number;
  provenance?: GraphProvenance;
};

const LLM_METHOD = "llm_extraction";

/** True when a record was derived by the LLM (vs the deterministic extractor). */
function isLLMMethod(method: string | undefined): boolean {
  return method === LLM_METHOD;
}

/** Build a renderable graph from raw entities + relationships (shared by panels). */
export function buildGraphData(
  entities: RawEntity[],
  relationships: RawRelationship[]
): GraphData {
  const degree = new Map<string, number>();
  for (const rel of relationships) {
    if (rel.subject.id && rel.object.id) {
      degree.set(rel.subject.id, (degree.get(rel.subject.id) ?? 0) + 1);
      degree.set(rel.object.id, (degree.get(rel.object.id) ?? 0) + 1);
    }
  }
  return {
    entities: entities.map((entity) => ({
      id: entity.id,
      name: entity.name,
      kind: entity.kind ?? "unknown",
      degree: degree.get(entity.id) ?? 0,
      confidence: entity.provenance?.confidence,
      provenance: entity.provenance,
      aliases: entity.aliases,
      sourceRefs: entity.sourceRefs,
    })),
    relationships: relationships
      .filter((rel) => rel.subject.id && rel.object.id)
      .map((rel, index) => ({
        id: rel.id ?? `edge-${index}`,
        subject: { id: rel.subject.id!, name: rel.subject.name },
        predicate: rel.predicate,
        object: { id: rel.object.id!, name: rel.object.name },
        confidence: rel.confidence ?? rel.provenance?.confidence,
        method: rel.provenance?.method,
        provenance: rel.provenance,
      })),
  };
}

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

// Provenance/confidence encoding. Deterministic edges are blue-gray; LLM-extracted
// edges are amber so the source of each claim is legible at a glance. Confidence
// scales edge width/opacity and node opacity; missing confidence reads as 1.0.
const EDGE_DET = "#3a4768";
const EDGE_DET_DIM = "#26304a";
const EDGE_LLM = "#8a6a2a";
const EDGE_LLM_DIM = "#3a2f1a";

function clamp01(value: number | undefined): number {
  if (typeof value !== "number" || !Number.isFinite(value)) return 1;
  return Math.max(0, Math.min(1, value));
}

function edgeColor(isLLM: boolean, active: boolean, dimmed: boolean): string {
  if (active) return ACCENT;
  if (dimmed) return isLLM ? EDGE_LLM_DIM : EDGE_DET_DIM;
  return isLLM ? EDGE_LLM : EDGE_DET;
}

function edgeWidth(confidence: number | undefined, active: boolean): number {
  return active ? 2.4 : 0.5 + clamp01(confidence) * 1.2;
}

function edgeOpacity(confidence: number | undefined, dimmed: boolean): number {
  if (dimmed) return 0.2;
  return 0.4 + 0.6 * clamp01(confidence);
}

function nodeOpacity(confidence: number | undefined, dimmed: boolean): number {
  if (dimmed) return 0.25;
  return 0.55 + 0.45 * clamp01(confidence);
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
          transparent
          opacity={nodeOpacity(entity.confidence, dimmed)}
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
  const llm = isLLMMethod(relationship.method);
  return (
    <Line
      points={[from, to]}
      color={edgeColor(llm, active, dimmed)}
      lineWidth={edgeWidth(relationship.confidence, active)}
      transparent
      opacity={edgeOpacity(relationship.confidence, dimmed)}
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

function ConfidenceBar({ value }: { value?: number }) {
  const c = clamp01(value);
  return (
    <div className="graph3d__conf" aria-label={`confidence ${c.toFixed(2)}`}>
      <div className="graph3d__conf-fill" style={{ width: `${Math.round(c * 100)}%` }} />
    </div>
  );
}

function ProvenanceBlock({
  provenance,
  confidence,
}: {
  provenance?: GraphProvenance;
  confidence?: number;
}) {
  if (!provenance && typeof confidence !== "number") return null;
  const method = provenance?.method;
  const llm = isLLMMethod(method);
  return (
    <div className="graph3d__prov">
      <div className="graph3d__prov-head">provenance</div>
      <dl className="graph3d__fields">
        {provenance?.source && (
          <div>
            <dt>source</dt>
            <dd>
              <code>{provenance.source}</code>
            </dd>
          </div>
        )}
        {method && (
          <div>
            <dt>method</dt>
            <dd>
              <span className={`graph3d__badge${llm ? " graph3d__badge--llm" : ""}`}>
                {llm ? "LLM" : "deterministic"}
              </span>
            </dd>
          </div>
        )}
        <div>
          <dt>confidence</dt>
          <dd>
            <ConfidenceBar value={confidence ?? provenance?.confidence} />
          </dd>
        </div>
        {provenance?.observedAt && (
          <div>
            <dt>observed</dt>
            <dd>{new Date(provenance.observedAt).toLocaleString()}</dd>
          </div>
        )}
      </dl>
    </div>
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
        <ProvenanceBlock provenance={entity.provenance} confidence={entity.confidence} />
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
      </dl>
      <ProvenanceBlock provenance={rel.provenance} confidence={rel.confidence} />
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
      {relationships.length > 0 && (
        <div className="graph3d__legend">
          <span className="graph3d__legend-item">
            <i style={{ background: EDGE_DET }} /> deterministic
          </span>
          <span className="graph3d__legend-item">
            <i style={{ background: EDGE_LLM }} /> LLM-extracted
          </span>
        </div>
      )}
      {entities.length === 0 && (
        <div className="graph3d__empty">Ingest a document to populate the graph.</div>
      )}
    </div>
  );
}
