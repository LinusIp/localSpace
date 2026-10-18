import { test } from "node:test";
import assert from "node:assert/strict";
import { STRINGS, languageOf, systemLanguage, type Language, type Strings } from "./strings.ts";

const LANGUAGES: Language[] = ["en", "ru", "uz"];

test("the language follows Windows: Uzbek or Russian when it is one of those, English otherwise", () => {
  assert.equal(systemLanguage(["uz-Latn-UZ", "en-US"]), "uz");
  assert.equal(systemLanguage(["ru-RU"]), "ru");
  assert.equal(systemLanguage(["en-GB", "ru-RU"]), "en");
  assert.equal(systemLanguage(["de-DE"]), "en");
  assert.equal(systemLanguage(["kk-KZ", "ru-RU"]), "ru");
  assert.equal(systemLanguage([]), "en");
});

test("a choice in Settings wins over Windows", () => {
  assert.equal(languageOf("ru", ["uz-UZ"]), "ru");
  assert.equal(languageOf("system", ["uz-UZ"]), "uz");
});

test("every message is there in all three languages, and none is empty", () => {
  const keys = Object.keys(STRINGS.en) as Array<keyof Strings>;
  for (const language of LANGUAGES) {
    assert.deepEqual(Object.keys(STRINGS[language]).sort(), [...keys].sort(), language);
    for (const key of keys) {
      const value = STRINGS[language][key];
      const said = typeof value === "function" ? (value as (...a: unknown[]) => string)(30, 40) : value;
      assert.ok(typeof said === "string" && said.trim().length > 0, `${language}.${key}`);
    }
  }
});

test("the numbers and the model's name reach the words", () => {
  for (const language of LANGUAGES) {
    const s = STRINGS[language];
    assert.match(s.pictureTooLarge(45), /45/, language);
    assert.match(s.pictureTooManyPixels(9000, 8000), /9000.*8000/, language);
    assert.match(s.tooManyPictures(4), /4/, language);
    assert.match(s.switchForPictures("Qwen3.5 4B"), /Qwen3\.5 4B/, language);
    assert.match(s.pictureFileLeftOut("1.2", "Qwen3.5 4B"), /1\.2.*Qwen3\.5 4B/, language);
  }
});

test("the approved note on memory reads as approved", () => {
  assert.equal(
    STRINGS.en.memoryTight,
    "Needs most of this computer's memory: close other programs, such as your browser, before you use it.",
  );
  assert.equal(
    STRINGS.ru.memoryTight,
    "Занимает большую часть памяти этого компьютера: перед работой закройте другие программы, например браузер.",
  );
  assert.equal(
    STRINGS.uz.memoryTight,
    "Bu kompyuter xotirasining katta qismini egallaydi: ishlatishdan oldin boshqa dasturlarni, masalan, brauzerni yoping.",
  );
});
