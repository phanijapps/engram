import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Dev proxy: the frontend calls relative /memory/* and /health, and Vite
// forwards them to the demo backend so the UI is a pure client (no CORS in dev).
export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: {
      "/memory": "http://localhost:8787",
      "/knowledge": "http://localhost:8787",
      "/taxonomy": "http://localhost:8787",
      "/health": "http://localhost:8787"
    }
  }
});
