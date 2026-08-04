//! viz-observatory S4 E2E. The Observatory tab reuses the deck.gl overview canvas
//! and overlays a LearningHealthBar; populated surfaces (graph/memory) show health,
//! unpopulated ones (belief/hierarchy) show honest empty-states.

import { test, expect } from "@playwright/test";

test.describe("viz-observatory S4", () => {
  test("health bar renders graph/memory health + belief/hierarchy empty-states", async ({
    page,
  }) => {
    const errors: string[] = [];
    page.on("pageerror", (e) => errors.push(`pageerror: ${e.message}`));
    page.on("console", (m) => {
      if (m.type() === "error") errors.push(`console.error: ${m.text()}`);
    });

    const stats = page.waitForResponse(
      (r) => r.url().includes("/api/graph/stats") && r.ok(),
    );
    await page.goto("/observatory");
    await stats;

    await expect(page.getByText("LEARNING HEALTH")).toBeVisible({ timeout: 15000 });
    // populated surfaces present; unpopulated surfaces show the populate pointer.
    await expect(page.getByText("Graph").first()).toBeVisible();
    await expect(page.getByText(/Synthesize via reflection/i)).toBeVisible();
    await expect(page.getByText(/Build via hierarchy_build/i)).toBeVisible();

    expect(errors).toEqual([]);
  });
});
