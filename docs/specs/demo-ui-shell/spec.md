# Spec: demo-ui-shell (engram demo frontend → shadcn-admin shell)

- **Status:** Draft
- **Shape:** ui
- **Constrained by:** RFC-0004 (the demo program; the Rust-backed "soul"); the existing demo backend routes are the contract this UI consumes — they do not change
- **Contract:** none (frontend-only; consumes the existing `/memory`, `/ingest`, `/llm`, `/ontology`, `/taxonomy`, `/belief`, `/qa`, `/retrieval` routes)
- **Reference:** [satnaing/shadcn-admin](https://github.com/satnaing/shadcn-admin) — UI/UX patterns only (the soul stays engram)

## Objective

Migrate the engram demo frontend (`demo/frontend`) onto the shadcn-admin shell — TanStack Router (one route per capability) + Tailwind + shadcn/ui + a sidebar, command palette, and dark mode — porting each existing capability into exactly one route (verified by no orphaned `*Panel` imports after the old stack is removed), and keeping the `react-three-fiber` 3D knowledge graph as the primary 3D visualization. The backend is untouched: the "soul" (Rust-backed ingest, knowledge graph, taxonomy/ontology, memory, belief/contradiction, Q&A) is unchanged; this slice only re-skins how it is reached.

The demo's capabilities map to routes:
- **`/ingest`** — ingest docs/text + extract a graph (deterministic + optional LLM enhance). *(IngestPanel)*
- **`/index`** — point at a repo/folder and index code (scan job, incremental). *(ScanPanel)*
- **`/knowledge`** — ontology + taxonomy maintenance (IT-org sample, validate, browse classes/properties/concepts). *(OntologyPanel)*
- **`/belief`** — belief + contradiction maintenance (add beliefs with valid-time, detect, resolve). *(BeliefPanel)*
- **`/memory`** — write / retrieve / forget memories (factored out of `App.tsx`). *(new MemoryPanel)*
- **`/chat`** — Q&A over knowledge + memory, with a **context composer**: a UI panel that assembles and shows the grounding (memory items + beliefs from `/qa/ask`'s `sources`, plus semantic hits from `/retrieval/search`) so the user sees exactly what grounds the answer. *(QAPanel + context-composer view; `SearchPanel`'s retrieval is folded in here)*
- **`/` (dashboard)** — the 3D graph overview.

`Graph3D` is the primary 3D visualization, wrapped in a shadcn `Card` and rendered as a route child on `/ingest`, `/index`, `/chat`, and `/`.

The backend is unchanged: `/qa/ask`, `/retrieval/index`, `/retrieval/search`, and every other existing route persist as-is — the context composer is a pure UI view that calls them.

## Decision

Adopt shadcn-admin's stack wholesale (TanStack Router + Tailwind + shadcn/ui + Lucide + Vite + React 18 — Vite is already the demo's build tool, so no build-tool migration). Reuse shadcn-admin's UI primitives (sidebar, command palette, theme toggle, data table, dialog, etc.) rather than reimplementing them. Keep `@react-three/fiber`/`drei`/`three` for `Graph3D` — shadcn-admin has no 3D, so it is preserved as-is and embedded in the shell. Drop Clerk auth (the demo is single-user/local; auth is not part of the engram soul). The existing plain-CSS panel stack (`App.tsx` columns + `styles.css`) is replaced, not duplicated.

## Assumptions

- Technical: shadcn-admin stack = ShadcnUI (Tailwind + Radix) + Vite + TanStack Router + TS + ESLint/Prettier + Lucide; sidebar, command palette, dark mode, RTL. ([satnaing/shadcn-admin README](https://github.com/satnaing/shadcn-admin))
- Technical: current `demo/frontend` = React 18 + Vite + @react-three/fiber v8/drei v9/three + plain CSS; panels `IngestPanel`/`ScanPanel`/`OntologyPanel`/`BeliefPanel`/`QAPanel`/`SearchPanel`/`Graph3D` + a memory panel inlined in `App.tsx`. (`demo/frontend/package.json`, `src/`)
- Technical: Vite is shared → no build-tool migration; the 3D graph has no shadcn-admin equivalent. (both)
- Product: route map + "backend untouched" + "keep 3D" + "drop auth" confirmed by user 2026-06-29. (user confirmation)
- Process: lighter single-pass adversarial review. (user standing preference)

## Boundaries

**Always do**
- One route per capability — each existing capability lives in exactly one place. After the old stack is removed, `git grep -E "Panel|app__columns"` under `demo/frontend/src` returns only the new route modules (no orphaned old panels).
- Reuse shadcn-admin components for UI primitives; do not hand-roll a second component set.
- Keep `Graph3D` (`react-three-fiber`) as the primary 3D viz, wrapped in a shadcn `Card`.
- Keep the backend + all existing routes unchanged (`/qa/*`, `/retrieval/*`, etc. persist); lift existing fetch logic out of the old panels into the new routes verbatim where possible.
- Factor the memory panel out of `App.tsx` into its own `MemoryPanel` route.
- Give belief/contradiction its own route (`/belief`) so no existing capability is orphaned.

**Ask first**
- Adding new backend routes/capabilities (this slice is UI-only).
- Replacing the 3D viz with tables.
- Adding auth, i18n/RTL, or multi-tenant concerns.

**Never do**
- Duplicate a capability across routes/panels.
- Change Rust, contracts, or any backend route.
- Add Clerk/auth, or a second router or styling system (Tailwind is the one system).
- Ship the old plain-CSS `App.tsx` stack alongside the new shell.

## Testing Strategy

- **Goal-based (build):** `demo/frontend` `typecheck` + `build` (Vite + TanStack Router); `vite dev` boots; every route renders without runtime error.
- **Goal-based (plumbing):** each route still calls the existing backend routes correctly — since the backend is unchanged, the existing backend smoke tests remain the contract proof; a route-to-endpoint mapping check confirms no capability lost.
- **Manual QA:** sidebar nav → `/ingest` (see 3D graph) → `/index` → `/knowledge` → `/memory` → `/chat` (context composer + answer + sources); dark-mode toggle; command palette opens.

## Acceptance Criteria

- [ ] `demo/frontend` runs on TanStack Router with routes `/`, `/ingest`, `/index`, `/knowledge`, `/belief`, `/memory`, `/chat`.
- [ ] Tailwind + shadcn/ui + sidebar + command palette + dark mode are adopted from shadcn-admin.
- [ ] Each capability (ingest docs/text, index code, ontology+taxonomy maintenance, belief/contradiction maintenance, memory, Q&A + context composer) lives in exactly one route — `git grep` confirms no orphaned old `*Panel`/`app__columns` after the old stack is removed.
- [ ] The `/chat` context composer is a UI view assembling grounding from `/qa/ask` (`sources`) + `/retrieval/search`; no new backend route.
- [ ] `Graph3D` (`react-three-fiber`) is wrapped in a shadcn `Card` and rendered on `/ingest`, `/index`, `/chat`, and `/` — no 3D logic change.
- [ ] The memory panel is factored out of `App.tsx` into a `MemoryPanel` route.
- [ ] The old plain-CSS `App.tsx` panel stack + `styles.css` are removed (not left alongside).
- [ ] Backend untouched; existing routes still work; `demo/frontend` `typecheck` + `build` pass.
