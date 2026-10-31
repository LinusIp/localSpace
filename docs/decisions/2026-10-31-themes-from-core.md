# 2026-10-31: the new look follows Core's `themes` switch

The client's temporary flag for the new look is gone. The look now follows Core (`docs/decisions/2026-10-31-switched-features.md`).

- **The switch:** the client shows the new look when Core reports `themes` on, in the environment and in `features_changed`. Until the environment arrives, every feature counts as off (`web/src/lib/features.ts`, `web/src/useFeature.ts`).
- **The palette and the mode** are the person's preferences `theme` and `mode`, held by Core. Settings → Appearance, to choose them, is the next step.
- **This browser remembers the last look it showed** (`localStorage["ls.look"]`). A page drawn before Core answers, such as the sign-in page or a reload, starts in that look instead of flashing the old one. Once Core answers, Core's word wins, and the remembered look follows it. The browser keeps a convenience, never the decision.
- **The browser walks turn the switch the way an installation does:** `set_feature` and `set_preference` through the API. On an organisation's server they act as the administrator, the only one allowed to turn a switch there. Each walk turns `themes` off again when it ends.
- **Off by default:** every channel in `features.toml` leaves `themes` off, so nothing changes for anyone until it is turned on.
