//! engram-cc (Control Center) app shell — zbot-style top-bar + 3-tab nav (Memory / Observatory
//! / Graph). Observatory is the interactive 2D graph explorer (canvas + drill +
//! health strip); Graph is the animated 3D globe overview; Memory is the facts/
//! beliefs/procedures deck. The BFF reads engram in-process via @engram/node.

import { useEffect, useState, type ComponentType } from "react";
import { BrowserRouter, Routes, Route, Navigate, NavLink, Link } from "react-router-dom";
import { Brain, Network, Layers, Activity, Upload, Sparkles } from "lucide-react";

import { api, type Health } from "./lib/api.ts";
import { GlobeGraph } from "./features/graph/GlobeGraph.tsx";
import { IngestTab } from "./features/ingest/IngestTab.tsx";
import { MaintainTab } from "./features/maintain/MaintainTab.tsx";
import { MemoryTab } from "./features/memory/MemoryTab.tsx";
import { ObservatoryTab } from "./features/observatory/ObservatoryTab.tsx";

interface NavItem {
  to: string;
  label: string;
  icon: ComponentType<{ className?: string }>;
}

const NAV: NavItem[] = [
  { to: "/memory", label: "Memory", icon: Brain },
  { to: "/observatory", label: "Observatory", icon: Network },
  { to: "/graph", label: "Graph", icon: Layers },
  { to: "/ingest", label: "Ingest", icon: Upload },
  { to: "/maintain", label: "Maintain", icon: Sparkles },
];

function WebAppShell() {
  const [health, setHealth] = useState<Health | null>(null);

  useEffect(() => {
    let cancelled = false;
    api.health().then((h) => !cancelled && setHealth(h)).catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  const ok = health?.status === "ok";

  return (
    <div className="app-shell">
      <span className="app-shell__reticle app-shell__reticle--tl" />
      <span className="app-shell__reticle app-shell__reticle--tr" />
      <span className="app-shell__reticle app-shell__reticle--bl" />
      <span className="app-shell__reticle app-shell__reticle--br" />

      <header className="topbar">
        <Link to="/observatory" className="topbar__brand">
          <span className="topbar__brand-mark">e</span>
          <span className="topbar__brand-name">engram<b>-cc</b></span>
        </Link>

        <nav className="topbar__nav">
          {NAV.map(({ to, label, icon: Icon }) => (
            <NavLink
              key={to}
              to={to}
              className={({ isActive }) =>
                "nav-link" + (isActive ? " nav-link--active" : "")
              }
            >
              <Icon className="nav-link__icon" />
              <span className="nav-link__label">{label}</span>
            </NavLink>
          ))}
        </nav>

        <div className="topbar__right">
          <span className="status-pill" data-ok={ok ? "true" : "false"} title="BFF health">
            <Activity className="nav-link__icon" />
            <span className="status-pill__dot" />
            {health ? health.scope.workspace : "—"}
          </span>
          {health?.mcp && (
            <span className="status-pill" data-ok={health.mcp === "up" ? "true" : "false"} title="MCP server (:8788)">
              <span className="status-pill__dot" />
              MCP
            </span>
          )}
        </div>
      </header>

      <main className="app-shell__main">
        <Routes>
          <Route path="/" element={<Navigate to="/observatory" replace />} />
          <Route path="/memory" element={<MemoryTab />} />
          <Route path="/observatory" element={<ObservatoryTab />} />
          <Route path="/graph" element={<GlobeGraph />} />
          <Route path="/ingest" element={<IngestTab />} />
          <Route path="/maintain" element={<MaintainTab />} />
          <Route path="*" element={<Navigate to="/observatory" replace />} />
        </Routes>
      </main>
    </div>
  );
}

export default function App() {
  return (
    <BrowserRouter>
      <WebAppShell />
    </BrowserRouter>
  );
}
