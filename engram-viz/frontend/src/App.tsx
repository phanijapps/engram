//! engram-viz app shell — zbot-style top-bar + 3-tab nav (Memory / Observatory
//! / Graph) over the in-process BFF. T7 ships the styled shell; the Graph deck.gl
//! overview lands in T8; Memory/Observatory in S3/S4.

import { useEffect, useState, type ComponentType } from "react";
import { BrowserRouter, Routes, Route, Navigate, NavLink, Link } from "react-router-dom";
import { Brain, Network, Layers, Activity } from "lucide-react";

import { api, type Health } from "./lib/api.ts";
import { GraphOverview } from "./features/graph/GraphOverview.tsx";
import { MemoryTab } from "./features/memory/MemoryTab.tsx";

interface NavItem {
  to: string;
  label: string;
  icon: ComponentType<{ className?: string }>;
}

const NAV: NavItem[] = [
  { to: "/memory", label: "Memory", icon: Brain },
  { to: "/observatory", label: "Observatory", icon: Network },
  { to: "/graph", label: "Graph", icon: Layers },
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
        <Link to="/graph" className="topbar__brand">
          <span className="topbar__brand-mark">e</span>
          <span className="topbar__brand-name">engram<b>-viz</b></span>
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
        </div>
      </header>

      <main className="app-shell__main">
        <Routes>
          <Route path="/" element={<Navigate to="/graph" replace />} />
          <Route path="/memory" element={<MemoryTab />} />
          <Route path="/observatory" element={<Placeholder title="Observatory" note="Graph / belief / hierarchy health — S4." />} />
          <Route path="/graph" element={<GraphOverview />} />
          <Route path="*" element={<Navigate to="/graph" replace />} />
        </Routes>
      </main>
    </div>
  );
}

function Placeholder({ title, note }: { title: string; note: string }) {
  return (
    <div className="page">
      <div className="page-container">
        <h1 style={{ fontFamily: "var(--font-display)", color: "var(--foreground)", margin: 0 }}>
          {title}
        </h1>
        <p style={{ fontFamily: "var(--font-mono)", color: "var(--muted-foreground)", marginTop: "var(--spacing-2)" }}>
          {note}
        </p>
      </div>
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
