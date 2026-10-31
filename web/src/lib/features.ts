// The switched features (docs/decisions/2026-10-31-switched-features.md):
// Core decides which are on and reports them in the environment; the client
// only shows or hides. Before the environment arrives, every feature is off.

import type { Feature, Features } from "../api/generated";

/** Whether `name` is on in what Core reported; off when nothing is known yet. */
export function featureIn(features: Features | null | undefined, name: Feature): boolean {
  return features?.on.includes(name) ?? false;
}
