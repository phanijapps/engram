// Q&A route — grounds in deterministic retrieval, synthesizes via the pi SDK
// when creds are present. Never throws; missing/failed LLM → evidence summary.

import type { Hono } from "hono";
import { answerQuestion } from "../services/qa.service.js";
import { SCAN_SCOPE } from "../data/scan-defaults.js";

export function registerQaRoutes(app: Hono): void {
  app.post("/qa/ask", async (c) => {
    const { question, scope } = await c.req.json();
    if (!question || typeof question !== "string") return c.json({ error: "question required" }, 400);
    const reqScope = scope ?? SCAN_SCOPE;
    return c.json(await answerQuestion(question, reqScope));
  });
}
