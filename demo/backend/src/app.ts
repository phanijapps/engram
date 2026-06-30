import { Hono } from "hono";
import { cors } from "hono/cors";
import { getTransport } from "./engram.js";

// The backend is a thin JSON transport over the Rust memory service. It owns no
// behavior — v1 JSON in, v1 JSON out, unchanged by Rust — so TypeScript stays
// ergonomic and Rust stays the single source of truth.
export const app = new Hono();

// Dev CORS so the Vite frontend (separate origin) can call the API locally.
app.use(
  "*",
  cors({ origin: ["http://localhost:5173", "http://127.0.0.1:5173"] })
);

app.get("/health", (c) => c.json({ status: "ok" }));

app.post("/memory/write", async (c) => {
  const request = await c.req.json();
  const response = await getTransport().writeMemory(request);
  return c.json(response);
});

app.post("/memory/retrieve", async (c) => {
  const request = await c.req.json();
  const response = await getTransport().retrieve(request);
  return c.json(response);
});

app.post("/memory/forget", async (c) => {
  const request = await c.req.json();
  const response = await getTransport().forget(request);
  return c.json(response);
});
