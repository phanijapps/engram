#!/usr/bin/env node
// Removes generated JS/native build outputs while preserving dependencies,
// Cargo artifacts, generated contracts, and local demo databases.

import { rm } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { dirname } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = join(here, "..", "..");

const paths = [
  "demo/backend/dist",
  "demo/frontend/dist",
  "packages/adapters/dist",
  "packages/client/dist",
  "packages/contracts/dist",
  "packages/node/dist",
  "packages/node/engram_node.node"
];

for (const relativePath of paths) {
  const target = join(repoRoot, relativePath);
  await rm(target, { recursive: true, force: true });
  console.log(`removed ${relativePath}`);
}
