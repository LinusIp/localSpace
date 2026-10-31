# 2026-10-31: the palette and the mode are remembered per user

The new look's choice is kept by Core with the user's other preferences, so it follows them across devices.

- **Keys:** `theme` and `mode` join `PREFERENCE_KEYS`, next to `rail_collapsed` and `language`.
- **Values Core keeps:**
  - `theme`: `indigo`, `violet` or `teal`;
  - `mode`: `system`, `light` or `dark`.
- **Anything else is refused** with an error, and nothing is kept. That includes an empty value and a different case.
- **Test:** `the_palette_and_the_mode_are_remembered_and_only_known_values_kept` (`crates/localspace-core/tests/preferences.rs`).
