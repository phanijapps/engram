import { defineConfig } from "tsup";

export default defineConfig({
  clean: true,
  dts: true,
  entry: ["src/index.ts"],
  external: ["@engram/contracts"],
  format: ["esm"],
  sourcemap: true,
  splitting: false
});
