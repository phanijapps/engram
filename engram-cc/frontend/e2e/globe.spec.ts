//! Graph tab E2E — the animated 3D globe. Gates that it mounts (canvas + legend)
//! and renders without an uncaught crash. (Headless Chrome uses software WebGL;
//! this is render-without-crash, not a fidelity check — the globe is verified
//! visually on real hardware.)

import { test, expect } from "@playwright/test";

test.describe("viz-graph globe", () => {
  test("animated 3D globe mounts + renders without crash", async ({ page }) => {
    const errors: string[] = [];
    page.on("pageerror", (e) => errors.push(`pageerror: ${e.message}`));
    page.on("console", (m) => {
      if (m.type() === "error") errors.push(`console.error: ${m.text()}`);
    });

    await page.goto("/graph");
    // frosted HUD eyebrow + the orbit hint + a WebGL canvas mount
    await expect(page.getByText(/engram · graph/i)).toBeVisible({ timeout: 20000 });
    await expect(page.getByText(/drag to orbit/i)).toBeVisible();
    await expect(page.locator("canvas")).toBeVisible();

    expect(errors).toEqual([]);
  });
});
