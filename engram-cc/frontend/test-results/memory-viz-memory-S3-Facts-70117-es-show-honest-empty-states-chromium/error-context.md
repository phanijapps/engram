# Instructions

- Following Playwright test failed.
- Explain why, be concise, respect Playwright best practices.
- Provide a snippet of code with the fix, if possible.

# Test info

- Name: memory.spec.ts >> viz-memory S3 >> Facts loads memories; empty surfaces show honest empty-states
- Location: e2e/memory.spec.ts:7:3

# Error details

```
Error: expect(received).toBeGreaterThan(expected)

Expected: > 0
Received:   0
```

# Page snapshot

```yaml
- generic [ref=e3]:
  - banner [ref=e4]:
    - link "e engram-cc" [ref=e5] [cursor=pointer]:
      - /url: /observatory
      - generic [ref=e6]: e
      - generic [ref=e7]: engram-cc
    - navigation [ref=e8]:
      - link "Memory" [ref=e9] [cursor=pointer]:
        - /url: /memory
      - link "Observatory" [ref=e21] [cursor=pointer]:
        - /url: /observatory
      - link "Graph" [ref=e28] [cursor=pointer]:
        - /url: /graph
      - link "Ingest" [ref=e34] [cursor=pointer]:
        - /url: /ingest
      - link "Maintain" [ref=e39] [cursor=pointer]:
        - /url: /maintain
      - link "Ask" [ref=e43] [cursor=pointer]:
        - /url: /ask
    - generic [ref=e47]:
      - generic "BFF health" [ref=e48]: agentzero
      - generic "MCP server (:8788)" [ref=e52]: MCP
  - main [ref=e54]:
    - generic [ref=e56]:
      - navigation [ref=e57]:
        - button "Facts" [ref=e58] [cursor=pointer]
        - button "Beliefs" [ref=e59] [cursor=pointer]
        - button "Contradictions" [ref=e60] [cursor=pointer]
        - button "Procedures" [ref=e61] [cursor=pointer]
      - textbox "hybrid search (vector + graph)…" [ref=e67]
      - generic [ref=e68]:
        - generic [ref=e69]: No memories
        - generic [ref=e70]: The memory store is empty for this workspace.
```

# Test source

```ts
  1  | //! viz-memory S3 E2E. The Memory tab loads facts (real memories) and shows honest
  2  | //! empty-states for the unpopulated surfaces (beliefs/contradictions/procedures).
  3  | 
  4  | import { test, expect } from "@playwright/test";
  5  | 
  6  | test.describe("viz-memory S3", () => {
  7  |   test("Facts loads memories; empty surfaces show honest empty-states", async ({
  8  |     page,
  9  |   }) => {
  10 |     const errors: string[] = [];
  11 |     page.on("pageerror", (e) => errors.push(`pageerror: ${e.message}`));
  12 |     page.on("console", (m) => {
  13 |       if (m.type() === "error") errors.push(`console.error: ${m.text()}`);
  14 |     });
  15 | 
  16 |     const mem = page.waitForResponse(
  17 |       (r) => r.url().includes("/api/memory") && r.ok(),
  18 |     );
  19 |     await page.goto("/memory");
  20 |     const body = await (await mem).json();
  21 |     // Facts has real content (not its empty-state).
> 22 |     expect(body.items.length).toBeGreaterThan(0);
     |                               ^ Error: expect(received).toBeGreaterThan(expected)
  23 |     await expect(page.getByText(/^No memories$/)).toHaveCount(0);
  24 | 
  25 |     // Beliefs / Contradictions / Procedures → honest empty-states.
  26 |     const cases: Array<[string, string]> = [
  27 |       ["Beliefs", "No beliefs"],
  28 |       ["Contradictions", "No contradictions"],
  29 |       ["Procedures", "No procedures"],
  30 |     ];
  31 |     for (const [tab, empty] of cases) {
  32 |       await page.getByRole("button", { name: tab }).click();
  33 |       await expect(page.getByText(empty)).toBeVisible();
  34 |     }
  35 | 
  36 |     expect(errors).toEqual([]);
  37 |   });
  38 | });
  39 | 
```