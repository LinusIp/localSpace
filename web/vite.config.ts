import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";
import { resolve } from "node:path";

// In development Vite serves the client and proxies the API to a running
// `localspace serve`; in production the server serves `dist/` itself. The
// in-house packages resolve to their sources (architecture v2.1 §6.1).
export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@localspace/ui": resolve(__dirname, "packages/ui/src/index.ts"),
      "@localspace/canvas": resolve(__dirname, "packages/canvas/src/index.ts"),
    },
  },
  server: {
    proxy: {
      "/api": "http://127.0.0.1:8443",
      "/ws": { target: "ws://127.0.0.1:8443", ws: true },
    },
  },
  build: { sourcemap: false },
});
