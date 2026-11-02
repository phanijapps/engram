/**
 * TASK-009 focused proofs (GATE-E, AC-001): the inventory view renders one
 * row per partition with memory/entity/belief counts, "(unset)" labels, the
 * org/project alias header (DEC-002), a bounded summary, and the named
 * empty/degraded states. Markup is asserted via react-dom/server — the view
 * is a pure function of its props.
 */
import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import {
  InventoryView,
  InventorySummary,
  AliasHeader,
  dimensionLabel
} from "../src/views/Inventory";
import type { InventoryResponse, PartitionSummary } from "../src/api";

function partition(overrides: Partial<PartitionSummary> = {}): PartitionSummary {
  return {
    scope: { tenant: "acme", subject: null, workspace: "proj", session: null, environment: null },
    memoryCount: 2,
    entityCount: 3,
    beliefCount: 5,
    ...overrides
  };
}

describe("job012 inventory view (STEP-008)", () => {
  it("inventory: renders one row per partition with the count trio", () => {
    const rows = [
      partition(),
      partition({ scope: { tenant: "other", subject: null, workspace: null, session: null, environment: null } })
    ];
    const markup = renderToStaticMarkup(
      <InventoryView inventory={{ state: "ready", partitions: rows, partitionCount: 2 }} onSelect={() => {}} />
    );
    expect(markup).toContain("acme");
    expect(markup).toContain("other");
    expect(markup).toMatch(/memories/);
    expect((markup.match(/tabular-nums/g) ?? []).length).toBeGreaterThanOrEqual(6);
  });

  it("inventory: unset dimensions render as '(unset)', never merged", () => {
    const row = partition({ scope: { tenant: "acme", subject: null, workspace: null, session: null, environment: null } });
    const markup = renderToStaticMarkup(
      <InventoryView inventory={{ state: "ready", partitions: [row], partitionCount: 1 }} onSelect={() => {}} />
    );
    expect(dimensionLabel(null)).toBe("(unset)");
    expect(dimensionLabel(undefined)).toBe("(unset)");
    expect(dimensionLabel("proj")).toBe("proj");
    // subject + workspace + session + environment all unset → four labels.
    expect((markup.match(/\(unset\)/g) ?? []).length).toBe(4);
  });

  it("inventory: empty store renders the named empty state with guidance", () => {
    const markup = renderToStaticMarkup(
      <InventoryView
        inventory={{ state: "ready", partitions: [], partitionCount: 0 }}
        onSelect={() => {}}
      />
    );
    expect(markup).toContain("Empty store");
    expect(markup).toMatch(/No partitions are present/);
  });

  it("inventory: degraded states render by name, never a partial table", () => {
    const degraded: InventoryResponse = {
      state: "backend-unreachable",
      partitions: [],
      message: "The configured backend is unreachable."
    };
    const markup = renderToStaticMarkup(<InventoryView inventory={degraded} onSelect={() => {}} />);
    expect(markup).toContain("State: backend-unreachable");
    expect(markup).not.toContain("<table");
  });

  it("inventory: the bounded summary aggregates without dumping records", () => {
    const markup = renderToStaticMarkup(
      <InventorySummary partitions={[partition(), partition({ memoryCount: 1, entityCount: 0, beliefCount: 0 })]} />
    );
    expect(markup).toContain("2 partitions");
    expect(markup).toContain("3 memories");
    expect(markup).toContain("3 entities");
    expect(markup).toContain("5 beliefs");
  });

  it("inventory: the org/project alias header states the DEC-002 mapping", () => {
    const markup = renderToStaticMarkup(<AliasHeader />);
    expect(markup).toContain("--org");
    expect(markup).toContain("tenant");
    expect(markup).toContain("--project");
    expect(markup).toContain("workspace");
  });
});
