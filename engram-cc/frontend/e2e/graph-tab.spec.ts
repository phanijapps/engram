//! Graph tab E2E — the ACTUAL graph (real symbols + resolved call edges), not
//! the community meta-graph. Gates that the subgraph endpoint answers with a
//! bounded payload, the canvas mounts, and clicking a symbol (via the
//! SymbolGraph e2e hook — force layouts move nodes off any seed) selects it
//! into the entity-detail panel. Render-without-crash, not a fidelity check.

import { test, expect, type Response } from "@playwright/test";

test.describe("viz-graph symbols", () => {
  test("actual graph mounts + subgraph is bounded + click selects", async ({ page }) => {
    const errors: string[] = [];
    page.on("pageerror", (e) => errors.push(`pageerror: ${e.message}`));
    page.on("console", (m) => {
      if (m.type() === "error") errors.push(`console.error: ${m.text()}`);
    });

    const subgraph = page.waitForResponse(
      (r: Response) => r.url().includes("/api/graph/subgraph") && r.ok(),
    );
    await page.goto("/graph");

    // 1. The subgraph endpoint answers with a bounded, resolved payload.
    const body = (await (await subgraph).json()) as {
      nodes: { id: string; name: string; kind: string; degree: number }[];
      edges: { source: string; target: string; predicate: string }[];
      totalNodes: number;
      resolvedOnly: boolean;
    };
    expect(body.nodes.length).toBeGreaterThan(0);
    expect(body.nodes.length).toBeLessThanOrEqual(800); // MAX_SUBGRAPH_NODES
    expect(body.resolvedOnly).toBe(true);
    expect(body.edges.length).toBeGreaterThan(0);

    // 2. A canvas is mounted (the symbol graph renders, not an error state).
    await expect(page.locator("canvas")).toBeVisible({ timeout: 20000 });
    await expect(page.getByText(/symbols · \d+ edges/i)).toBeVisible({ timeout: 20000 });

    // 3. Click a symbol via the live-position hook (force layout ⇒ compute
    //    targets from the screen, not API coords) — the detail panel opens.
    await page.waitForTimeout(2500); // let the simulation settle
    const nodes = await page.evaluate(() => {
      const w = window as unknown as {
        __engramSymbolGraphNodes?: () => { id: string; name: string; x: number; y: number; r: number }[];
      };
      return w.__engramSymbolGraphNodes?.() ?? [];
    });
    expect(nodes.length).toBeGreaterThan(0);
    const target = nodes.reduce((a, b) => (b.r > a.r ? b : a)); // biggest node = highest degree
    const box = await page.locator("canvas").boundingBox();
    expect(box).toBeTruthy();
    await page.mouse.click(box!.x + (target.x - box!.x), box!.y + (target.y - box!.y));

    await expect(page.getByText(/kind|degree/i).first()).toBeVisible({ timeout: 15000 });

    expect(errors).toEqual([]);
  });
});
