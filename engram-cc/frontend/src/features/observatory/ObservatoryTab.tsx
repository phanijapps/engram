//! Observatory tab — the knowledge-graph command center (zbot model). This is
//! the SOLE graph view (the old Graph tab merged in): a toolbar (highlight search
//! + density + refresh) + the community-overview canvas with drill (GraphOverview)
//! + a bottom LearningHealthBar. Belief/hierarchy are empty today → the health
//! strip shows them dim with a details slideover carrying the populate pointer.

import { useState, type CSSProperties } from "react";
import { RefreshCw, Search } from "lucide-react";

import { GraphOverview } from "../graph/GraphOverview.tsx";
import { LearningHealthBar } from "./LearningHealthBar.tsx";

const DENSITIES = [50, 150, 500];

export function ObservatoryTab() {
  const [limit, setLimit] = useState(150);
  const [highlight, setHighlight] = useState("");
  const [refreshSignal, setRefreshSignal] = useState(0);

  return (
    <div style={wrapStyle}>
      <div style={toolbarStyle}>
        <div style={toolbarLeftStyle}>
          <span style={titleStyle}>OBSERVATORY</span>
          <div style={searchStyle}>
            <Search style={{ width: 13, height: 13, opacity: 0.6 }} aria-hidden />
            <input
              style={inputStyle}
              placeholder="highlight community (e.g. c65)…"
              value={highlight}
              onChange={(e) => setHighlight(e.target.value)}
            />
          </div>
        </div>
        <div style={toolbarRightStyle}>
          <span style={densityLabelStyle}>TOP</span>
          {DENSITIES.map((d) => (
            <button
              key={d}
              style={d === limit ? densActiveStyle : densStyle}
              onClick={() => setLimit(d)}
            >
              {d}
            </button>
          ))}
          <button
            style={refreshBtnStyle}
            onClick={() => setRefreshSignal((s) => s + 1)}
            title="refresh"
            aria-label="refresh"
          >
            <RefreshCw style={{ width: 13, height: 13 }} aria-hidden /> Refresh
          </button>
        </div>
      </div>

      <div style={canvasStyle}>
        <GraphOverview limit={limit} highlight={highlight} refreshSignal={refreshSignal} />
      </div>

      <LearningHealthBar refreshSignal={refreshSignal} />
    </div>
  );
}

const mono = "var(--font-mono)" as const;
const wrapStyle: CSSProperties = {
  display: "flex",
  flexDirection: "column",
  width: "100%",
  height: "100%",
};
const toolbarStyle: CSSProperties = {
  display: "flex",
  justifyContent: "space-between",
  alignItems: "center",
  gap: "var(--spacing-3)",
  padding: "var(--spacing-2) var(--spacing-4)",
  borderBottom: "1px solid var(--border)",
  background: "var(--sidebar)",
  fontFamily: mono,
  flex: "0 0 auto",
};
const toolbarLeftStyle: CSSProperties = { display: "flex", alignItems: "center", gap: "var(--spacing-3)" };
const toolbarRightStyle: CSSProperties = { display: "flex", alignItems: "center", gap: "var(--spacing-2)" };
const titleStyle: CSSProperties = { letterSpacing: "0.12em", color: "var(--muted-foreground)", fontSize: 11 };
const searchStyle: CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: "var(--spacing-1)",
  background: "var(--background)",
  border: "1px solid var(--border)",
  borderRadius: "var(--radius-sm)",
  padding: "2px var(--spacing-2)",
  color: "var(--muted-foreground)",
};
const inputStyle: CSSProperties = {
  background: "transparent",
  border: "none",
  outline: "none",
  color: "var(--foreground)",
  fontFamily: mono,
  fontSize: 12,
  width: 200,
};
const densityLabelStyle: CSSProperties = { color: "var(--muted-foreground)", fontSize: 10, letterSpacing: "0.08em" };
const densBase: CSSProperties = {
  background: "transparent",
  border: "1px solid var(--border)",
  color: "var(--muted-foreground)",
  borderRadius: "var(--radius-sm)",
  cursor: "pointer",
  fontFamily: mono,
  fontSize: 11,
  padding: "1px var(--spacing-2)",
};
const densStyle: CSSProperties = { ...densBase };
const densActiveStyle: CSSProperties = { ...densBase, color: "var(--primary, #7df9ff)", borderColor: "var(--primary, #7df9ff)" };
const refreshBtnStyle: CSSProperties = {
  ...densBase,
  display: "inline-flex",
  alignItems: "center",
  gap: 4,
};
const canvasStyle: CSSProperties = { position: "relative", flex: "1 1 auto", minHeight: 0 };
