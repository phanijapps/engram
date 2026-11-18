#!/usr/bin/env node
// Publish through the same locked, named-toolchain, immutable recipe builder.
import { ensureRecipeArtifacts } from "../test/helpers/build-native-recipes.mjs";
import { copyFileSync, constants, renameSync } from "node:fs";
import { randomUUID } from "node:crypto";
import { fileURLToPath } from "node:url";
const recipe = process.argv[2] ?? "sqlite";
const artifacts = ensureRecipeArtifacts({ profile: "release", recipes: [recipe], extraFeatures: recipe === "sqlite" ? ["fastembed"] : [] });
const destination = fileURLToPath(new URL("../engram_node.node", import.meta.url));
const pending = `${destination}.${randomUUID()}.pending`;
copyFileSync(artifacts[recipe], pending, constants.COPYFILE_EXCL);
renameSync(pending, destination);
console.log(`selected ${recipe} artifact SHA256=${artifacts.digests[recipe]}`);
