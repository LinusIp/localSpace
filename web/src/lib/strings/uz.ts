// Uzbek (Latin). A message missing here shows in English; while the
// `translation` switch is off, only 0.2.0's messages show in Uzbek
// (docs/decisions/2026-10-31-strings-one-file-per-language.md).

import type { Translation } from "../strings.ts";

export const uz: Translation = {
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
