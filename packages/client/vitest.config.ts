import { defineConfig } from "vitest/config";

export default defineConfig({
  resolve: {
    alias: {
      "@engram/contracts": new URL("../contracts/src/index.ts", import.meta.url).pathname,
      "@engram/node": new URL("../node/src/index.ts", import.meta.url).pathname
    }
  },
  test: {
    globals: false
  }
});
