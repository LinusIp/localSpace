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

const en: Strings = {
  languageTitle: "Language",
  languageSystem: "Same as Windows",
  memoryTight: "Needs most of this computer's memory: close other programs, such as your browser, before you use it.",
  attachFile: "Attach a picture or a text file",
  removePicture: "Remove",
  dropFileHere: "Drop the picture or file here",
  pictureTooLarge: (mb) => `This file is ${mb} MB, and pictures up to 30 MB can be attached. Attach a normal-size photo or a screenshot instead.`,
  pictureTooManyPixels: (w, h) =>
    `This picture is ${w} by ${h} pixels, and pictures up to 60 megapixels can be attached. Attach a normal-size photo or a screenshot instead.`,
  notAPicture: "Only PNG, JPEG and WebP pictures can be attached.",
  pictureUnreadable: "This picture could not be read. Try saving it again, or take a screenshot of it.",
  tooManyPictures: (most) => `Up to ${most} pictures or files can go with one message.`,
  pictureNotFound: "That file is no longer here. Attach it again.",
  notText: "Only pictures (PNG, JPEG, WebP) and text files (.txt, .md, .csv and code) can be attached.",
  emptyFile: "This file is empty.",
  textTooLarge: (mb) => `This file is ${mb} MB, and text files up to 2 MB can be attached.`,
  fileNotAttached: "This file could not be attached. Try again.",
  filePartRead: (percent, words) =>
    `Too long to read whole: the model read the first ${percent}%${words ? ` (about ${words.read} of ${words.total} words)` : ""}.`,
  fileNotRead: "No room was left in this message to read this file, so the model didn't read it.",
  saveFile: "Save",
  cannotReadPictures: (model) => `${model} can't read pictures.`,
  pictureFileLeftOut: (gb, model) =>
    `Pictures need about ${gb} GB more memory than is free now. Close other programs, or switch to ${model} for pictures.`,
  pictureFileLeftOutAlone: (gb) => `Pictures need about ${gb} GB more memory than is free now. Close other programs, then try again.`,
  olderMessagesLeftOut: "Earlier messages are no longer read by the model. They stay here for you.",
  tryAgain: "Try again",
  answerStoppedHere: "The answer stopped here.",
  continueAnswer: "Continue",
  atLimit: "The answer reached its length limit.",
  repeated: "The answer started repeating itself, so it was stopped.",
  chatFull: "This chat is full, so the answer was cut short. Start a new chat to go on.",
  chatFullNoRoom: "This chat is full, so the model had no room to answer. Start a new chat to go on.",
  tooLong: "This message is too long for this model, so the answer was cut short. Send a shorter message, or attach the text as a file.",
  tooLongNoRoom: "This message is too long for this model, so it had no room to answer. Send a shorter message, or attach the text as a file.",
  newChat: "New chat",
  thoughtTooLong: "The model used all its room for thinking and wrote no answer.",
  modelFailed: "The model couldn't answer. If it happens again, load the model again in Settings.",
  modelFailedAskAdmin: "The model couldn't answer. If it happens again, tell your administrator.",
  noAnswer: "The model gave no answer. Ask again.",
  chooseModel: "Choose a model",
  onlineForDownloads: "Online for downloads only",
  offline: "Offline",
  chooseModelToStart: "Choose a model to start, then ask here",
  send: "Send",
  stop: "Stop",
  chooseModelLabel: "Choose a model",
  notResponding: "localSpace isn't responding. Close it and open it again.",
  noToolsYet: "No tools are available for this version yet.",
  switchForPictures: (model) => `Switch to ${model} for pictures`,
  downloadForPictures: (model) => `Download ${model} for pictures`,
};

const ru: Strings = {
  languageTitle: "Язык",
  languageSystem: "Как в Windows",
  memoryTight: "Занимает большую часть памяти этого компьютера: перед работой закройте другие программы, например браузер.",
  attachFile: "Прикрепить изображение или текстовый файл",
  removePicture: "Убрать",
  dropFileHere: "Перетащите изображение или файл сюда",
  pictureTooLarge: (mb) =>
    `Этот файл весит ${mb} МБ, а прикрепить можно изображения до 30 МБ. Прикрепите фото обычного размера или снимок экрана.`,
  pictureTooManyPixels: (w, h) =>
    `Это изображение ${w} на ${h} пикселей, а прикрепить можно изображения до 60 мегапикселей. Прикрепите фото обычного размера или снимок экрана.`,
  notAPicture: "Прикрепить можно только изображения PNG, JPEG и WebP.",
  pictureUnreadable: "Не удалось прочитать это изображение. Сохраните его заново или сделайте снимок экрана.",
  tooManyPictures: (most) => `К одному сообщению можно прикрепить не больше ${most} изображений или файлов.`,
  pictureNotFound: "Этого файла больше нет. Прикрепите его заново.",
  notText: "Прикрепить можно только изображения (PNG, JPEG, WebP) и текстовые файлы (.txt, .md, .csv и код).",
  emptyFile: "Этот файл пустой.",
  textTooLarge: (mb) => `Этот файл весит ${mb} МБ, а прикрепить можно текстовые файлы до 2 МБ.`,
  fileNotAttached: "Не удалось прикрепить этот файл. Попробуйте ещё раз.",
  filePartRead: (percent, words) =>
    `Файл слишком длинный, чтобы прочитать его целиком: модель прочитала первые ${percent}%${words ? ` (примерно ${words.read} из ${words.total} ${words.count % 10 === 1 && words.count % 100 !== 11 ? "слова" : "слов"})` : ""}.`,
  fileNotRead: "В этом сообщении не хватило места, чтобы прочитать этот файл, поэтому модель его не прочитала.",
  saveFile: "Сохранить",
  cannotReadPictures: (model) => `${model} не умеет читать изображения.`,
  pictureFileLeftOut: (gb, model) =>
    `Для изображений нужно ещё примерно ${gb} ГБ свободной памяти. Закройте другие программы или переключитесь на ${model} для изображений.`,
  pictureFileLeftOutAlone: (gb) =>
    `Для изображений нужно ещё примерно ${gb} ГБ свободной памяти. Закройте другие программы и попробуйте снова.`,
  olderMessagesLeftOut: "Более ранние сообщения модель больше не читает. Они остаются здесь для вас.",
  tryAgain: "Попробовать снова",
  answerStoppedHere: "Ответ остановлен здесь.",
  continueAnswer: "Продолжить",
  atLimit: "Ответ достиг предела длины.",
  repeated: "Ответ начал повторяться, поэтому он остановлен.",
  chatFull: "Этот чат заполнен, поэтому ответ оборван. Чтобы продолжить, начните новый чат.",
  chatFullNoRoom: "Этот чат заполнен, поэтому модели не хватило места для ответа. Чтобы продолжить, начните новый чат.",
  tooLong: "Это сообщение слишком длинное для этой модели, поэтому ответ оборван. Отправьте сообщение покороче или прикрепите текст файлом.",
  tooLongNoRoom: "Это сообщение слишком длинное для этой модели, поэтому ей не хватило места для ответа. Отправьте сообщение покороче или прикрепите текст файлом.",
  newChat: "Новый чат",
  thoughtTooLong: "Модель потратила всё место на размышления и не написала ответ.",
  modelFailed: "Модели не удалось ответить. Если это повторится, загрузите модель заново в Settings.",
  modelFailedAskAdmin: "Модели не удалось ответить. Если это повторится, сообщите администратору.",
  noAnswer: "Модель не дала ответа. Спросите ещё раз.",
  chooseModel: "Выберите модель",
  onlineForDownloads: "В сети только для загрузок",
  offline: "Не в сети",
  chooseModelToStart: "Выберите модель, чтобы начать, а затем задайте вопрос здесь",
  send: "Отправить",
  stop: "Остановить",
  chooseModelLabel: "Выбрать модель",
  notResponding: "localSpace не отвечает. Закройте его и откройте снова.",
  noToolsYet: "Для этой версии пока нет инструментов.",
  switchForPictures: (model) => `Переключиться на ${model} для изображений`,
  downloadForPictures: (model) => `Скачать ${model} для изображений`,
};

const uz: Strings = {
  languageTitle: "Til",
  languageSystem: "Windows dagidek",
  memoryTight: "Bu kompyuter xotirasining katta qismini egallaydi: ishlatishdan oldin boshqa dasturlarni, masalan, brauzerni yoping.",
  attachFile: "Rasm yoki matnli fayl biriktirish",
  removePicture: "Olib tashlash",
  dropFileHere: "Rasm yoki faylni shu yerga tashlang",
  pictureTooLarge: (mb) =>
    `Bu fayl ${mb} MB, 30 MB gacha boʻlgan rasmlarni biriktirish mumkin. Oddiy oʻlchamdagi surat yoki ekran rasmini biriktiring.`,
  pictureTooManyPixels: (w, h) =>
    `Bu rasm ${w} × ${h} piksel, 60 megapikselgacha boʻlgan rasmlarni biriktirish mumkin. Oddiy oʻlchamdagi surat yoki ekran rasmini biriktiring.`,
  notAPicture: "Faqat PNG, JPEG va WebP rasmlarini biriktirish mumkin.",
  pictureUnreadable: "Bu rasmni oʻqib boʻlmadi. Uni qaytadan saqlang yoki ekran rasmini oling.",
  tooManyPictures: (most) => `Bitta xabarga ${most} tagacha rasm yoki fayl biriktirish mumkin.`,
  pictureNotFound: "Bu fayl endi yoʻq. Uni qaytadan biriktiring.",
  notText: "Faqat rasmlarni (PNG, JPEG, WebP) va matnli fayllarni (.txt, .md, .csv va kod) biriktirish mumkin.",
  emptyFile: "Bu fayl boʻsh.",
  textTooLarge: (mb) => `Bu fayl ${mb} MB. Faqat 2 MB gacha boʻlgan matnli fayllarni biriktirish mumkin.`,
  fileNotAttached: "Bu faylni biriktirib boʻlmadi. Qaytadan urinib koʻring.",
  filePartRead: (percent, words) =>
    `Butunlay oʻqish uchun juda uzun: model birinchi ${percent}% qismini oʻqidi${words ? ` (taxminan ${words.total} soʻzdan ${words.read} tasi)` : ""}.`,
  fileNotRead: "Bu xabarda faylni oʻqish uchun joy qolmadi, shuning uchun model uni oʻqimadi.",
  saveFile: "Saqlash",
  cannotReadPictures: (model) => `${model} rasmlarni oʻqiy olmaydi.`,
  pictureFileLeftOut: (gb, model) =>
    `Rasmlar uchun yana taxminan ${gb} GB boʻsh xotira kerak. Boshqa dasturlarni yoping yoki rasmlar uchun ${model} ga oʻting.`,
  pictureFileLeftOutAlone: (gb) =>
    `Rasmlar uchun yana taxminan ${gb} GB boʻsh xotira kerak. Boshqa dasturlarni yoping va qayta urinib koʻring.`,
  olderMessagesLeftOut: "Oldingi xabarlarni model endi oʻqimaydi. Ular siz uchun shu yerda qoladi.",
  tryAgain: "Qayta urinib koʻrish",
  answerStoppedHere: "Javob shu yerda toʻxtadi.",
  continueAnswer: "Davom ettirish",
  atLimit: "Javob uzunlik chegarasiga yetdi.",
  repeated: "Javob takrorlana boshladi, shuning uchun u toʻxtatildi.",
  chatFull: "Bu suhbat toʻldi, shuning uchun javob chala qoldi. Davom etish uchun yangi suhbat boshlang.",
  chatFullNoRoom: "Bu suhbat toʻldi, shuning uchun modelga javob berish uchun joy qolmadi. Davom etish uchun yangi suhbat boshlang.",
  tooLong: "Bu xabar ushbu model uchun juda uzun, shuning uchun javob chala qoldi. Qisqaroq xabar yuboring yoki matnni fayl sifatida biriktiring.",
  tooLongNoRoom: "Bu xabar ushbu model uchun juda uzun, shuning uchun modelga javob berish uchun joy qolmadi. Qisqaroq xabar yuboring yoki matnni fayl sifatida biriktiring.",
  newChat: "Yangi suhbat",
  thoughtTooLong: "Model butun joyini oʻylashga sarfladi va javob yozmadi.",
  modelFailed: "Model javob bera olmadi. Yana takrorlansa, modelni Settings boʻlimida qaytadan yuklang.",
  modelFailedAskAdmin: "Model javob bera olmadi. Yana takrorlansa, administratoringizga xabar bering.",
  noAnswer: "Model javob bermadi. Qaytadan soʻrang.",
  chooseModel: "Modelni tanlang",
  onlineForDownloads: "Faqat yuklab olish uchun onlayn",
  offline: "Oflayn",
  chooseModelToStart: "Boshlash uchun modelni tanlang, keyin shu yerda soʻrang",
  send: "Yuborish",
  stop: "Toʻxtatish",
  chooseModelLabel: "Modelni tanlash",
  notResponding: "localSpace javob bermayapti. Uni yoping va qaytadan oching.",
  noToolsYet: "Bu versiya uchun hozircha vositalar yoʻq.",
  switchForPictures: (model) => `Rasmlar uchun ${model} ga oʻtish`,
  downloadForPictures: (model) => `Rasmlar uchun ${model} ni yuklab olish`,
};

export const STRINGS: Record<Language, Strings> = { en, ru, uz };

/** The strings of `language`. */
export function stringsOf(language: Language): Strings {
  return STRINGS[language];
}
