// The whiteboard's web surface, built into the package (architecture v2.1
// §6.3, §13 step 5). The output is what the harness's origin serves:
// `index.js` as the entry module the shell's generated page loads, assets
// under `assets/`. Everything is addressed relatively, because the page
// lives under a grant path (`/s/<token>/`) the build cannot know. The SDK,
// React, the canvas, the UI library and Automerge stay bare imports, which
// the page's import map resolves to the one copy the shell provides.

import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { resolve } from "node:path";

const here = resolve(__dirname);

export default defineConfig({
  root: here,
  plugins: [react()],
  base: "./",
  define: { "process.env.NODE_ENV": JSON.stringify("production") },
  build: {
    outDir: resolve(here, "../../../harnesses/whiteboard/ui/web"),
    emptyOutDir: true,
    target: "es2022",
    sourcemap: false,
    assetsInlineLimit: 0,
    rollupOptions: {
      input: resolve(here, "src/main.tsx"),
      external: ["@localspace/harness-sdk", "@localspace/canvas", "@localspace/ui", "react", "react/jsx-runtime", "react-dom/client", "@automerge/automerge"],
      output: {
        format: "es",
        entryFileNames: "index.js",
        chunkFileNames: "assets/[name]-[hash].js",
        assetFileNames: "assets/[name]-[hash][extname]",
      },
    },
  },
});
