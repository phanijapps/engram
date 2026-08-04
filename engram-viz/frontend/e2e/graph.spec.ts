//! viz-foundation S1 E2E. Guards the contract the plan's T9 fixes:
//!  - the zbot-styled shell renders (brand + 3 nav tabs + live scope pill);
//!  - the Graph overview paints from REAL data (legend + a mounted WebGL canvas);
//!  - the network is BOUNDED — /api/graph/communities within node/edge caps and
//!    the overview never fires an unbounded entity/neighborhood dump; and
//!  - the bounded set renders without an uncaught crash.
//!
//! Headless Chrome = software WebGL: this is render-without-crash + network
//! shape, not FPS (manual gate on reference hardware, separate).

import { test, expect, type Response } from "@playwright/test";

const COMMUNITIES_MAX_NODES = 2000;
const COMMUNITIES_MAX_EDGES = 4000;

test.describe("viz-foundation S1", () => {
  test("styled shell + bounded Graph overview over real data, no crash", async ({
    page,
  }) => {
    const errors: string[] = [];
    page.on("pageerror", (e) => errors.push(`pageerror: ${e.message}`));
    page.on("console", (m) => {
      if (m.type() === "error") errors.push(`console.error: ${m.text()}`);
    });

    const apiCalls: string[] = [];
    page.on("response", (r: Response) => {
      const u = r.url();
      if (u.includes("/api/")) apiCalls.push(u.replace(/^https?:\/\/[^/]+/, ""));
    });

    const communitiesResp = page.waitForResponse(
      (r) => r.url().includes("/api/graph/communities") && r.ok(),
      { timeout: 20_000 },
    );

    await page.goto("/graph");

    // 1. Styled shell: brand + the three nav tabs.
    await expect(page.locator(".topbar__brand-name")).toContainText(/engram/i);
    await expect(
      page.getByRole("link", { name: /^memory$/i }),
    ).toBeVisible();
    await expect(
      page.getByRole("link", { name: /^observatory$/i }),
    ).toBeVisible();
    await expect(page.getByRole("link", { name: /^graph$/i })).toBeVisible();

    // 2. Live scope pill — the BFF is reachable on the real agentzero store.
    await expect(page.locator(".status-pill")).toContainText(/agentzero/i, {
      timeout: 10_000,
    });

    // 3. The overview legend renders — meta-nodes/edges arrived from real data.
    await expect(page.getByText(/\d[\d,]* communities · \d[\d,]* edges/i)).toBeVisible({
      timeout: 20_000,
    });

    // 4. deck.gl WebGL canvas is mounted (the overview, not a raw-node dump).
    await expect(page.locator("canvas")).toBeVisible();

    // 5. Network is bounded: within caps, and built (not the too-few empty-state).
    const comm = await communitiesResp;
    const body = (await comm.json()) as {
      communities: unknown[];
      edges: unknown[];
      built: boolean;
    };
    expect(body.built).toBe(true);
    expect(body.communities.length).toBeLessThanOrEqual(COMMUNITIES_MAX_NODES);
    expect(body.edges.length).toBeLessThanOrEqual(COMMUNITIES_MAX_EDGES);

    // 6. The overview never fired an unbounded entity/neighborhood dump.
    await page.waitForLoadState("networkidle");
    const dumped = apiCalls.filter((u) =>
      /\/api\/(entities|graph\/node)/.test(u),
    );
    expect(dumped, `unbounded dump fired: ${dumped.join(", ")}`).toEqual([]);

    // 7. No uncaught crash / console error over the bounded render.
    expect(errors).toEqual([]);
  });
});
