//! viz-memory S3 E2E. The Memory tab loads facts (real memories) and shows honest
//! empty-states for the unpopulated surfaces (beliefs/contradictions/procedures).

import { test, expect } from "@playwright/test";

test.describe("viz-memory S3", () => {
  test("Facts loads memories; empty surfaces show honest empty-states", async ({
    page,
  }) => {
    const errors: string[] = [];
    page.on("pageerror", (e) => errors.push(`pageerror: ${e.message}`));
    page.on("console", (m) => {
      if (m.type() === "error") errors.push(`console.error: ${m.text()}`);
    });

    const mem = page.waitForResponse(
      (r) => r.url().includes("/api/memory") && r.ok(),
    );
    await page.goto("/memory");
    const body = await (await mem).json();
    // Facts has real content (not its empty-state).
    expect(body.items.length).toBeGreaterThan(0);
    await expect(page.getByText(/^No memories$/)).toHaveCount(0);

    // Beliefs / Contradictions / Procedures → honest empty-states.
    const cases: Array<[string, string]> = [
      ["Beliefs", "No beliefs"],
      ["Contradictions", "No contradictions"],
      ["Procedures", "No procedures"],
    ];
    for (const [tab, empty] of cases) {
      await page.getByRole("button", { name: tab }).click();
      await expect(page.getByText(empty)).toBeVisible();
    }

    expect(errors).toEqual([]);
  });
});
