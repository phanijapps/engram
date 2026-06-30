import path from "node:path";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// Dev proxy: the frontend calls relative routes and Vite forwards them to the
// demo backend so the UI is a pure client (no CORS in dev).
const BACKEND = "http://localhost:8787";
const proxied = [
  "/memory",
  "/knowledge",
  "/taxonomy",
  "/ontology",
  "/belief",
  "/qa",
  "/ingest",
  "/llm",
  "/retrieval",
  "/health",
];
const proxy: Record<string, string> = {};
for (const route of proxied) proxy[route] = BACKEND;

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  server: {
    port: 5173,
    proxy,
  },
});
