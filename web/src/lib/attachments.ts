// Pictures a person attaches in the composer (docs/DECISIONS.md, 2026-10-18;
// plan 38, P3): which files are taken, how many, and what a refusal from
// Core says in the person's language.

import type { AttachmentRefusal, ModelCatalogEntry } from "../api/generated";
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

/** What a refusal from Core says, in `strings`' language; `model` names the model in use. */
export function refusalText(strings: Strings, refusal: AttachmentRefusal, model = "This model"): string {
  if (refusal === "cannot_read_pictures") return strings.cannotReadPictures(model);
  if (refusal === "not_a_picture") return strings.notAPicture;
  if (refusal === "unreadable") return strings.pictureUnreadable;
  if (refusal === "not_found") return strings.pictureNotFound;
  if ("too_large" in refusal) return strings.pictureTooLarge(refusal.too_large.megabytes);
  if ("too_many_pixels" in refusal) return strings.pictureTooManyPixels(refusal.too_many_pixels.width, refusal.too_many_pixels.height);
  return strings.tooManyPictures(refusal.too_many.most);
}

/**
 * The model to suggest when the one in use cannot read pictures
 * (docs/DECISIONS.md, 2026-10-18; plan 38, P4): one with a picture file that
 * runs here with room to spare, already on this computer if one is, and of
 * those the largest. None when nothing fits.
 */
export function pictureModel(entries: ModelCatalogEntry[], current: string | null): ModelCatalogEntry | null {
  const fits = entries.filter(
    (e) => e.reads_pictures && e.id !== current && (e.verdict === "runs_well" || e.verdict === "works") && !e.memory_tight,
  );
  const largest = (list: ModelCatalogEntry[]) =>
    list.reduce<ModelCatalogEntry | null>((best, e) => (best === null || e.params_b > best.params_b ? e : best), null);
  return largest(fits.filter((e) => e.installed)) ?? largest(fits);
}
