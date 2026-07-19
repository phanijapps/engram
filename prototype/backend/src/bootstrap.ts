// Shared process-level defaults for backend entry points (HTTP server + stdio
// MCP server). Kept here so every entry applies the same environment without
// duplicating the logic.

export function applyEnvDefaults(): void {
  // Load local .env (LLM base_url/api_key/model) if present. Node >=21.7.
  // Missing file is fine — the backend runs deterministic-only without LLM
  // creds.
  try {
    process.loadEnvFile();
  } catch {
    // no .env present — continue with whatever environment is set
  }

  // Durable, shared SQLite: memory, knowledge, and ingest engines all open
  // this file so state persists across restarts and graph data extracted by
  // ingest is visible to the knowledge engine. Delete the file to reset.
  process.env.ENGRAM_DB ??= "demo-engram.db";
}
