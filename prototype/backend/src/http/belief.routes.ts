// Belief + contradiction routes. Durable storage in the belief adapter (distinct
// from knowledge + memory). Detection is advisory; resolution is an action.

import type { Hono } from "hono";
import { getBeliefTransport } from "../adapters/engram.client.js";

export function registerBeliefRoutes(app: Hono): void {
  app.post("/belief/put", async (c) => {
    const request = await c.req.json();
    return c.json(await getBeliefTransport().putBelief(request));
  });
  app.post("/belief/list", async (c) => {
    const { scope } = await c.req.json();
    return c.json(await getBeliefTransport().listBeliefs(scope));
  });
  app.post("/belief/contradiction", async (c) => {
    const request = await c.req.json();
    return c.json(await getBeliefTransport().putContradiction(request));
  });
  app.post("/belief/contradictions", async (c) => {
    const { scope } = await c.req.json();
    return c.json(await getBeliefTransport().listContradictions(scope));
  });
  app.post("/belief/get", async (c) => {
    const { id, scope } = await c.req.json();
    return c.json(await getBeliefTransport().getContradiction(id, scope));
  });
  app.post("/belief/resolve", async (c) => {
    const { id, scope, resolution } = await c.req.json();
    return c.json(await getBeliefTransport().resolveContradiction(id, scope, resolution));
  });
  // Runs advisory detection over the beliefs visible to `scope`, persists each
  // detected contradiction (so it appears in the review queue), and returns them.
  app.post("/belief/detect", async (c) => {
    const { scope } = await c.req.json();
    const transport = getBeliefTransport();
    const beliefs = await transport.listBeliefs(scope);
    const detected = (await transport.detectContradictions(beliefs)) as unknown[];
    for (const contradiction of detected) {
      await transport.putContradiction(contradiction);
    }
    return c.json({ beliefs: (beliefs as unknown[]).length, contradictions: detected });
  });
}
