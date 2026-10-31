# 2026-10-31: no way into the whiteboard on a default install

The whiteboard left the product in October. Its full removal, with an export of old boards, comes later with the legacy removal. Until then, nothing on screen leads to it.

- **Core already hides it.** A release sets the whiteboard and the planner aside (`localspace_core::SET_ASIDE`, the default of the desktop app and of `localspace serve`, in personal and organisation mode alike). A set-aside harness is:
  - not loaded, not offered by a catalog, not installable;
  - its documents not listed.

  Every visible way in comes from those lists: the sidebar's board buttons, the Store and its entry, Help's whiteboard row, the "Plan on the board" card in an empty chat, the Documents page, and the board page itself. So none of them shows on a default install.
- **Documents and the Store, in both modes:**
  - the Store shows only when Core offers a tool, and the only tools are the whiteboard's and the planner's;
  - Documents shows only when Core lists a document, and it lists none of a set-aside harness's.

  Neither shows on a default install.
- **What the client still showed, now removed, with `themes` off and on:**
  - an old link to a board (`/board/…`) said "That board is not in this workspace"; it now opens the chat and says nothing;
  - Settings → Tools said "Add one from the Store" with a link to the empty Store; the link shows only where the Store does (`web/src/useStoreShown.ts`, the sidebar's own rule);
  - Documents' title and empty text spoke of boards; they no longer do.
- **The client still shows the whiteboard where Core offers it.** Only CI's test servers and `scripts/check.sh` set `set_aside = []`, so that the walks keep testing Core's whiteboard machinery until it is removed.
- **A check in the looks walk:** on a default install, in every look and on every screen, the page names no whiteboard, planner, board or Store, and offers no Whiteboard, Documents or Store button. The old board link is checked too.
- **For the legacy removal (not now):**
  - Core's whiteboard and planner harnesses and their data, with an export of old boards first;
  - the CI steps and `scripts/check.sh` lines that turn them back on;
  - the client's board page, board buttons, Help row, starter card and board links.
