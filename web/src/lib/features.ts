// The switched features the client knows, each off unless turned on. Until
// Core reports its switches in the environment, the one place to turn one on
// is a temporary local flag: localStorage "ls.features", a comma-separated
// list ("themes,translation"). The browser walks set it to run both ways.

export type Feature = "themes" | "translation";

const FEATURES: readonly Feature[] = ["themes", "translation"];

/** The features a flag value turns on; anything it doesn't name stays off. */
export function featuresIn(flag: string | null): Set<Feature> {
  const on = new Set<Feature>();
  for (const part of (flag ?? "").split(",")) {
    const name = part.trim();
    if ((FEATURES as readonly string[]).includes(name)) on.add(name as Feature);
  }
  return on;
}

function storedFlag(): string | null {
  try {
    return globalThis.localStorage?.getItem("ls.features") ?? null;
  } catch {
    return null;
  }
}

/** Whether a switched feature is on in this window. */
export function featureOn(name: Feature): boolean {
  return featuresIn(storedFlag()).has(name);
}
