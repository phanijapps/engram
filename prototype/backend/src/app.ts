// HTTP application root — a thin facade.
//
// The backend is a JSON transport over the Rust memory service: v1 JSON in, v1
// JSON out, unchanged by Rust. Behavior lives in services/ and adapters/; this
// file only wires CORS and mounts one router per resource family from http/.
// Add a route by editing the family's router in http/, not here.

import { Hono } from "hono";
import { cors } from "hono/cors";

import { registerBeliefRoutes } from "./http/belief.routes.js";
import { registerBenchRoutes } from "./http/bench.routes.js";
import { registerConsolidationRoutes } from "./http/consolidation.routes.js";
import { registerEvalRoutes } from "./http/eval.routes.js";
import { registerHealthRoutes } from "./http/health.routes.js";
import { registerHierarchyRoutes } from "./http/hierarchy.routes.js";
import { registerIngestRoutes } from "./http/ingest.routes.js";
import { registerKnowledgeRoutes } from "./http/knowledge.routes.js";
import { registerLlmRoutes } from "./http/llm.routes.js";
import { registerMcpRoute } from "./http/mcp.routes.js";
import { registerMemoryRoutes } from "./http/memory.routes.js";
import { registerOntologyRoutes } from "./http/ontology.routes.js";
import { registerQaRoutes } from "./http/qa.routes.js";
import { registerRetrievalRoutes } from "./http/retrieval.routes.js";
import { registerTaxonomyRoutes } from "./http/taxonomy.routes.js";

export const app = new Hono();

// Dev CORS so the Vite frontend (separate origin) can call the API locally.
app.use("*", cors({ origin: ["http://localhost:5173", "http://127.0.0.1:5173"] }));

// One router per resource family.
registerHealthRoutes(app);
registerMemoryRoutes(app);
registerIngestRoutes(app);
registerLlmRoutes(app);
registerRetrievalRoutes(app);
registerKnowledgeRoutes(app);
registerTaxonomyRoutes(app);
registerOntologyRoutes(app);
registerHierarchyRoutes(app);
registerConsolidationRoutes(app);
registerEvalRoutes(app);
registerBeliefRoutes(app);
registerQaRoutes(app);
registerMcpRoute(app);
registerBenchRoutes(app);
