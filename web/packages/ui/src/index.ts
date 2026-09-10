// @localspace/ui: the component library (architecture v2.1 §6.1). Tokens
// and styles in `ui.css`; function components over them; our own icons.
// React is the rendering runtime underneath; nothing else.

import "./ui.css";

export * from "./controls.tsx";
export * from "./overlays.tsx";
export * from "./dock.tsx";
export * from "./icons.tsx";
export { useStore, createStore } from "./store.ts";
export type { Store, StoreApi } from "./store.ts";
