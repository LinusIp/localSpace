import { test } from "node:test";
import assert from "node:assert/strict";
import { STRINGS, TRANSLATED_IN_0_2, compose, languageOf, stringsOf, systemLanguage, type Language, type Strings } from "./strings.ts";

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

const said = (value: unknown) => (typeof value === "function" ? (value as (...a: unknown[]) => string)(30, 40) : value);

test("no language has a message English lacks, and none is empty", () => {
  const keys = new Set(Object.keys(STRINGS.en));
  for (const language of LANGUAGES) {
    for (const [key, value] of Object.entries(STRINGS[language])) {
      assert.ok(keys.has(key), `${language}.${key} is not an English message`);
      const words = said(value);
      assert.ok(typeof words === "string" && words.trim().length > 0, `${language}.${key}`);
    }
  }
});

test("every message 0.2.0 shipped translated is still there in Russian and Uzbek", () => {
  for (const key of TRANSLATED_IN_0_2) {
    assert.ok(key in STRINGS.en, `${key} is not an English message`);
    for (const language of ["ru", "uz"] as const) assert.ok(STRINGS[language][key] !== undefined, `${language}.${key}`);
  }
});

test("with translation off only 0.2.0's messages are translated; on, all of them; missing ones are English", () => {
  const english = { ...(STRINGS.en as Strings), send: "Send", stop: "Stop" };
  const file = { send: "Отправить", stop: "Стоп" };
  // `send` shipped translated in 0.2.0, so it shows either way.
  assert.ok(TRANSLATED_IN_0_2.includes("send"));
  const notIn02 = Object.keys(STRINGS.en).find((k) => !TRANSLATED_IN_0_2.includes(k as keyof Strings));
  if (notIn02) {
    const off = compose(english, { ...file, [notIn02]: "переведено" }, false);
    assert.equal(off.send, "Отправить");
    assert.equal(off[notIn02 as keyof Strings], english[notIn02 as keyof Strings]);
    const on = compose(english, { ...file, [notIn02]: "переведено" }, true);
    assert.equal(on[notIn02 as keyof Strings], "переведено");
  }
  assert.equal(compose(english, {}, true).send, "Send");
  assert.equal(compose(english, file, false).send, "Отправить");
});

test("English is shown as written, with the switch on or off", () => {
  assert.equal(stringsOf("en", false), STRINGS.en);
  assert.equal(stringsOf("en", true), STRINGS.en);
  assert.equal(stringsOf("ru", false).send, STRINGS.ru.send);
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
