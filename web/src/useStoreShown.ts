// Whether the Store is shown anywhere: once it has a tool to offer, or one is
// installed. A Store with nothing in it promises what does not exist yet
// (ruled 2026-10-10, when 0.2 ships no tools). The whiteboard and the planner
// are set aside in this release, so a default install offers none.

import { useSession } from "./store";

export function useStoreShown(): boolean {
  return useSession(
    (s) => s.catalog.some((e) => e.kind === "harness" && e.tool_count > 0) || (s.environment?.harnesses ?? []).some((h) => h.tool_count > 0),
  );
}
