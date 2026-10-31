// The strings of the language the person reads (docs/DECISIONS.md,
// 2026-10-18): their choice in Settings, or Windows' display language as the
// webview reports it. With the `translation` switch off, only 0.2.0's
// messages show translated (docs/decisions/2026-10-31-strings-one-file-per-language.md).

import { useSession } from "./store";
import { useFeature } from "./useFeature";
import { languageOf, stringsOf, type Language, type Strings } from "./lib/strings";

/** The language shown now. */
export function useLanguage(): Language {
  const choice = useSession((s) => s.language);
  return languageOf(choice, navigator.languages?.length ? navigator.languages : [navigator.language]);
}

/** The messages in the language shown now. */
export function useStrings(): Strings {
  return stringsOf(useLanguage(), useFeature("translation"));
}
