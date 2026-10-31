# 2026-10-31: Settings → Appearance

`docs/design/0.2.1-THEMES-AND-TRANSLATION.md` §2: people choose the palette and the mode themselves.

- **A pane of its own, Settings → Appearance,** between General and Assistant. **It is offered only while `themes` is on.** With the switch off it isn't listed, and a stale choice of the pane falls back to General.
- **Colours:** "Indigo and amber", "Violet and magenta" and "Petrol teal", each with a preview of its two colours.
- **Mode:** "Same as Windows", "Light" and "Dark". "Same as Windows" changes when Windows does.
- **Kept by Core,** as the person's preferences `theme` and `mode`, so a choice holds after a restart. On an organisation's server, each person chooses their own.
- **The previews' colours** are tokens in `look.css` (`--ls-preview-*`), so the rule that every colour comes from a token still holds.
- **The looks walk** opens Appearance in every palette, checks that it shows the palette in use, and checks that it isn't offered with `themes` off.
