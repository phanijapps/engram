//! HISTORY tab — temporal provenance for the entity.

import { Clock, GitBranch } from "lucide-react";
import type { NodeDetail } from "../../lib/types";

function Row({ label, value }: { label: string; value?: string | null }) {
  if (!value) return null;
  return (
    <div className="flex items-start gap-2 py-1.5">
      <span className="w-24 shrink-0 text-[10px] uppercase tracking-wider text-ink-faint">
        {label}
      </span>
      <span className="font-mono text-[11px] text-ink-muted">{value}</span>
    </div>
  );
}

export function HistoryTab({ detail }: { detail: NodeDetail }) {
  const e = detail.entity;
  const introduced = e.validFrom;
  const retired = e.validUntil;
  const provenanceSource = e.provenance?.source;
  const observed = e.provenance?.observedAt;

  const hasAny = introduced || retired || provenanceSource || observed;

  return (
    <div className="p-4">
      {!hasAny ? (
        <div className="flex flex-col items-center gap-2 py-12 text-center">
          <Clock size={26} className="text-ink-faint" />
          <p className="text-xs text-ink-muted">No temporal data for this symbol.</p>
        </div>
      ) : (
        <>
          <div className="mb-3 flex items-center gap-2 text-xs text-ink">
            <Clock size={13} className="text-accent" />
            <span>Provenance</span>
          </div>
          <div className="divide-y divide-base-700/60 rounded-md border border-base-700 bg-base-850 px-3 py-1">
            <Row label="Introduced" value={fmt(introduced)} />
            <Row label="Retired" value={retired ? fmt(retired) : null} />
            <Row label="Observed" value={fmt(observed)} />
          </div>

          {provenanceSource && (
            <div className="mt-4">
              <div className="mb-1.5 flex items-center gap-2 text-[10px] font-semibold uppercase tracking-wider text-ink-faint">
                <GitBranch size={12} />
                Source
              </div>
              <div className="break-all rounded-md border border-base-700 bg-base-850 px-2.5 py-2 font-mono text-[11px] text-ink-muted">
                {provenanceSource}
              </div>
            </div>
          )}

          <p className="mt-4 text-[10px] leading-relaxed text-ink-faint">
            Temporal data is sparse when a repository is indexed in a single
            pass — every symbol shares one introduction timestamp. Per-version
            history requires incremental re-scans.
          </p>
        </>
      )}
    </div>
  );
}

function fmt(iso?: string | null): string | undefined {
  if (!iso) return undefined;
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toISOString().replace("T", " ").slice(0, 19) + " UTC";
}
