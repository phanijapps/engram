//! CODE tab — the entity's source text (from its chunk), if available.

import { FileQuestion } from "lucide-react";
import type { NodeDetail } from "../../lib/types";

export function CodeTab({ detail }: { detail: NodeDetail }) {
  const source = detail.source;
  if (!source) {
    return (
      <div className="flex flex-col items-center justify-center gap-2 px-6 py-12 text-center">
        <FileQuestion size={28} className="text-ink-faint" />
        <p className="text-xs text-ink-muted">No source text stored for this symbol.</p>
        <p className="text-[10px] text-ink-faint">
          The codegraph indexes symbol bodies; some declarations (e.g. traits,
          type aliases) have no extractable chunk.
        </p>
      </div>
    );
  }
  return (
    <div className="overflow-auto p-3">
      {detail.complexity !== null && (
        <div className="mb-2 flex items-center gap-2 text-[10px] text-ink-faint">
          <span className="rounded bg-base-700 px-1.5 py-0.5 font-mono">
            cc {detail.complexity}
          </span>
          <span>cyclomatic complexity</span>
        </div>
      )}
      <pre className="overflow-x-auto rounded-md border border-base-700 bg-base-950/80 p-3 font-mono text-[11px] leading-relaxed text-ink">
        <code>{source}</code>
      </pre>
    </div>
  );
}
