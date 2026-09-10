// Automerge for every harness frame; the import map maps `@automerge/automerge` here.
// The slim build: its WebAssembly is a file of its own beside this module,
// fetched once from the harness origin (connect-src 'self') and compiled
// under 'wasm-unsafe-eval', instead of travelling inside the script as
// base64. Importers wait for it: the module finishes loading only once the
// WebAssembly is ready.
import { initializeWasm } from "@automerge/automerge/slim";

// A variable rather than a literal, so the bundler leaves the URL for the
// browser to resolve beside this module instead of treating it as an asset.
const file = "automerge_wasm_bg.wasm";
await initializeWasm(new URL(file, import.meta.url).href);

export * from "@automerge/automerge/slim";
