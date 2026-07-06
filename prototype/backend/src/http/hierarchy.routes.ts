// Hierarchy routes — parentage validation for node trees.

import type { Hono } from "hono";
import { getHierarchyTransport } from "../adapters/engram.client.js";

export function registerHierarchyRoutes(app: Hono): void {
  app.post("/hierarchy/validate-parentage", async (c) => {
    const { nodes } = await c.req.json();
    return c.json(await getHierarchyTransport().validateParentage(nodes ?? []));
  });
}
