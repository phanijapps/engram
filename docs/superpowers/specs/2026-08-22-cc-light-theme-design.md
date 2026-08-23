# engram-cc Light Theme — Design Doc

**Date:** 2026-08-22
**Status:** Approved design (pending implementation plan)
**Scope owner:** `engram-cc/frontend`

## Summary

Reskin engram-cc from the futuristic dark theme (zbot-style: void background,
cyan glow, scanlines, holo borders) to a **light-only, Memory-Hub-inspired
admin theme** — clean white surfaces, slate text, a single restrained blue
accent, familiar admin-panel components. This is a visual redesign only:
no route, tab, data, backend, or BFF changes.

Inspiration source: TencentDB-Agent-Memory's Memory Hub panel (analysis in
`docs/research/tencentdb-agent-memory-comparison.md`; screenshots in that
repo under `assets/images/`). What we take from it: light palette discipline,
understated accent, card/list master patterns, calm hovers, readable
typography hierarchy. We do **not** take its org/asset IA in this pass.

## Decisions (locked with user)

1. **Light only.** The dark futuristic theme is replaced, not kept behind a
   toggle. No theme switcher.
2. **Structure unchanged.** The 6 tabs (Memory / Observatory / Graph / Ingest
   / Maintain / Ask), routes, and views stay as-is. IA changes (asset views,
   L0–L3) are a possible later pass.
3. **3D canvases stay dark.** Graph and Observatory deck.gl scenes keep their
   dark styling and glow tuning; they are framed as dark *viewports* inside
   light chrome (standard maps/earth-tool pattern). Lowest risk to the two
   hardest tabs.
4. **No new dependencies.** No shadcn adoption; existing CSS architecture
   (token files + tailwind v4 utilities) is kept.

## Current styling surface (measured)

- `src/index.css` (8 lines): import chain `fonts → tailwind → theme → shell → effects`
- `src/styles/theme.css` (278) — dark design tokens + `--fx-*` runtime knobs
- `src/styles/shell.css` (209) — top bar / nav chrome
- `src/styles/effects.css` (133) — void grid, scan beam, reticles, glow helpers
- `src/styles/fonts.css` (2), `src/features/graph/globe.css` (308, stays dark)
- Inline dark color literals in features: **~20 total** (graph 4, ingest 7,
  maintain 9; ask/memory/observatory clean)
- No tailwind arbitrary color utilities in features; no AccentPicker component
  exists in engram-cc (the theme.css comment referencing it is inherited from
  zbot and goes away with the rewrite).

## Design

### 1. Token rewrite (`theme.css`)

Rewrite to a light token set **keeping existing variable names** so
downstream rules keep resolving: `--background`, `--background-surface`,
`--background-elevated`, `--sidebar`, `--foreground` + 3 muted stops,
`--border`, `--border-hover`, `--primary`, `--primary-foreground`,
`--primary-hover`, `--primary-muted`, `--primary-subtle`, `--success`,
`--success-muted`, `--warning`, `--warning-muted`, `--destructive`,
`--destructive-muted`, avatar tints.

| Token | Light value |
|---|---|
| `--background` | `#ffffff` |
| `--background-surface` | `#f8fafc` |
| `--background-elevated` | `#ffffff` (elevation via border+shadow) |
| `--sidebar` | `#ffffff` |
| `--foreground` | `#0f172a` |
| `--muted-foreground` | `#475569` |
| `--subtle-foreground` | `#64748b` |
| `--dim-foreground` | `#94a3b8` |
| `--border` | `#e2e8f0` |
| `--border-hover` | `#cbd5e1` |
| `--primary` | `#2563eb` |
| `--primary-foreground` | `#ffffff` |
| `--primary-hover` | `#1d4ed8` |
| `--primary-muted` | `#dbeafe` |
| `--primary-subtle` | `#eff6ff` |
| `--success` / `--success-muted` | `#16a34a` / `#f0fdf4` |
| `--warning` / `--warning-muted` | `#d97706` / `#fffbeb` |
| `--destructive` / `--destructive-muted` | `#dc2626` / `#fef2f2` |
| `--blue` (avatar tints) | lightened set (implementation picks readable pairs) |

`--fx-accent / --fx-glow / --fx-grid / --fx-scan` are **removed**. All
references resolve to static light values (e.g. focus ring = `--primary`).

Shadows replace glow: `0 1px 2px rgba(15,23,42,.06)` cards,
`0 4px 12px rgba(15,23,42,.08)` popovers/dialogs.

### 2. Shell restyle (`shell.css`)

- Top bar: white background, `--border` bottom hairline, brand mark keeps
  gradient accent square (small), nav tabs with **blue underline active
  state** (Hub pattern) and slate inactive text; hover = `--background-surface`.
- Remove scan-beam/holo borders/glow halos; reticle decorations deleted.
- Focus states: 2px `--primary` ring at 2px offset for interactive elements.

### 3. Effects removal (`effects.css`)

Delete the futuristic layer: void + grid background (body becomes plain
`--background`), scan beam, corner reticles, glow/holo helpers. Any structurally
still-needed utility folds into `shell.css`. The file remains as a (possibly
near-empty) stub so the `index.css` import chain stays stable.

### 4. Viewport pattern (Graph + Observatory)

- Canvas containers: explicit dark viewport frame — `1px solid --border`,
  `border-radius: 10px`, inner background stays dark (`#07080d` — a local,
  canvas-scoped constant, not a theme token), subtle outer shadow.
- HUD/stat/pick panels around the canvas restyle light via tokens.
- `globe.css` stays dark (canvas-scoped); any `gg-*` panel classes that live
  outside the canvas go light.

### 5. Component idioms (applied via existing classes + tailwind)

- **Buttons:** primary filled `--primary` (white text); secondary white +
  `--border`; ghost text-only. Destructive = outlined red.
- **Inputs:** white, 1px `--border`, focus ring blue; `--background-surface` disabled.
- **Tabs:** underline active (blue), slate inactive.
- **Cards/lists:** `border-radius: 10px`, 1px `--border`, white bg, hover
  `#f8fafc`; selected = `--primary-subtle` + `--primary` left/underline marker.
- **Status pills:** soft tinted bg (`*-muted`) + dark semantic text + dot.
- **Tables:** `--background-surface` header, white rows, hover `#f8fafc`.
- **Typography:** system-UI/Inter-class stack; 14px body, 13px secondary,
  600-weight headings 16/18/20; small uppercase 11px labels (letter-spacing
  0.04em) only for metadata eyebrows.
- **Scrollbars:** light (`#cbd5e1` thumb, transparent track).

### 6. Feature sweep

Replace the ~20 inline dark color literals (graph 4 / ingest 7 / maintain 9)
with tokens or viewport-scoped constants; grep-verify zero remaining
`#07080d`-family literals outside canvas scopes; visual pass per tab for
stragglers (inline `style={{}}` objects, deck.gl overlay colors that are
*chrome* vs *scene*).

### 7. Verification

- `pnpm typecheck` and the frontend build (`pnpm cc` path) pass.
- Per-tab visual inspection in the running app.
- Before/after screenshots of all six tabs captured for user review.
- Existing Playwright smoke (if it asserts anything) must still pass; color
  assertions, if any exist, get updated to the new palette.

## Out of scope

- IA/structure changes (asset library views, L0–L3 visualization, import
  workflows) — candidate follow-up pass.
- Dark-mode toggle or theme switcher (light only).
- shadcn/ui or any new dependency.
- Backend/BFF/Rust changes of any kind.
