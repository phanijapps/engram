import { serve } from "@hono/node-server";
import { app } from "./app.js";

// Load demo-local .env (LLM base_url/api_key/model) if present. Node >=21.7.
// Missing file is fine — the demo runs deterministic-only without LLM creds.
try {
  process.loadEnvFile();
} catch {
  // no .env present — continue with whatever environment is set
}

// Durable, shared SQLite for the demo: memory, knowledge, and ingest engines
// all open this file so state persists across restarts and graph data extracted
// by ingest is visible to the knowledge engine. Delete the file to reset.
process.env.ENGRAM_DB ??= "demo-engram.db";

const port = Number(process.env.PORT ?? 8787);

serve({ fetch: app.fetch, port }, (info) => {
  // eslint-disable-next-line no-console
  console.log(`engram demo backend on http://localhost:${info.port}`);
});
