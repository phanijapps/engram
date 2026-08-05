//! Slideover — a right-side drawer (zbot's Observatory detail pattern). Click
//! the backdrop (or ✕) to close. Self-contained styling via CSS vars.

import type { CSSProperties, ReactNode } from "react";

export function Slideover({
  open,
  onClose,
  title,
  subtitle,
  children,
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  subtitle?: string;
  children: ReactNode;
}) {
  if (!open) return null;
  return (
    <div style={overlayStyle} onClick={onClose}>
      <aside style={panelStyle} onClick={(e) => e.stopPropagation()}>
        <div style={headStyle}>
          <div>
            <div style={titleStyle}>{title}</div>
            {subtitle && <div style={subStyle}>{subtitle}</div>}
          </div>
          <button style={closeStyle} onClick={onClose} aria-label="close">
            ✕
          </button>
        </div>
        <div style={bodyStyle}>{children}</div>
      </aside>
    </div>
  );
}

const mono = "var(--font-mono)" as const;

const overlayStyle: CSSProperties = {
  position: "fixed",
  inset: 0,
  background: "rgba(0,0,0,0.5)",
  zIndex: 20,
  display: "flex",
  justifyContent: "flex-end",
};
const panelStyle: CSSProperties = {
  width: 380,
  maxWidth: "90vw",
  height: "100%",
  background: "var(--sidebar)",
  borderLeft: "1px solid var(--border)",
  padding: "var(--spacing-4)",
  overflowY: "auto",
  fontFamily: mono,
  display: "flex",
  flexDirection: "column",
  gap: "var(--spacing-3)",
};
const headStyle: CSSProperties = {
  display: "flex",
  justifyContent: "space-between",
  alignItems: "flex-start",
  gap: "var(--spacing-2)",
};
const titleStyle: CSSProperties = { color: "var(--foreground)", fontSize: 14, fontWeight: 600 };
const subStyle: CSSProperties = { color: "var(--muted-foreground)", fontSize: 11 };
const closeStyle: CSSProperties = {
  background: "transparent",
  border: "1px solid var(--border)",
  color: "var(--muted-foreground)",
  borderRadius: "var(--radius-sm)",
  cursor: "pointer",
  fontSize: 12,
  lineHeight: 1,
  padding: "2px 6px",
};
const bodyStyle: CSSProperties = { display: "flex", flexDirection: "column", gap: "var(--spacing-3)" };
