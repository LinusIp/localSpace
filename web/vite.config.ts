import react from "@vitejs/plugin-react";
import { defineConfig, type Plugin } from "vite";
import { resolve } from "node:path";

// In development Vite serves the client and proxies the API to a running
// `localspace serve`; in production the server serves `dist/` itself. The
// in-house packages resolve to their sources (architecture v2.1 §6.1).

/**
 * KaTeX's stylesheet names each font three times, woff2, woff and ttf; every
 * browser the shell runs in reads woff2, so only those are shipped
 * (docs/DECISIONS.md, 2026-10-10).
 */
function katexWoff2Only(): Plugin {
  return {
    name: "katex-woff2-only",
    enforce: "pre",
    transform(code, id) {
      if (!/[\\/]katex[\\/]dist[\\/]katex(\.min)?\.css(\?.*)?$/.test(id)) return null;
      return code.replace(/,\s*url\([^)]*\.woff\)\s*format\("woff"\)/g, "").replace(/,\s*url\([^)]*\.ttf\)\s*format\("truetype"\)/g, "");
    },
  };
}

export default defineConfig({
  plugins: [react(), katexWoff2Only()],
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
