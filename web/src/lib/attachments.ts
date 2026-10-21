// Pictures a person attaches in the composer (docs/DECISIONS.md, 2026-10-18;
// plan 38, P3): which files are taken, how many, and what a refusal from
// Core says in the person's language.

import type { AttachmentRefusal } from "../api/generated";
import type { Language, Strings } from "./strings";

/** The picture types offered by the file chooser and taken by drag and paste. */
export const PICTURE_TYPES = ["image/png", "image/jpeg", "image/webp"] as const;

/** The most pictures with one message, as Core takes them. */
export const MOST_PICTURES = 4;

/** Whether a file looks like a picture the app takes, by its type or its name. */
export function isPicture(file: { type: string; name: string }): boolean {
  if ((PICTURE_TYPES as readonly string[]).includes(file.type)) return true;
  return /\.(png|jpe?g|webp)$/i.test(file.name);
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
  if ("too_large" in refusal) return strings.pictureTooLarge(refusal.too_large.megabytes);
  if ("too_many_pixels" in refusal) return strings.pictureTooManyPixels(refusal.too_many_pixels.width, refusal.too_many_pixels.height);
  if ("picture_file_left_out" in refusal) return leftOutText(strings, refusal.picture_file_left_out.needs_mib, language, instead);
  return strings.tooManyPictures(refusal.too_many.most);
}
