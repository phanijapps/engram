import { serve } from "@hono/node-server";
import { app } from "./app.js";
import { applyEnvDefaults } from "./bootstrap.js";

applyEnvDefaults();

const port = Number(process.env.PORT ?? 8787);

serve({ fetch: app.fetch, port }, (info) => {
  // eslint-disable-next-line no-console
  console.log(`engram prototype backend on http://localhost:${info.port}`);
});
