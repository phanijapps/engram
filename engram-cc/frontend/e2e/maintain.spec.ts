//! Maintain tab E2E — gates that the tab mounts (op selector + Run + panels)
//! and renders without an uncaught crash. Does NOT run a real op (LLM ops route
//! scope data to a cloud model + need a key; consolidate mutates state). The
//! run-lifecycle is unit-tested in the backend (maintain.routes.test.ts).

import { test, expect } from "@playwright/test";

test.describe("engram-cc maintain", () => {
  test("Maintain tab mounts + renders the op selector + panels", async ({ page }) => {
    const errors: string[] = [];
    page.on("pageerror", (e) => errors.push(`pageerror: ${e.message}`));
    page.on("console", (m) => {
      if (m.type() === "error") errors.push(`console.error: ${m.text()}`);
    });

    await page.goto("/maintain");
    await expect(page.getByText(/^MAINTAIN$/)).toBeVisible({ timeout: 20_000 });
    // op buttons + Run
    await expect(page.getByRole("button", { name: "Reflect" })).toBeVisible();
    await expect(page.getByRole("button", { name: "Contradict" })).toBeVisible();
    await expect(page.getByRole("button", { name: "Consolidate" })).toBeVisible();
    await expect(page.getByRole("button", { name: "Run" })).toBeVisible();
    // panels render
    await expect(page.getByText(/^BELIEFS/)).toBeVisible();
    await expect(page.getByText(/^CONTRADICTIONS/)).toBeVisible();

    expect(errors).toEqual([]);
  });
});
