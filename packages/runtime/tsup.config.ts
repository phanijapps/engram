import { defineConfig } from "tsup";

export default defineConfig({
  entry: ["src/index.ts", "src/ingest/bin.ts"],
  format: ["esm"],
  dts: true,
  clean: true
});
