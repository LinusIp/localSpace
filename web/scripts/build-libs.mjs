// The libraries the shell provides to harness frames through the import map
// (architecture v2.1 §6.3): one ES module each under `dist/_localspace/`,
// served on every harness origin beside the SDK. React is bundled once;
// everything that needs it imports the bare name, which the import map
// resolves to that one copy. Run after the shell's own build.

import { build } from "vite";
import { resolve } from "node:path";
import { copyFileSync, existsSync, readdirSync, renameSync } from "node:fs";

const root = process.cwd();
const outDir = resolve(root, "dist/_localspace");

/** name → { entry, external } */
const libs = [
  { name: "react", entry: "packages/libs/react.ts", external: [] },
  { name: "react-jsx-runtime", entry: "packages/libs/react-jsx-runtime.ts", external: ["react"] },
  { name: "react-dom-client", entry: "packages/libs/react-dom-client.ts", external: ["react", "react/jsx-runtime"] },
  { name: "canvas", entry: "packages/canvas/src/index.ts", external: [] },
  { name: "ui", entry: "packages/ui/src/index.ts", external: ["react", "react/jsx-runtime", "react-dom/client"] },
  { name: "automerge", entry: "packages/libs/automerge.ts", external: [] },
];

let first = true;
for (const lib of libs) {
  const entry = resolve(root, lib.entry);
  if (!existsSync(entry)) {
    console.log(`skip ${lib.name}: no ${lib.entry} yet`);
    continue;
  }
  await build({
    configFile: false,
    root,
    logLevel: "warn",
    define: { "process.env.NODE_ENV": JSON.stringify("production") },
    build: {
      outDir,
      emptyOutDir: first,
      target: "es2022",
      sourcemap: false,
      minify: true,
      lib: { entry, formats: ["es"], fileName: () => `${lib.name}.js`, cssFileName: lib.name },
      rollupOptions: {
        external: lib.external,
        output: { chunkFileNames: `${lib.name}-[hash].js`, assetFileNames: `${lib.name}-[hash][extname]` },
      },
    },
  });
  first = false;
  console.log(`built ${lib.name}.js`);
}

// The library's stylesheet keeps the library's name, whatever the bundler
// called it, because the harness page links it by that name.
for (const file of readdirSync(outDir)) {
  const m = /^(ui)-[A-Za-z0-9_-]+\.css$/.exec(file);
  if (m) renameSync(resolve(outDir, file), resolve(outDir, `${m[1]}.css`));
}

// Automerge loads its WebAssembly from beside its module, by this name.
const wasm = resolve(root, "node_modules/@automerge/automerge/dist/automerge.wasm");
if (existsSync(wasm)) copyFileSync(wasm, resolve(outDir, "automerge_wasm_bg.wasm"));
