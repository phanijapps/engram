//! Playwright config for the viz-foundation S1 E2E.
//!
//! Self-contained: Playwright brings up BOTH the in-process BFF (backend, :3001)
//! and the production frontend preview (:4173, which proxies /api → :3001), then
//! runs the render/network-shape checks. The graph overview is WebGL; headless
//! Chrome uses software WebGL, so this suite gates render-without-crash +
//! bounded network shape — NOT FPS (FPS is a separate manual check on reference
//! hardware; see docs/specs/viz-foundation/plan.md T9).

import { defineConfig, devices } from "@playwright/test";

const BACKEND = "http://localhost:3001";
const FRONTEND = "http://localhost:4173";
const reuse = !process.env.CI;

export default defineConfig({
  testDir: "./e2e",
  fullyParallel: false,
  workers: 1,
  retries: 0,
  reporter: "list",
  use: {
    baseURL: FRONTEND,
    trace: "retain-on-failure",
    viewport: { width: 1320, height: 800 },
  },
  projects: [
    { name: "chromium", use: { ...devices["Desktop Chrome"] } },
  ],
  webServer: [
    {
      // In-process Hono BFF reading the agentzero store via @engram/node.
      command: "pnpm run start",
      cwd: "../backend",
      url: `${BACKEND}/api/health`,
      timeout: 60_000,
      reuseExistingServer: reuse,
    },
    {
      // Production build of the frontend, previewed with the /api proxy.
      command: "pnpm run build && pnpm run preview",
      cwd: ".",
      url: FRONTEND,
      timeout: 180_000,
      reuseExistingServer: reuse,
    },
  ],
});
