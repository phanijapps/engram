//! Ingest tab E2E — gates that the tab mounts (form + KIND toggle + counts panel)
//! and renders without an uncaught crash. Does NOT start a real scan: that would
//! mutate the agentzero store + load the native binding. The scan lifecycle
//! (running → done/error, JSON parse, validation) is unit-tested in the backend
//! (`ingest.routes.test.ts` with a fake spawner). Here: render + nav + counts.

import { test, expect } from "@playwright/test";

test.describe("engram-cc ingest", () => {
  test("Ingest tab mounts + renders the form + store counts", async ({ page }) => {
    const errors: string[] = [];
    page.on("pageerror", (e) => errors.push(`pageerror: ${e.message}`));
    page.on("console", (m) => {
      if (m.type() === "error") errors.push(`console.error: ${m.text()}`);
    });

    await page.goto("/ingest");
    await expect(page.getByText(/^INGEST$/)).toBeVisible({ timeout: 20_000 });
    // path input + KIND toggle + Start button
    await expect(page.getByPlaceholder(/absolute\/path/i)).toBeVisible();
    await expect(page.getByRole("button", { name: "auto" })).toBeVisible();
    await expect(page.getByRole("button", { name: "code" })).toBeVisible();
    await expect(page.getByRole("button", { name: "doc" })).toBeVisible();
    await expect(page.getByRole("button", { name: /start scan/i })).toBeVisible();
    // counts panel renders once /ingest/counts resolves (BFF over agentzero)
    await expect(page.getByText(/STORE COUNTS/i)).toBeVisible({ timeout: 20_000 });

    expect(errors).toEqual([]);
  });
});
