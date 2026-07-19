//! A titled, scrollable list of insight items. Clicking an item focuses its node.

import type { ReactNode } from "react";
import type { InsightItem } from "../../lib/types";

interface InsightCardProps {
  title: string;
  icon: ReactNode;
  items: InsightItem[];
  loading?: boolean;
  onSelect: (id: string) => void;
  /** Show the numeric score (central/bridge) or the category tag (dead code). */
  badge?: "score" | "category";
  accentClass?: string;
}

const kindColor: Record<string, string> = {
  function: "text-accent-cyan",
  method: "text-accent-cyan",
  struct: "text-accent-purple",
  class: "text-accent-purple",
  module: "text-accent-amber",
  variable: "text-accent-green",
  trait: "text-accent-purple",
  enum: "text-accent-purple",
};

export function InsightCard({
  title,
  icon,
  items,
  loading,
  onSelect,
  badge = "score",
  accentClass = "text-accent",
}: InsightCardProps) {
  return (
    <div className="rounded-lg border border-base-700 bg-base-850/60">
      <div className="flex items-center gap-2 border-b border-base-700 px-3 py-2">
        <span className={accentClass}>{icon}</span>
        <h3 className="text-xs font-semibold uppercase tracking-wider text-ink-muted">
          {title}
        </h3>
        <span className="ml-auto text-[10px] text-ink-faint">
          {loading ? "…" : items.length}
        </span>
      </div>
      <ul className="max-h-52 overflow-y-auto">
        {items.map((item, i) => (
          <li key={`${item.id}-${i}`}>
            <button
              onClick={() => onSelect(item.id)}
              className="group flex w-full items-center gap-2 px-3 py-1.5 text-left hover:bg-base-750/80"
            >
              <span className={`text-[10px] font-mono ${kindColor[item.kind] ?? "text-ink-faint"}`}>
                {item.kind.slice(0, 4)}
              </span>
              <span className="truncate text-xs text-ink group-hover:text-accent">
                {item.name}
              </span>
              {badge === "score" && item.score !== undefined && (
                <span className="ml-auto shrink-0 font-mono text-[10px] text-ink-faint">
                  {item.score.toFixed(3)}
                </span>
              )}
              {badge === "category" && item.category && (
                <span className="ml-auto shrink-0 rounded bg-base-700 px-1.5 py-0.5 text-[9px] uppercase text-ink-faint">
                  {item.category}
                </span>
              )}
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
