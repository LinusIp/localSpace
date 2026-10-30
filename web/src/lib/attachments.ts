// Pictures and text files a person attaches in the composer
// (docs/DECISIONS.md, 2026-10-18 and 2026-10-26; plan 38, P3 and F1): which
// files are taken, how many, and what a refusal from Core says in the
// person's language.

import type { Attachment, AttachmentRefusal } from "../api/generated";
import type { Language, Strings } from "./strings";

/** The picture types offered by the file chooser and taken by drag and paste. */
export const PICTURE_TYPES = ["image/png", "image/jpeg", "image/webp"] as const;

/** Text files the composer takes, by name when the browser gives no type (plan 38, F1). */
export const TEXT_EXTENSIONS = [
  "txt", "md", "markdown", "csv", "tsv", "json", "xml", "yaml", "yml", "toml", "ini", "log",
  "rs", "py", "js", "ts", "tsx", "jsx", "java", "c", "h", "cpp", "hpp", "cs", "go", "rb", "php",
  "swift", "kt", "sql", "sh", "ps1", "bat", "html", "css", "svg",
] as const;

/** What the file chooser offers: pictures and text files. */
export const ACCEPT = [...PICTURE_TYPES, "text/*", ...TEXT_EXTENSIONS.map((e) => `.${e}`)].join(",");

/** The most attachments, pictures and files together, with one message, as Core takes them. */
export const MOST_ATTACHMENTS = 4;

/** Core's limit for a text file; it holds to it, and the composer says so before sending one larger. */
export const TEXT_MAX_BYTES = 2 * 1024 * 1024;

/** Files known not to be text, by their type or their name. */
const NOT_TEXT_TYPES = /^(audio|video|image|font)\/|^application\/(pdf|zip|x-zip|x-7z|x-rar|x-compressed|x-bzip|x-xz|zstd|epub\+zip|java-archive|vnd\.|msword|x-msdownload|x-msi|gzip|x-tar|octet-stream$)/;
const NOT_TEXT_EXTENSIONS = [
  "pdf", "zip", "7z", "rar", "gz", "tgz", "tar", "xz", "bz2", "zst", "jar", "apk", "epub",
  "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods", "odp", "msg", "pst",
  "exe", "dll", "msi", "iso", "bin", "img", "dmg", "vhd", "vhdx", "so", "wasm", "lz4", "zipx",
  "gguf", "safetensors", "onnx", "pt", "pth", "ckpt", "h5", "pkl", "npy", "npz", "parquet",
  "db", "db3", "sqlite", "sqlite3", "mdb", "accdb",
  "mp3", "mp4", "m4a", "mov", "avi", "mkv", "wav", "flac", "ogg", "webm",
  "gif", "bmp", "tif", "tiff", "heic", "psd", "ttf", "otf", "woff", "woff2",
];

/** Whether a file too large to send may be text: it is told the size limit, unless its type or its name says it is something else, which is told it can't be attached at all. */
export function mayBeText(file: { type: string; name: string }): boolean {
  const dot = file.name.lastIndexOf(".");
  const extension = dot > 0 ? file.name.slice(dot + 1).toLowerCase() : "";
  // A name the chooser offers wins over the type Windows gives it (.ts can come as video, .csv as Excel).
  if ((TEXT_EXTENSIONS as readonly string[]).includes(extension)) return true;
  if (NOT_TEXT_TYPES.test(file.type)) return false;
  return !NOT_TEXT_EXTENSIONS.includes(extension);
}

/** Whether a file looks like a picture the app takes, by its type or its name. */
export function isPicture(file: { type: string; name: string }): boolean {
  if ((PICTURE_TYPES as readonly string[]).includes(file.type)) return true;
  return /\.(png|jpe?g|webp)$/i.test(file.name);
}

/**
 * What the person is told of a text file the model read only the start of,
 * or none of (plan 38, F4: never cut silently); null when it read all of it.
 */
export function partReadText(strings: Strings, file: Attachment, language: Language): string | null {
  const t = file.text;
  if (!t || t.read_chars >= t.chars) return null;
  if (t.read_chars === 0) return strings.fileNotRead;
  // Rounded down, so as never to claim more was read: whole numbers from
  // 1%, one decimal below it, and "<0.1" below that, never "0".
  const exact = (t.read_chars * 100) / Math.max(1, t.chars);
  const tenth = new Intl.NumberFormat(language, { minimumFractionDigits: 1, maximumFractionDigits: 1 });
  const percent =
    exact >= 1
      ? new Intl.NumberFormat(language).format(Math.floor(exact))
      : exact >= 0.1
        ? tenth.format(Math.floor(exact * 10) / 10)
        : `<${tenth.format(0.1)}`;
  // The words only when there are enough of them to say something.
  const n = new Intl.NumberFormat(language);
  // ...and only when they agree with the share of the characters (a long
  // line of data, a picture inside a Markdown file, can make them differ).
  const wordShare = t.read_words / Math.max(1, t.words);
  const charShare = t.read_chars / Math.max(1, t.chars);
  const agree = wordShare <= charShare * 2 && charShare <= wordShare * 2;
  const words = t.words >= 100 && t.read_words < t.words && agree ? { read: n.format(t.read_words), total: n.format(t.words), count: t.words } : null;
  return strings.filePartRead(percent, words);
}

/** A file's size as the message says it: megabytes to one decimal, rounded up, in the language's own figures. */
export function aboutMegabytes(bytes: number, language: Language): string {
  return new Intl.NumberFormat(language, { minimumFractionDigits: 1, maximumFractionDigits: 1 }).format(Math.ceil((bytes / (1024 * 1024)) * 10) / 10);
}

/**
 * MiB as the message says them: gigabytes to one decimal, rounded up so as
 * never to promise too little, in the language's own figures ("1.5", "1,5").
 */
export function aboutGigabytes(mib: number, language: Language): string {
  return new Intl.NumberFormat(language, { maximumFractionDigits: 1 }).format(Math.ceil((mib / 1024) * 10) / 10);
}

/** The words for a picture file left out at this load (plan 38, P5), with the model that reads pictures instead, if any. */
export function leftOutText(strings: Strings, mib: number, language: Language, instead: string | null): string {
  const gb = aboutGigabytes(mib, language);
  return instead ? strings.pictureFileLeftOut(gb, instead) : strings.pictureFileLeftOutAlone(gb);
}

/**
 * What a refusal from Core says, in `strings`' language; `model` names the
 * model in use, and `instead` one that reads pictures here.
 */
export function refusalText(
  strings: Strings,
  refusal: AttachmentRefusal,
  model = "This model",
  instead: string | null = null,
  language: Language = "en",
): string {
  if (refusal === "cannot_read_pictures") return strings.cannotReadPictures(model);
  if (refusal === "not_a_picture") return strings.notAPicture;
  if (refusal === "unreadable") return strings.pictureUnreadable;
  if (refusal === "not_found") return strings.pictureNotFound;
  if (refusal === "not_text") return strings.notText;
  if (refusal === "empty_file") return strings.emptyFile;
  if ("text_too_large" in refusal) return strings.textTooLarge(aboutMegabytes(refusal.text_too_large.bytes, language));
  if ("too_large" in refusal) return strings.pictureTooLarge(refusal.too_large.megabytes);
  if ("too_many_pixels" in refusal) return strings.pictureTooManyPixels(refusal.too_many_pixels.width, refusal.too_many_pixels.height);
  if ("picture_file_left_out" in refusal) return leftOutText(strings, refusal.picture_file_left_out.needs_mib, language, instead);
  return strings.tooManyPictures(refusal.too_many.most);
}
