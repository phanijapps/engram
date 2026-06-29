import { describe, expect, it } from "vitest";

import { engramV1Definitions, getContractDefinition } from "../src/index.js";

describe("@engram/contracts", () => {
  it("exports required v1 definitions", () => {
    expect(Object.keys(engramV1Definitions)).toContain("MemoryRecord");
    expect(Object.keys(engramV1Definitions)).toContain("RetrievalRequest");
    expect(Object.keys(engramV1Definitions)).toContain("WriteMemoryRequest");
  });

  it("returns a selected schema definition", () => {
    expect(getContractDefinition("MemoryRecord")).toBe(engramV1Definitions.MemoryRecord);
  });
});
