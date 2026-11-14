import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    projects: [
      {
        test: {
          name: "app",
          root: "./app",
          environment: "node",
          include: ["test/**/*.test.ts"],
        },
      },
      {
        test: {
          name: "core",
          root: "./core",
          environment: "node",
          include: ["test/**/*.test.ts"],
        },
      },
    ],
  },
});
