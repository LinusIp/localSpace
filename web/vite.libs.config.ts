// The libraries the shell provides to harness frames through the import map
// (architecture v2.1 §6.3): one ES module each under `dist/_localspace/`,
// served on every harness origin beside the SDK. Built after the shell.

import { defineConfig } from "vite";
import { resolve } from "node:path";

export default defineConfig({
  build: {
    outDir: "dist/_localspace",
    emptyOutDir: true,
    target: "es2022",
    sourcemap: false,
    lib: {
      entry: {
        canvas: resolve(__dirname, "packages/canvas/src/index.ts"),
      },
      formats: ["es"],
      fileName: (_format, name) => `${name}.js`,
    },
    rollupOptions: {
      output: {
        chunkFileNames: "[name]-[hash].js",
      },
    },
  },
});
