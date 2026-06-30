// LLM relationship extraction (RFC 0004 Slice 2).
//
// Drives ollama cloud (OpenAI-compatible) through the pi SDK
// (`@earendil-works/pi-coding-agent`): ollama cloud is registered as a custom
// provider/model in a process-local `models.json` generated from the demo's
// `.env` (base_url / api_key / model), and the model is run headless via
// `createAgentSession` + `prompt` with no tools. The deterministic
// GraphExtractor is always the baseline; this module adds an optional "enhance"
// layer. Output is validated before it is allowed near the graph; calls are
// bounded; the key stays server-side and is never written to disk (the
// models.json references it via `$ENGRAM_LLM_API_KEY`, resolved by pi at request
// time from the environment).

import { promises as fs } from "node:fs";
import os from "node:os";
import path from "node:path";

const ENTITY_KINDS = new Set([
  "person", "organization", "project", "repository", "file", "module", "class",
  "function", "method", "variable", "api", "concept", "task", "tool", "artifact",
  "unknown",
]);

export type ParsedEntity = { name: string; kind: string };
export type ParsedRelationship = { subject: string; predicate: string; object: string };
export type ParsedGraph = { entities: ParsedEntity[]; relationships: ParsedRelationship[] };
export type LLMConfig = { baseUrl: string; apiKey: string; model: string };

const MAX_ENTITIES = 200;
const MAX_RELATIONSHIPS = 400;
const MAX_NAME_CHARS = 256;
const MAX_PREDICATE_CHARS = 128;
const TIMEOUT_MS = 30_000;
const MAX_RESPONSE_CHARS = 100_000;
const MAX_INPUT_CHARS = 8_000;
const PLACEHOLDER_KEY = "replace-me";

/** Reads the LLM config from the environment, or null if not configured. */
export function getLLMConfig(): LLMConfig | null {
  const baseUrl = process.env.ENGRAM_LLM_BASE_URL;
  const apiKey = process.env.ENGRAM_LLM_API_KEY;
  const model = process.env.ENGRAM_LLM_MODEL;
  if (baseUrl && apiKey && model && apiKey !== PLACEHOLDER_KEY) {
    return { baseUrl: baseUrl.replace(/\/+$/, ""), apiKey, model };
  }
  return null;
}

function asString(value: unknown): string | null {
  return typeof value === "string" ? value : null;
}

/** Pure: parse + validate LLM graph output. Throws on a malformed top-level. */
export function parseLLMGraph(raw: unknown): ParsedGraph {
  if (typeof raw !== "object" || raw === null) {
    throw new Error("LLM output is not a JSON object");
  }
  const obj = raw as Record<string, unknown>;
  const rawEntities = Array.isArray(obj.entities) ? obj.entities : [];
  const rawRelationships = Array.isArray(obj.relationships) ? obj.relationships : [];

  const entities: ParsedEntity[] = [];
  const seen = new Set<string>();
  for (const entry of rawEntities) {
    if (typeof entry !== "object" || entry === null) continue;
    const name = asString((entry as Record<string, unknown>).name)?.trim();
    if (!name || name.length > MAX_NAME_CHARS) continue;
    const key = name.toLowerCase();
    if (seen.has(key)) continue;
    seen.add(key);
    const rawKind = asString((entry as Record<string, unknown>).kind)?.trim().toLowerCase();
    const kind = rawKind && ENTITY_KINDS.has(rawKind) ? rawKind : "unknown";
    entities.push({ name, kind });
    if (entities.length >= MAX_ENTITIES) break;
  }

  const nameSet = new Set(entities.map((e) => e.name.toLowerCase()));
  const relationships: ParsedRelationship[] = [];
  for (const rel of rawRelationships) {
    if (typeof rel !== "object" || rel === null) continue;
    const rec = rel as Record<string, unknown>;
    const subject = asString(rec.subject)?.trim();
    const predicate = asString(rec.predicate)?.trim();
    const object = asString(rec.object)?.trim();
    if (!subject || !predicate || !object) continue;
    if (predicate.length > MAX_PREDICATE_CHARS) continue;
    if (!nameSet.has(subject.toLowerCase()) || !nameSet.has(object.toLowerCase())) continue;
    relationships.push({ subject, predicate, object });
    if (relationships.length >= MAX_RELATIONSHIPS) break;
  }

  return { entities, relationships };
}

/**
 * Pulls the first balanced {...} object out of a possibly-noisy response.
 * String-aware: braces inside JSON string literals do not affect depth, so an
 * entity name like `"a}b"` does not truncate the extracted object.
 */
export function extractJsonObject(text: string): string {
  const start = text.indexOf("{");
  if (start === -1) throw new Error("no JSON object in LLM response");
  let depth = 0;
  let inString = false;
  let escaped = false;
  for (let i = start; i < text.length; i++) {
    const ch = text[i];
    if (inString) {
      if (escaped) escaped = false;
      else if (ch === "\\") escaped = true;
      else if (ch === '"') inString = false;
      continue;
    }
    if (ch === '"') inString = true;
    else if (ch === "{") depth++;
    else if (ch === "}") {
      depth--;
      if (depth === 0) return text.slice(start, i + 1);
    }
  }
  throw new Error("unterminated JSON object in LLM response");
}

/** Strips the configured API key from any message so it never leaks in errors. */
function redactKey(value: string): string {
  const key = process.env.ENGRAM_LLM_API_KEY;
  return key ? value.split(key).join("[redacted]") : value;
}

function extractionPrompt(kind: "code" | "text"): string {
  const kinds = [...ENTITY_KINDS].join(", ");
  const focus =
    kind === "code"
      ? "Focus on functions, methods, classes, modules, files, and call/define/contain relationships."
      : "Focus on concepts, people, organizations, projects, and mentions/relates-to relationships.";
  return `You extract a knowledge graph from a document. Return ONLY a JSON object (no prose) of shape {"entities":[{"name":string,"kind":string}],"relationships":[{"subject":string,"predicate":string,"object":string}]}. Valid entity kinds: ${kinds}. ${focus} Every relationship subject/object MUST exactly match an entity name. Predicates are short verb phrases.`;
}

let modelsJsonCache: string | null = null;

/**
 * Writes a process-local pi `models.json` describing ollama cloud as a custom
 * OpenAI-compatible provider, generated from the demo `.env`. The API key is
 * referenced (not embedded) via `$ENGRAM_LLM_API_KEY`; pi resolves it from the
 * environment at request time. No secret is written to disk.
 */
async function ensureModelsJson(config: LLMConfig): Promise<string> {
  if (modelsJsonCache) return modelsJsonCache;
  const file = path.join(os.tmpdir(), `engram-pi-models-${process.pid}.json`);
  const doc = {
    providers: {
      "ollama-cloud": {
        baseUrl: config.baseUrl,
        api: "openai-completions",
        apiKey: "$ENGRAM_LLM_API_KEY",
        authHeader: true,
        compat: { supportsDeveloperRole: false, supportsReasoningEffort: false },
        models: [{ id: config.model, input: ["text"] }],
      },
    },
  };
  await fs.writeFile(file, JSON.stringify(doc), { mode: 0o600 });
  modelsJsonCache = file;
  return file;
}

/**
 * Runs ollama cloud via the pi SDK and returns a validated parsed graph.
 * Throws on any failure (no creds, timeout, malformed output). The caller must
 * already have gated on `getLLMConfig()` and fall back to deterministic output
 * on error. The SDK is imported lazily so the zero-credential path and the unit
 * tests never load the agent harness.
 */
export async function extractGraph(
  text: string,
  kind: "code" | "text",
  config: LLMConfig
): Promise<ParsedGraph> {
  const modelsJsonPath = await ensureModelsJson(config);
  const pi = await import("@earendil-works/pi-coding-agent");
  const authStorage = pi.AuthStorage.inMemory();
  const modelRegistry = pi.ModelRegistry.create(authStorage, modelsJsonPath);
  const model = modelRegistry.find("ollama-cloud", config.model);
  if (!model) throw new Error(`pi model not configured: ollama-cloud/${config.model}`);

  const { session } = await pi.createAgentSession({
    model,
    authStorage,
    modelRegistry,
    sessionManager: pi.SessionManager.inMemory(),
    noTools: "all",
    cwd: os.tmpdir(),
  });

  let collected = "";
  let capped = false;
  // Stop accumulating + abort once the streamed text exceeds the cap, so an
  // adversarial or runaway response cannot grow memory without bound. The guard
  // at the top also drops any deltas that race in after the abort.
  const stop = () => {
    if (capped) return;
    capped = true;
    void session.abort().catch(() => {});
  };
  const off = session.subscribe((event) => {
    if (capped) return;
    if (
      event.type === "message_update" &&
      event.assistantMessageEvent.type === "text_delta"
    ) {
      collected += event.assistantMessageEvent.delta;
      if (collected.length > MAX_RESPONSE_CHARS) stop();
    }
  });

  const timer = setTimeout(stop, TIMEOUT_MS);

  let promptError: unknown = null;
  try {
    const truncated = text.length > MAX_INPUT_CHARS ? text.slice(0, MAX_INPUT_CHARS) : text;
    const prompt = `${extractionPrompt(kind)}\n\nDocument:\n"""\n${truncated}\n"""`;
    await session.prompt(prompt);
  } catch (err) {
    // abort (timeout / response cap) or provider error: fall through with
    // whatever text was streamed so far and let validation decide. Preserve the
    // error (redacted) so a misconfigured key is diagnosable instead of silent.
    promptError = err;
  } finally {
    clearTimeout(timer);
    off();
    session.dispose();
  }

  if (collected.length > MAX_RESPONSE_CHARS) collected = collected.slice(0, MAX_RESPONSE_CHARS);
  if (!collected.includes("{")) {
    const detail =
      promptError instanceof Error ? redactKey(promptError.message) : promptError ? redactKey(String(promptError)) : "no content";
    throw new Error(`LLM returned no content: ${detail}`);
  }
  return parseLLMGraph(JSON.parse(extractJsonObject(collected)));
}
