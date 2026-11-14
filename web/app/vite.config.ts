import { defineConfig } from "vite";

export default defineConfig({
  build: { outDir: "dist", assetsDir: "assets", emptyOutDir: true, assetsInlineLimit: 0 },
  server: {
    proxy: { "/ws": { target: "ws://127.0.0.1:20481", ws: true } },
  },
});
