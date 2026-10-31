# 2026-10-31: the strings, one file per language

`docs/design/0.2.1-THEMES-AND-TRANSLATION.md` §1: adding a language must be one file.

- **One file per language:** `web/src/lib/strings/en.ts`, `ru.ts` and `uz.ts`. `web/src/lib/strings.ts` keeps the `Strings` type, the language helpers and how the files combine.
- **English is complete** (`Strings`). **Russian and Uzbek may lag behind** (`Translation = Partial<Strings>`): a message missing from them shows in English. That lets every screen's words move into the English file first, with the translations following.
- **The `translation` switch:**
  - **off:** only the messages 0.2.0 shipped translated (`TRANSLATED_IN_0_2`, all 48 of today's) show in Russian or Uzbek, and everything else shows in English, exactly as in 0.2.0;
  - **on:** every message the language's file has.
- **Tests:**
  - no language has a message English lacks, and none is empty;
  - every 0.2.0 message is still there in Russian and Uzbek;
  - the switch, off and on, and the fallback to English.
- **Later:** when the translations are complete, a test requires every language to have every message, and the founder reads the Russian and Uzbek wording from a plain list before release.
- **Nothing on screen changes** with this step: every message today is a 0.2.0 one.
