# Plan: demo-ui-shell (engram demo frontend → shadcn-admin shell)

Re-skin `demo/frontend` onto the shadcn-admin shell (TanStack Router + Tailwind +
shadcn/ui + sidebar + command palette + dark mode), one route per capability,
keeping `Graph3D`, backend untouched. UI-only. Staged across commits.

Follows from RFC 0004 (the demo program). No ADR — package-internal reskin.

## Tasks

### T1 — Scaffold the shadcn-admin shell in `demo/frontend`
- **Tests:** goal-based — `vite dev` boots; `typecheck` + `build` green.
- **Depends on:** none
- **Approach:** Bring over shadcn-admin's infra: `tailwind.config`, `postcss.config`, `components.json`, `src/components/ui/` (sidebar, command, button, dialog, dropdown-menu, table, sonner, theme-provider, etc.), `src/lib/utils.ts`, the `ThemeProvider` + dark-mode toggle, and TanStack Router (`src/routes/` + router config). Add deps (`tailwindcss`, `@tanstack/react-router`, `lucide-react`, Radix peers, `clsx`/`tailwind-merge`). Configure the Vite proxy so the shell calls the existing backend on the same origin. A minimal root layout (`Sidebar` + `<Outlet />`) renders placeholder routes.

### T2 — Port capabilities into routes (one route per capability, one commit each)
- **Tests:** goal-based — `typecheck` + `build` per route; route→endpoint mapping check (table below) confirms no capability lost.
- **Depends on:** T1
- **Approach:** Lift existing fetch logic out of the old panels into TanStack Router routes, skinned with shadcn/ui. One commit per route; keep the old panels until T5 so the app stays runnable throughout. Route→endpoint map (the contract each route must call — all pre-existing, unchanged):

  | Route | Panel lifted | Backend calls |
  |---|---|---|
  | `/ingest` | `IngestPanel` | `POST /ingest/extract`, `POST /llm/extract` |
  | `/index` | `ScanPanel` | `POST /ingest/scan` (NDJSON) |
  | `/knowledge` | `OntologyPanel` | `POST /ontology/{ontology,class,property,axiom,get,it-org,validate}`, `POST /taxonomy/*` |
  | `/belief` | `BeliefPanel` | `POST /belief/{put,list,contradiction,contradictions,get,resolve,detect}` |
  | `/memory` | new `MemoryPanel` (from `App.tsx`) | `POST /memory/{write,retrieve,forget}` |
  | `/chat` | `QAPanel` + context-composer view (folds `SearchPanel`) | `POST /qa/ask`, `POST /retrieval/{index,search}` |

### T3 — Embed `Graph3D` as a shared viz
- **Tests:** goal-based — `typecheck` + `build`; renders on `/ingest`, `/index`, `/chat`, `/`.
- **Depends on:** T2
- **Approach:** `Graph3D` (`react-three-fiber`) stays as-is; wrap it in a shadcn `<Card>` so it reads as a first-class surface on the graph-bearing routes + dashboard. No 3D logic change.

### T4 — Sidebar nav + command palette + dark mode
- **Tests:** goal-based — `typecheck` + `build`; manual QA (operator-run checklist: each sidebar item navigates; command palette opens + jumps; dark-mode toggle persists).
- **Depends on:** T2
- **Approach:** Wire shadcn-admin's `Sidebar` (one item per route) + the global-search command palette (route quick-jump) + the theme toggle. Remove the old `app__header`/`app__columns` chrome.

### T5 — Remove the old plain-CSS stack (no duplication)
- **Tests:** goal-based — `typecheck` + `build`; gate: `git grep -nE "Panel|app__columns" demo/frontend/src` returns only the new route modules (no orphaned old panels); `styles.css` + old `App.tsx` composition gone.
- **Depends on:** T3, T4
- **Approach:** Delete `styles.css` + the old single-page `App.tsx` panel stack once every capability has a route. Keep only the route modules + shared components.

### T6 — Validate + lighter adversarial pass
- **Tests:** `demo/frontend` `typecheck` + `build`; `vite dev` boot; route→endpoint mapping (T2 table) exercised end-to-end (manual QA: ingest → 3D graph → index → knowledge → belief → memory → chat context composer + answer); existing backend smoke tests still green (backend unchanged); single-pass review focused on no-duplication, no-backend-change, + router/shell correctness.
- **Depends on:** T5

## Out of scope (logged)
- New backend capabilities; auth (Clerk); replacing the 3D viz; mobile-native builds; i18n/RTL; multi-tenant.
