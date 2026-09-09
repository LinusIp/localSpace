import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vite";

// In development Vite serves the client and proxies the API to a running
// `localspace serve`; in production the server serves `dist/` itself.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    proxy: {
      "/api": "http://127.0.0.1:8443",
      "/ws": { target: "ws://127.0.0.1:8443", ws: true },
    },
  },
  build: { sourcemap: false },
});
