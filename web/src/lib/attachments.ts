// Pictures a person attaches in the composer (docs/DECISIONS.md, 2026-10-18;
// plan 38, P3): which files are taken, how many, and what a refusal from
// Core says in the person's language.

import type { AttachmentRefusal } from "../api/generated";
import type { Strings } from "./strings";

/** The picture types offered by the file chooser and taken by drag and paste. */
export const PICTURE_TYPES = ["image/png", "image/jpeg", "image/webp"] as const;

/** The most pictures with one message, as Core takes them. */
export const MOST_PICTURES = 4;

/** Whether a file looks like a picture the app takes, by its type or its name. */
export function isPicture(file: { type: string; name: string }): boolean {
  if ((PICTURE_TYPES as readonly string[]).includes(file.type)) return true;
  return /\.(png|jpe?g|webp)$/i.test(file.name);
}

/** What a refusal from Core says, in `strings`' language. */
export function refusalText(strings: Strings, refusal: AttachmentRefusal): string {
  if (refusal === "not_a_picture") return strings.notAPicture;
  if (refusal === "unreadable") return strings.pictureUnreadable;
  if (refusal === "not_found") return strings.pictureNotFound;
  if ("too_large" in refusal) return strings.pictureTooLarge(refusal.too_large.megabytes);
  if ("too_many_pixels" in refusal) return strings.pictureTooManyPixels(refusal.too_many_pixels.width, refusal.too_many_pixels.height);
  return strings.tooManyPictures(refusal.too_many.most);
}
