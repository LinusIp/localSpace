// Models as a person reads them (Directive 5: human names, never a filename).

import type { ModelCatalogEntry } from "../api/generated";

/** The name shown for a model id: the catalog's title, or the name an endpoint was given. */
export function modelLabel(id: string | null | undefined, catalog: ModelCatalogEntry[]): string {
  if (!id) return "No model";
  const entry = catalog.find((m) => m.id === id);
  if (entry) return entry.title;
  // `http://host/v1|name`: a model on a server the user connected under Advanced.
  const bar = id.lastIndexOf("|");
  if (bar >= 0) return id.slice(bar + 1);
  return id;
}

/** One line on why to pick a model, in plain words, from what the planner knows. */
export function modelReason(entry: ModelCatalogEntry): string {
  if (entry.notes) return entry.notes;
  switch (entry.verdict) {
    case "resident":
      return "Answers quickly on this server.";
    case "hybrid":
    case "streaming":
      return "Slower to answer here; better at longer documents and difficult reasoning.";
    case "does not fit":
      return "Too large for this server.";
    default:
      return "Runs on this server.";
  }
}
