//! Taxonomy sidebar panel: concept schemes as a collapsible tree
//! (broader → narrower). Clicking a concept highlights matching entities
//! in the graph.

import { useEffect, useMemo, useState } from "react";
import { ChevronDown, ChevronRight, Search, Tag } from "lucide-react";
import { api } from "../../lib/api";
import { useGraphStore } from "../../store/graphStore";
import type { TaxonomyResponse, TaxonomyConcept } from "../../lib/types";

interface ConceptNode {
  concept: TaxonomyConcept;
  children: ConceptNode[];
}

/** Builds a forest of broader→narrower trees from flat concept + relation data. */
function buildForest(
  concepts: TaxonomyConcept[],
  relations: { subjectId: string; predicate: string; objectId: string }[],
): ConceptNode[] {
  // narrower relations point from broader → narrower.
  // subject=broader, object=narrower in SKOS broader/narrower.
  // Our schema stores the relation as { subject, predicate, object } where
  // predicate "narrower" means subject is narrower than object (transitive).
  // We invert: treat "broader" relations as parent→child edges.
  const childrenOf = new Map<string, string[]>();
  const hasParent = new Set<string>();

  for (const r of relations) {
    if (r.predicate === "broader") {
      // subject has a broader parent: object is the parent.
      const list = childrenOf.get(r.objectId) ?? [];
      list.push(r.subjectId);
      childrenOf.set(r.objectId, list);
      hasParent.add(r.subjectId);
    } else if (r.predicate === "narrower") {
      // subject is broader; object is the narrower child.
      const list = childrenOf.get(r.subjectId) ?? [];
      list.push(r.objectId);
      childrenOf.set(r.subjectId, list);
      hasParent.add(r.objectId);
    }
  }

  const byId = new Map(concepts.map((c) => [c.id, c]));
  const built = new Map<string, ConceptNode>();

  function buildNode(concept: TaxonomyConcept): ConceptNode {
    const existing = built.get(concept.id);
    if (existing) return existing;
    const childIds = childrenOf.get(concept.id) ?? [];
    const children = childIds
      .map((cid) => byId.get(cid))
      .filter((c): c is TaxonomyConcept => !!c)
      .map(buildNode);
    const node = { concept, children };
    built.set(concept.id, node);
    return node;
  }

  // Roots are concepts with no broader parent.
  const roots = concepts.filter((c) => !hasParent.has(c.id));
  return roots.map(buildNode);
}

function ConceptTreeNode({ node, depth }: { node: ConceptNode; depth: number }) {
  const [open, setOpen] = useState(depth < 2);
  const nodes = useGraphStore((s) => s.nodes);
  const setHighlight = useGraphStore((s) => s.setHighlight);
  const clearHighlight = useGraphStore((s) => s.clearHighlight);
  const focusNode = useGraphStore((s) => s.focusNode);

  const hasChildren = node.children.length > 0;
  const label = node.concept.prefLabel?.value ?? node.concept.id;

  const handleClick = () => {
    // Highlight graph nodes whose name matches the concept label.
    const labelLower = label.toLowerCase();
    const matches = new Set(
      nodes
        .filter(
          (n) =>
            n.name.toLowerCase().includes(labelLower) ||
            n.name.toLowerCase() === labelLower,
        )
        .map((n) => n.id),
    );
    if (matches.size > 0) {
      setHighlight(matches, "#58a6ff");
      // Focus the first match.
      const first = nodes.find((n) => matches.has(n.id));
      if (first) focusNode(first.id);
    } else {
      clearHighlight();
    }
  };

  return (
    <div>
      <div
        className="group flex items-center gap-1 rounded px-1 py-0.5 hover:bg-base-750"
        style={{ paddingLeft: `${depth * 12 + 4}px` }}
      >
        {hasChildren ? (
          <button
            onClick={() => setOpen(!open)}
            className="shrink-0 text-ink-faint hover:text-ink"
          >
            {open ? <ChevronDown size={11} /> : <ChevronRight size={11} />}
          </button>
        ) : (
          <span className="w-[11px] shrink-0" />
        )}
        <button
          onClick={handleClick}
          className="flex min-w-0 flex-1 items-center gap-1.5 text-left"
        >
          <Tag size={10} className="shrink-0 text-accent-purple" />
          <span className="truncate text-xs text-ink-muted group-hover:text-accent">
            {label}
          </span>
        </button>
      </div>
      {open &&
        hasChildren &&
        node.children.map((child) => (
          <ConceptTreeNode key={child.concept.id} node={child} depth={depth + 1} />
        ))}
    </div>
  );
}

export function TaxonomyPanel() {
  const [data, setData] = useState<TaxonomyResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [filter, setFilter] = useState("");

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    api
      .taxonomy()
      .then((d) => {
        if (!cancelled) setData(d);
      })
      .catch(() => {
        if (!cancelled) setData({ schemes: [], concepts: [], relations: [] });
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const filteredConcepts = useMemo(() => {
    if (!data || filter.trim().length === 0) return data?.concepts ?? [];
    const f = filter.toLowerCase();
    return (data?.concepts ?? []).filter(
      (c) =>
        c.prefLabel?.value?.toLowerCase().includes(f) ||
        c.definition?.toLowerCase().includes(f),
    );
  }, [data, filter]);

  const forest = useMemo(() => {
    if (!data) return [];
    // If filtering, show flat; otherwise show tree.
    if (filter.trim().length > 0) {
      return filteredConcepts.map((c) => ({ concept: c, children: [] }));
    }
    return buildForest(data.concepts, data.relations);
  }, [data, filteredConcepts, filter]);

  if (loading) {
    return (
      <div className="p-4 text-center text-xs text-ink-faint">
        Loading taxonomy…
      </div>
    );
  }

  if (!data || data.schemes.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center gap-2 px-6 py-12 text-center">
        <Tag size={26} className="text-ink-faint" />
        <p className="text-xs text-ink-muted">No taxonomy concepts indexed.</p>
        <p className="text-[10px] text-ink-faint">
          Scan a repo with concept extraction enabled to populate the taxonomy.
        </p>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      {/* Scheme summary */}
      <div className="border-b border-base-700 px-3 py-2">
        <div className="flex items-center gap-2">
          <span className="text-[10px] font-semibold uppercase tracking-wider text-ink-faint">
            Schemes
          </span>
          <span className="font-mono text-[10px] text-ink-muted">
            {data.schemes.length}
          </span>
          <span className="ml-auto font-mono text-[10px] text-ink-faint">
            {data.concepts.length} concepts
          </span>
        </div>
        <div className="mt-1 space-y-0.5">
          {data.schemes.map((s) => (
            <div
              key={s.id}
              className="truncate font-mono text-[10px] text-ink-faint"
              title={s.uri}
            >
              {s.name}{" "}
              <span className="text-ink-faint">v{s.version}</span>
            </div>
          ))}
        </div>
      </div>

      {/* Search filter */}
      <div className="border-b border-base-700 px-3 py-2">
        <div className="flex items-center gap-2 rounded-md border border-base-700 bg-base-850 px-2 py-1">
          <Search size={11} className="text-ink-faint" />
          <input
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            placeholder="Filter concepts…"
            className="w-full bg-transparent text-[11px] text-ink placeholder:text-ink-faint focus:outline-none"
          />
        </div>
      </div>

      {/* Concept tree */}
      <div className="flex-1 overflow-y-auto py-1">
        {forest.length === 0 ? (
          <p className="px-4 py-3 text-[11px] text-ink-faint">
            No concepts match the filter.
          </p>
        ) : (
          forest.map((node) => (
            <ConceptTreeNode
              key={node.concept.id}
              node={node}
              depth={0}
            />
          ))
        )}
      </div>
    </div>
  );
}
