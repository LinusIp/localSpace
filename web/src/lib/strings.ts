// What the app says in the person's language (docs/DECISIONS.md, 2026-10-18):
// English, Russian and Uzbek (Latin), one file each in strings/, the Russian
// and Uzbek checked by the founder. The language follows Windows' display
// language unless the person chooses one in Settings. Messages move here from
// the screens; a language that lacks one shows it in English, and with the
// `translation` switch off only 0.2.0's messages show translated
// (docs/decisions/2026-10-31-strings-one-file-per-language.md).

import { en } from "./strings/en.ts";
import { ru } from "./strings/ru.ts";
import { uz } from "./strings/uz.ts";

export type Language = "en" | "ru" | "uz";

/** What Settings → Language offers: Windows' own, or one of the three. */
export type LanguageChoice = "system" | Language;

export const LANGUAGE_NAMES: Record<Language, string> = {
  en: "English",
  ru: "Русский",
  uz: "Oʻzbekcha",
};

/**
 * The language a person's computer is in, from the languages the webview
 * reports (Windows' display language first): Uzbek or Russian when it is
 * one of those, English otherwise.
 */
export function systemLanguage(languages: readonly string[]): Language {
  for (const tag of languages) {
    const primary = tag.toLowerCase().split(/[-_]/)[0];
    if (primary === "uz") return "uz";
    if (primary === "ru") return "ru";
    if (primary === "en") return "en";
  }
  return "en";
}

/** The language shown: the person's choice, or Windows' when they made none. */
export function languageOf(choice: LanguageChoice, languages: readonly string[]): Language {
  return choice === "system" ? systemLanguage(languages) : choice;
}

export interface Strings {
  /** Settings → General. */
  languageTitle: string;
  languageSystem: string;
  /** A model that would load, but leaves no room to spare (the approved note). */
  memoryTight: string;
  /** Under an answer that ended early: why, in the person's words, and what to do (docs/DECISIONS.md, 2026-10-26). */
  answerStoppedHere: string;
  continueAnswer: string;
  atLimit: string;
  repeated: string;
  chatFull: string;
  chatFullNoRoom: string;
  tooLong: string;
  tooLongNoRoom: string;
  newChat: string;
  thoughtTooLong: string;
  modelFailed: string;
  /** The same, for a person who cannot load a model: a member of an organisation. */
  modelFailedAskAdmin: string;
  noAnswer: string;
  /** The top bar (plan 45 §5): each chip opens the part of Settings it speaks of. */
  chooseModel: string;
  onlineForDownloads: string;
  offline: string;
  /** The composer. */
  chooseModelToStart: string;
  send: string;
  stop: string;
  /** The model's name under the message box, which opens Settings → Assistant. */
  chooseModelLabel: string;
  /** Said when the app on this computer can't reach its own Core. */
  notResponding: string;
  /** The Store on this computer, with no tools for this version. */
  noToolsYet: string;
  attachFile: string;
  removePicture: string;
  dropFileHere: string;
  /** Why a picture was not taken. */
  pictureTooLarge: (megabytes: number) => string;
  pictureTooManyPixels: (width: number, height: number) => string;
  notAPicture: string;
  pictureUnreadable: string;
  tooManyPictures: (most: number) => string;
  pictureNotFound: string;
  /** Why a text file was not taken (plan 38, F1). */
  notText: string;
  emptyFile: string;
  textTooLarge: (megabytes: string) => string;
  /** A text file that could not reach Core (the connection, the session). */
  fileNotAttached: string;
  /** A text file the model read only the start of: how much, never silently (plan 38, F4); its words when there are enough to say. */
  filePartRead: (percent: string, words: { read: string; total: string; count: number } | null) => string;
  /** A text file the model read none of: the message had no room left for it. */
  fileNotRead: string;
  /** A sent text file: saves it on this computer. */
  saveFile: string;
  /** A model that cannot read pictures here, and what to do. */
  cannotReadPictures: (model: string) => string;
  pictureFileLeftOut: (gigabytes: string, model: string) => string;
  /** The same, where no other model here reads pictures. */
  pictureFileLeftOutAlone: (gigabytes: string) => string;
  /** The divider before the first message the model still reads in a long chat. */
  olderMessagesLeftOut: string;
  /** Loads the model again, to keep its picture file if memory is free now. */
  tryAgain: string;
  switchForPictures: (model: string) => string;
  downloadForPictures: (model: string) => string;
}

/** A language's file: any message it lacks shows in English. */
export type Translation = Partial<Strings>;

/**
 * The messages 0.2.0 shipped in Russian and Uzbek. While the `translation`
 * switch is off, only these show translated; every other message shows in
 * English, as in 0.2.0.
 */
export const TRANSLATED_IN_0_2: ReadonlyArray<keyof Strings> = [
  "languageTitle",
  "languageSystem",
  "memoryTight",
  "attachFile",
  "removePicture",
  "dropFileHere",
  "pictureTooLarge",
  "pictureTooManyPixels",
  "notAPicture",
  "pictureUnreadable",
  "tooManyPictures",
  "pictureNotFound",
  "notText",
  "emptyFile",
  "textTooLarge",
  "fileNotAttached",
  "filePartRead",
  "fileNotRead",
  "saveFile",
  "cannotReadPictures",
  "pictureFileLeftOut",
  "pictureFileLeftOutAlone",
  "olderMessagesLeftOut",
  "tryAgain",
  "answerStoppedHere",
  "continueAnswer",
  "atLimit",
  "repeated",
  "chatFull",
  "chatFullNoRoom",
  "tooLong",
  "tooLongNoRoom",
  "newChat",
  "thoughtTooLong",
  "modelFailed",
  "modelFailedAskAdmin",
  "noAnswer",
  "chooseModel",
  "onlineForDownloads",
  "offline",
  "chooseModelToStart",
  "send",
  "stop",
  "chooseModelLabel",
  "notResponding",
  "noToolsYet",
  "switchForPictures",
  "downloadForPictures",
];

export const STRINGS: Record<Language, Translation> = { en, ru, uz };

/** English with `file`'s messages over it: all of them with `translation` on, 0.2.0's with it off. */
export function compose(english: Strings, file: Translation, translation: boolean): Strings {
  if (translation) return { ...english, ...file };
  const shown: Translation = {};
  for (const key of TRANSLATED_IN_0_2) if (file[key] !== undefined) Object.assign(shown, { [key]: file[key] });
  return { ...english, ...shown };
}

/** The strings of `language`, with or without the `translation` switch. */
export function stringsOf(language: Language, translation: boolean): Strings {
  return language === "en" ? en : compose(en, STRINGS[language], translation);
}
