import { afterEach, describe, expect, it } from "vitest";
import { extractJsonObject, getLLMConfig, parseLLMGraph } from "./llm.js";

describe("parseLLMGraph", () => {
  it("parses a well-formed graph", () => {
    const out = parseLLMGraph({
      entities: [
        { name: "write", kind: "function" },
        { name: "Store", kind: "class" },
      ],
      relationships: [{ subject: "write", predicate: "calls", object: "Store" }],
    });
    expect(out.entities).toHaveLength(2);
    expect(out.relationships).toEqual([{ subject: "write", predicate: "calls", object: "Store" }]);
  });

  it("maps an unknown/disallowed kind to 'unknown'", () => {
    const out = parseLLMGraph({ entities: [{ name: "X", kind: "widget" }], relationships: [] });
    expect(out.entities[0].kind).toBe("unknown");
  });

  it("dedupes entities by name (case-insensitive) and trims names", () => {
    const out = parseLLMGraph({
      entities: [{ name: "  Foo " }, { name: "foo", kind: "concept" }],
      relationships: [],
    });
    expect(out.entities).toHaveLength(1);
    expect(out.entities[0].name).toBe("Foo");
  });

  it("drops relationships whose endpoints are not extracted entities", () => {
    const out = parseLLMGraph({
      entities: [{ name: "A", kind: "concept" }],
      relationships: [
        { subject: "A", predicate: "mentions", object: "B" }, // B unknown → drop
        { subject: "A", predicate: "relates_to", object: "A" },
      ],
    });
    expect(out.relationships).toEqual([{ subject: "A", predicate: "relates_to", object: "A" }]);
  });

  it("throws on a non-object top-level", () => {
    expect(() => parseLLMGraph(null)).toThrow();
    expect(() => parseLLMGraph("not an object")).toThrow();
  });

  it("treats missing arrays as empty rather than throwing", () => {
    const out = parseLLMGraph({});
    expect(out.entities).toEqual([]);
    expect(out.relationships).toEqual([]);
  });

  it("drops entities with over-long names and predicates", () => {
    const longName = "X".repeat(500);
    const longPred = "p".repeat(200);
    const out = parseLLMGraph({
      entities: [{ name: longName, kind: "concept" }, { name: "ok", kind: "concept" }],
      relationships: [
        { subject: "ok", predicate: longPred, object: "ok" },
        { subject: "ok", predicate: "relates_to", object: "ok" },
      ],
    });
    expect(out.entities.map((e) => e.name)).toEqual(["ok"]);
    expect(out.relationships.map((r) => r.predicate)).toEqual(["relates_to"]);
  });
});

describe("extractJsonObject", () => {
  it("ignores braces inside JSON string values", () => {
    const raw = `prose {"entities":[{"name":"a}b"}],"relationships":[]} trailing`;
    const out = JSON.parse(extractJsonObject(raw));
    expect(out.entities[0].name).toBe("a}b");
  });

  it("handles escaped quotes inside strings", () => {
    const raw = `{"name":"a\\"b"}`
    expect(JSON.parse(extractJsonObject(raw)).name).toBe('a"b');
  });

  it("skips surrounding prose and returns the object", () => {
    const raw = `Here is the graph: {"entities":[],"relationships":[]} done.`;
    expect(JSON.parse(extractJsonObject(raw)).entities).toEqual([]);
  });

  it("throws when there is no object", () => {
    expect(() => extractJsonObject("no json here")).toThrow();
  });

  it("throws on an unterminated object", () => {
    expect(() => extractJsonObject('{"a":{"b":1}')).toThrow();
  });
});

describe("getLLMConfig", () => {
  const backup = { ...process.env };
  afterEach(() => {
    process.env = { ...backup };
  });

  it("returns null when credentials are absent", () => {
    delete process.env.ENGRAM_LLM_BASE_URL;
    delete process.env.ENGRAM_LLM_API_KEY;
    delete process.env.ENGRAM_LLM_MODEL;
    expect(getLLMConfig()).toBeNull();
  });

  it("returns null when the key is the placeholder", () => {
    process.env.ENGRAM_LLM_BASE_URL = "https://api.example.com/v1";
    process.env.ENGRAM_LLM_API_KEY = "replace-me";
    process.env.ENGRAM_LLM_MODEL = "gemma4:31b-cloud";
    expect(getLLMConfig()).toBeNull();
  });

  it("returns a trimmed config when all three are set", () => {
    process.env.ENGRAM_LLM_BASE_URL = "https://api.example.com/v1/";
    process.env.ENGRAM_LLM_API_KEY = "sk-real";
    process.env.ENGRAM_LLM_MODEL = "gemma4:31b-cloud";
    expect(getLLMConfig()).toEqual({
      baseUrl: "https://api.example.com/v1",
      apiKey: "sk-real",
      model: "gemma4:31b-cloud",
    });
  });
});
