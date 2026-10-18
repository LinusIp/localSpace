// What the app says in the person's language (docs/DECISIONS.md, 2026-10-18;
// plan 38, B1): English, Russian and Uzbek (Latin), written here and checked
// by the founder. The language follows Windows' display language unless the
// person chooses one in Settings. In 0.2 the strings below are the new
// messages for pictures and memory; the rest of the app is English until it
// moves here.

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
  /** The composer. */
  attachPicture: string;
  removePicture: string;
  dropPictureHere: string;
  /** Why a picture was not taken. */
  pictureTooLarge: (megabytes: number) => string;
  pictureTooManyPixels: (width: number, height: number) => string;
  notAPicture: string;
  pictureUnreadable: string;
  tooManyPictures: (most: number) => string;
  pictureNotFound: string;
  /** A model that cannot read pictures here, and what to do. */
  cannotReadPictures: (model: string) => string;
  pictureFileLeftOut: (gigabytes: string, model: string) => string;
  switchForPictures: (model: string) => string;
  downloadForPictures: (model: string) => string;
}

const en: Strings = {
  languageTitle: "Language",
  languageSystem: "Same as Windows",
  memoryTight: "Needs most of this computer's memory: close other programs, such as your browser, before you use it.",
  attachPicture: "Attach a picture",
  removePicture: "Remove",
  dropPictureHere: "Drop the picture here",
  pictureTooLarge: (mb) => `This file is ${mb} MB, and pictures up to 30 MB can be attached. Attach a normal-size photo or a screenshot instead.`,
  pictureTooManyPixels: (w, h) =>
    `This picture is ${w} by ${h} pixels, and pictures up to 60 megapixels can be attached. Attach a normal-size photo or a screenshot instead.`,
  notAPicture: "Only PNG, JPEG and WebP pictures can be attached.",
  pictureUnreadable: "This picture could not be read. Try saving it again, or take a screenshot of it.",
  tooManyPictures: (most) => `Up to ${most} pictures can go with one message.`,
  pictureNotFound: "That picture is no longer here. Attach it again.",
  cannotReadPictures: (model) => `${model} can't read pictures.`,
  pictureFileLeftOut: (gb, model) =>
    `Pictures need about ${gb} GB more memory than is free now. Close other programs, or switch to ${model} for pictures.`,
  switchForPictures: (model) => `Switch to ${model} for pictures`,
  downloadForPictures: (model) => `Download ${model} for pictures`,
};

const ru: Strings = {
  languageTitle: "Язык",
  languageSystem: "Как в Windows",
  memoryTight: "Занимает большую часть памяти этого компьютера: перед работой закройте другие программы, например браузер.",
  attachPicture: "Прикрепить изображение",
  removePicture: "Убрать",
  dropPictureHere: "Перетащите изображение сюда",
  pictureTooLarge: (mb) =>
    `Этот файл весит ${mb} МБ, а прикрепить можно изображения до 30 МБ. Прикрепите фото обычного размера или снимок экрана.`,
  pictureTooManyPixels: (w, h) =>
    `Это изображение ${w} на ${h} пикселей, а прикрепить можно изображения до 60 мегапикселей. Прикрепите фото обычного размера или снимок экрана.`,
  notAPicture: "Прикрепить можно только изображения PNG, JPEG и WebP.",
  pictureUnreadable: "Не удалось прочитать это изображение. Сохраните его заново или сделайте снимок экрана.",
  tooManyPictures: (most) => `К одному сообщению можно прикрепить не больше ${most} изображений.`,
  pictureNotFound: "Этого изображения больше нет. Прикрепите его заново.",
  cannotReadPictures: (model) => `${model} не умеет читать изображения.`,
  pictureFileLeftOut: (gb, model) =>
    `Для изображений нужно ещё примерно ${gb} ГБ свободной памяти. Закройте другие программы или переключитесь на ${model} для изображений.`,
  switchForPictures: (model) => `Переключиться на ${model} для изображений`,
  downloadForPictures: (model) => `Скачать ${model} для изображений`,
};

const uz: Strings = {
  languageTitle: "Til",
  languageSystem: "Windows dagidek",
  memoryTight: "Bu kompyuter xotirasining katta qismini egallaydi: ishlatishdan oldin boshqa dasturlarni, masalan, brauzerni yoping.",
  attachPicture: "Rasm biriktirish",
  removePicture: "Olib tashlash",
  dropPictureHere: "Rasmni shu yerga tashlang",
  pictureTooLarge: (mb) =>
    `Bu fayl ${mb} MB, 30 MB gacha boʻlgan rasmlarni biriktirish mumkin. Oddiy oʻlchamdagi surat yoki ekran rasmini biriktiring.`,
  pictureTooManyPixels: (w, h) =>
    `Bu rasm ${w} × ${h} piksel, 60 megapikselgacha boʻlgan rasmlarni biriktirish mumkin. Oddiy oʻlchamdagi surat yoki ekran rasmini biriktiring.`,
  notAPicture: "Faqat PNG, JPEG va WebP rasmlarini biriktirish mumkin.",
  pictureUnreadable: "Bu rasmni oʻqib boʻlmadi. Uni qaytadan saqlang yoki ekran rasmini oling.",
  tooManyPictures: (most) => `Bitta xabarga ${most} tagacha rasm biriktirish mumkin.`,
  pictureNotFound: "Bu rasm endi yoʻq. Uni qaytadan biriktiring.",
  cannotReadPictures: (model) => `${model} rasmlarni oʻqiy olmaydi.`,
  pictureFileLeftOut: (gb, model) =>
    `Rasmlar uchun yana taxminan ${gb} GB boʻsh xotira kerak. Boshqa dasturlarni yoping yoki rasmlar uchun ${model} ga oʻting.`,
  switchForPictures: (model) => `Rasmlar uchun ${model} ga oʻtish`,
  downloadForPictures: (model) => `Rasmlar uchun ${model} ni yuklab olish`,
};

export const STRINGS: Record<Language, Strings> = { en, ru, uz };

/** The strings of `language`. */
export function stringsOf(language: Language): Strings {
  return STRINGS[language];
}
