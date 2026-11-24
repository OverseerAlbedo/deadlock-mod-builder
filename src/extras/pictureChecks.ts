import { useSyncExternalStore } from "react";

/**
 * Replacement pictures (icons and item icons) whose preview could not be read, by
 * file path, for the session. The replacement's preview reads the file; a file it
 * cannot decode (a text file named .png) is not counted as Replaced. Analyze and the
 * build check the files again themselves.
 */
const unreadable = new Set<string>();
const listeners = new Set<() => void>();
let version = 0;

/** The start of the preview error for a file that is there but does not decode as a picture
 * (Rust `icons::PICTURE_NOT_READABLE`); other preview failures (busy, a moved file) mark nothing. */
export const PICTURE_NOT_READABLE = "A replacement picture is not a PNG or JPEG image the app can read";
export const isNotAPicture = (error: string) => error.includes(PICTURE_NOT_READABLE);
/** The status a replacement row shows when its picture cannot be read. */
export const NOT_A_PICTURE_STATUS = "Not a picture";
/** The line in the replacement's preview. */
export const NOT_A_PICTURE = "This file is not a picture the app can read. Choose a PNG or JPEG image.";

const key = (path: string) => path.trim().toLowerCase();

export function markPicture(path: string, readable: boolean) {
  const changed = readable ? unreadable.delete(key(path)) : !unreadable.has(key(path)) && Boolean(unreadable.add(key(path)));
  if (!changed) return;
  version++;
  listeners.forEach((listener) => listener());
}
export function isUnreadablePicture(path: string | null | undefined): boolean {
  return Boolean(path && unreadable.has(key(path)));
}
/** Re-renders the caller when a picture is found readable or unreadable. */
export function usePictureChecks(): number {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => version,
  );
}
/** Tests only. */
export function clearPictureChecks() {
  unreadable.clear();
  version++;
  listeners.forEach((listener) => listener());
}
