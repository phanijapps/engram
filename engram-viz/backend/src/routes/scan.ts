//! POST /api/scan — index a repository path into the codegraph.
//!
//! Runs the scan to completion (background Rust thread) and returns the summary.
//! The knowledge cache is invalidated so the next graph read reflects the scan.

import { Hono } from "hono";
import { scanToCompletion } from "../lib/ingest.ts";

export const scanRoute = new Hono();

scanRoute.post("/", async (c) => {
  let body: { path?: string };
  try {
    body = await c.req.json();
  } catch {
    return c.json({ error: "expected JSON body { path }" }, 400);
  }
  const root = body.path;
  if (!root || root.length === 0) {
    return c.json({ error: "missing 'path' in request body" }, 400);
  }

  const state = await scanToCompletion(root);
  if (state.status === "error") {
    return c.json({ error: state.error ?? "scan failed" }, 500);
  }
  return c.json({
    path: root,
    status: state.status,
    summary: state.summary,
  });
});
