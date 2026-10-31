// Whether a switched feature is on, as Core last reported it
// (docs/decisions/2026-10-31-switched-features.md). Off until the
// environment arrives.

import { useEffect } from "react";
import type { Feature } from "./api/generated";
import { featureIn } from "./lib/features";
import { applyLook, rememberedLook } from "./lib/look";
import { useSession } from "./store";

/** Whether `name` is on here. */
export function useFeature(name: Feature): boolean {
  return useSession((s) => featureIn(s.environment?.features, name));
}

/**
 * Keeps the page in the look Core says: the remembered one until the
 * environment arrives, then `themes` with the person's palette and mode, or
 * the old look with the switch off.
 */
export function useLook(): void {
  const known = useSession((s) => s.environment !== null);
  const on = useFeature("themes");
  const palette = useSession((s) => s.palette);
  const mode = useSession((s) => s.mode);
  useEffect(() => {
    if (!known) {
      applyLook(rememberedLook());
      return;
    }
    applyLook(on ? { on: true, palette, mode } : { on: false });
  }, [known, on, palette, mode]);
}
