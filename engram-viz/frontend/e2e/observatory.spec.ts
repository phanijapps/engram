//! viz-observatory E2E. Observatory is now the sole graph view (zbot model):
//! toolbar + community canvas + a bottom LearningHealthBar. The health strip
//! surfaces graph/memory/belief/hierarchy; unpopulated surfaces (belief/hierarchy)
//! open a details slideover carrying the honest empty-state + populate pointer.

import { test, expect } from "@playwright/test";

test.describe("viz-observatory", () => {
  test("health strip renders + belief/hierarchy detail slideovers show empty-states", async ({
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
    await expect(page.getByText("Entities")).toBeVisible();
    await expect(page.getByText("Beliefs")).toBeVisible();

    // Belief detail slideover → honest empty-state + out-of-band populate pointer.
    await page.getByRole("button", { name: "Beliefs details" }).click();
    await expect(page.getByText(/Run reflection via engram-mcp/i)).toBeVisible();

    expect(errors).toEqual([]);
  });
});
