import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["test/**/*.test.ts"],
    // The native provider opens a SQLite store; give it room.
    testTimeout: 30000,
  },
});
