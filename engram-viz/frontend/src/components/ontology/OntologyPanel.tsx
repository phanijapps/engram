//! Ontology sidebar panel: class definitions, property definitions, and
//! validation axioms for the indexed ontology.

import { useEffect, useState } from "react";
import {
  AlertTriangle,
  Boxes,
  ChevronDown,
  ChevronRight,
  CircleDot,
  Network,
} from "lucide-react";
import { api } from "../../lib/api";
import type { OntologyResponse } from "../../lib/types";

export function OntologyPanel() {
  const [data, setData] = useState<OntologyResponse | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    api
      .ontology()
      .then((d) => {
        if (!cancelled) setData(d);
      })
      .catch(() => {
        if (!cancelled)
          setData({ ontologies: [], classes: [], properties: [], axioms: [] });
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  if (loading) {
    return (
      <div className="p-4 text-center text-xs text-ink-faint">
        Loading ontology…
      </div>
    );
  }

  if (!data || data.ontologies.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center gap-2 px-6 py-12 text-center">
        <Network size={26} className="text-ink-faint" />
        <p className="text-xs text-ink-muted">No ontology defined.</p>
        <p className="text-[10px] text-ink-faint">
          Define an ontology with class and property constraints to see
          validation findings here.
        </p>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col overflow-y-auto">
      {/* Ontology headers */}
      <div className="border-b border-base-700 px-3 py-2">
        <div className="flex items-center gap-2">
          <span className="text-[10px] font-semibold uppercase tracking-wider text-ink-faint">
            Ontologies
          </span>
          <span className="font-mono text-[10px] text-ink-muted">
            {data.ontologies.length}
          </span>
        </div>
        <div className="mt-1 space-y-0.5">
          {data.ontologies.map((o) => (
            <div
              key={o.id}
              className="truncate font-mono text-[10px] text-ink-faint"
              title={o.uri}
            >
              {o.name}{" "}
              <span className="text-ink-faint">
                v{o.version} · {o.status}
              </span>
            </div>
          ))}
        </div>
      </div>

      {/* Classes */}
      <Section
        title="Classes"
        icon={<Boxes size={12} className="text-accent-purple" />}
        count={data.classes.length}
      >
        {data.classes.map((cls) => (
          <div
            key={cls.id}
            className="rounded-md border border-base-700 bg-base-850 px-2.5 py-1.5"
          >
            <div className="flex items-center gap-1.5">
              <span
                className="inline-block h-2 w-2 rounded-full"
                style={{ backgroundColor: "#bc8cff" }}
              />
              <span className="font-mono text-xs text-ink">{cls.label}</span>
              {cls.parentClassIds && cls.parentClassIds.length > 0 && (
                <span className="ml-auto text-[9px] text-ink-faint">
                  extends {cls.parentClassIds.length}
                </span>
              )}
            </div>
            {cls.description && (
              <p className="mt-1 text-[10px] leading-relaxed text-ink-faint">
                {cls.description}
              </p>
            )}
          </div>
        ))}
      </Section>

      {/* Properties */}
      <Section
        title="Properties"
        icon={<CircleDot size={12} className="text-accent-cyan" />}
        count={data.properties.length}
      >
        {data.properties.map((prop) => (
          <div
            key={prop.id}
            className="rounded-md border border-base-700 bg-base-850 px-2.5 py-1.5"
          >
            <div className="flex items-center gap-1.5">
              <span
                className="inline-block h-2 w-2 rounded-full"
                style={{ backgroundColor: "#39c5cf" }}
              />
              <span className="font-mono text-xs text-ink">{prop.label}</span>
              <span className="ml-auto rounded bg-base-700 px-1.5 py-0.5 text-[9px] uppercase text-ink-faint">
                {prop.kind}
              </span>
            </div>
            {(prop.domainClassId || prop.rangeClassId || prop.datatype) && (
              <div className="mt-1 flex flex-wrap gap-2 text-[9px] text-ink-faint">
                {prop.domainClassId && (
                  <span>
                    domain:{" "}
                    <span className="font-mono text-ink-muted">
                      {prop.domainClassId}
                    </span>
                  </span>
                )}
                {prop.rangeClassId && (
                  <span>
                    range:{" "}
                    <span className="font-mono text-ink-muted">
                      {prop.rangeClassId}
                    </span>
                  </span>
                )}
                {prop.datatype && (
                  <span>
                    type:{" "}
                    <span className="font-mono text-ink-muted">
                      {prop.datatype}
                    </span>
                  </span>
                )}
              </div>
            )}
          </div>
        ))}
      </Section>

      {/* Axioms */}
      {data.axioms.length > 0 && (
        <Section
          title="Axioms"
          icon={<AlertTriangle size={12} className="text-accent-amber" />}
          count={data.axioms.length}
        >
          {data.axioms.map((ax) => (
            <div
              key={ax.id}
              className="rounded-md border border-base-700 bg-base-850 px-2.5 py-1.5"
            >
              <span className="font-mono text-[10px] text-accent-amber">
                {ax.kind}
              </span>
              {(ax.subjectClassId || ax.propertyId || ax.objectClassId) && (
                <div className="mt-0.5 text-[9px] text-ink-faint">
                  {ax.subjectClassId && (
                    <span className="font-mono">{ax.subjectClassId} </span>
                  )}
                  {ax.propertyId && (
                    <span className="font-mono">→ {ax.propertyId} </span>
                  )}
                  {ax.objectClassId && (
                    <span className="font-mono">→ {ax.objectClassId}</span>
                  )}
                </div>
              )}
            </div>
          ))}
        </Section>
      )}
    </div>
  );
}

function Section({
  title,
  icon,
  count,
  children,
}: {
  title: string;
  icon: React.ReactNode;
  count: number;
  children: React.ReactNode;
}) {
  const [open, setOpen] = useState(true);
  return (
    <div className="border-b border-base-700">
      <button
        onClick={() => setOpen(!open)}
        className="flex w-full items-center gap-2 px-3 py-2"
      >
        {open ? (
          <ChevronDown size={11} className="text-ink-faint" />
        ) : (
          <ChevronRight size={11} className="text-ink-faint" />
        )}
        {icon}
        <span className="text-[10px] font-semibold uppercase tracking-wider text-ink-muted">
          {title}
        </span>
        <span className="ml-auto font-mono text-[10px] text-ink-faint">
          {count}
        </span>
      </button>
      {open && (
        <div className="space-y-1 px-3 pb-3">
          {count === 0 ? (
            <p className="text-[10px] text-ink-faint">None defined.</p>
          ) : (
            children
          )}
        </div>
      )}
    </div>
  );
}
