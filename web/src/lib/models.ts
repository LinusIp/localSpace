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

/**
 * A download's size as a person says it: "2.0 GB", "469 MB". Counted as
 * Windows counts (1 GB = 1024 MB), so the figure matches File Explorer's and
 * the free space beside it, which Core reports the same way.
 */
export function sizeInWords(bytes: number): string {
  return bytes >= 2 ** 30 ? `${(bytes / 2 ** 30).toFixed(1)} GB` : `${Math.round(bytes / 2 ** 20)} MB`;
}

/** Whether this computer's memory cannot hold the model at all. */
export function willNotFit(entry: ModelCatalogEntry): boolean {
  return entry.verdict === "will_not_fit" || entry.verdict === "does not fit";
}

/**
 * One line on how a model will run here, in plain words. On a person's own
 * computer that is the verdict and a range of words a second, known before
 * any download; on a server of the reference tiers, what the planner knows.
 * A model of the smallest band carries Core's sentence on what to expect of
 * its answers, wherever it is listed.
 */
export function modelReason(entry: ModelCatalogEntry): string {
  const said = howItRuns(entry);
  return entry.quality_words ? `${said} ${entry.quality_words}` : said;
}

function howItRuns(entry: ModelCatalogEntry): string {
  if (entry.verdict_label) {
    const how = entry.speed ? `${entry.verdict_label}: ${entry.speed}.` : `${entry.verdict_label}.`;
    return entry.placement ? `${how} ${entry.placement}` : how;
  }
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
