import { defineConfig } from "tsup";

export default defineConfig({
  entry: ["src/index.ts", "src/ingest/bin.ts"],
  format: ["esm"],
  dts: true,
  clean: true,
  // Keep workspace deps external (mirrors @engram/client/@engram/node) so the
  // bin resolves @engram/node (and its native addon) from node_modules at runtime
  // instead of inlining a frozen copy into dist.
  external: ["@engram/contracts", "@engram/node"]
});
