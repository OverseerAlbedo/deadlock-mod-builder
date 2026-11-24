/** File windows that start in the folder last used for the same kind of file.
 *
 * Every open and save window of the app goes through `openFor` / `saveFor` with a purpose
 * (project, model source, gun file …). The folder of the last successful pick is kept per
 * purpose on this PC (`localStorage` `dmb.dialogFolders`), so a project is saved where the
 * last project went, not wherever the last texture came from. Storage that is unavailable or
 * throws is ignored: the window then opens where Windows chooses, as before. */
import { open, save, type OpenDialogOptions, type SaveDialogOptions } from '@tauri-apps/plugin-dialog';
import type { PathKind } from './models';

export type DialogPurpose =
  | 'project' | 'model-source' | 'gun-file' | 'texture-folder' | 'texture-image' | 'clothing-or-prop'
  | 'tested-cloth' | 'icon-image' | 'sound-file' | 'effect-file' | 'animation-clip' | 'vpk'
  | 'outfit-preset' | 'source-package' | 'tools-folder' | 'log' | 'timing-report' | 'object-file'
  /** Motion → Make it from my animation: the animation file (.blend, .fbx, .glb, .gltf) the clip is made from. */
  | 'animation-blend';

export const DIALOG_FOLDERS = 'dmb.dialogFolders';

/** The purpose of a `FilePathInput` (and an Extras drop zone) from its kind of path. */
export const PURPOSE_OF_KIND: Record<PathKind, DialogPurpose> = {
  texture_directory: 'texture-folder', texture: 'texture-image', physics: 'tested-cloth', garment: 'clothing-or-prop',
  prop: 'clothing-or-prop', icon: 'icon-image', animation: 'animation-clip', effect: 'effect-file', audio: 'sound-file',
  variant_project: 'project', weapon: 'gun-file',
};

function readAll(): Record<string, string> {
  try {
    const raw = localStorage.getItem(DIALOG_FOLDERS);
    const value: unknown = raw ? JSON.parse(raw) : {};
    if (!value || typeof value !== 'object' || Array.isArray(value)) return {};
    return Object.fromEntries(Object.entries(value).filter((entry): entry is [string, string] => typeof entry[1] === 'string' && entry[1].length > 0));
  } catch { return {}; }
}

/** The folder last used for `purpose`, or undefined. */
export function lastFolder(purpose: DialogPurpose): string | undefined {
  return readAll()[purpose];
}

/** Keep `folder` as the start of the next window for `purpose` (ignored when storage fails). */
export function rememberFolder(purpose: DialogPurpose, folder: string) {
  if (!folder) return;
  try { localStorage.setItem(DIALOG_FOLDERS, JSON.stringify({ ...readAll(), [purpose]: folder })); } catch { /* storage unavailable: nothing is remembered */ }
}

/** The folder that holds `path` (Windows or forward slashes), or '' for a bare name. */
export function folderOf(path: string): string {
  const cut = Math.max(path.lastIndexOf('\\'), path.lastIndexOf('/'));
  if (cut < 0) return '';
  // A drive root keeps its separator ("C:\"), so it stays a folder.
  if (cut === 2 && path[1] === ':') return path.slice(0, 3);
  return cut === 0 ? path.slice(0, 1) : path.slice(0, cut);
}

/** `folder` and `name` joined with the folder's own separator. */
export function joinPath(folder: string, name: string): string {
  if (!folder) return name;
  const separator = folder.includes('\\') || !folder.includes('/') ? '\\' : '/';
  return /[\\/]$/.test(folder) ? `${folder}${name}` : `${folder}${separator}${name}`;
}

/** The file name at the end of `path`. */
const fileName = (path: string) => path.slice(Math.max(path.lastIndexOf('\\'), path.lastIndexOf('/')) + 1);

/** An open window for `purpose`. Files record their folder, folders record themselves. */
export async function openFor(purpose: DialogPurpose, options: OpenDialogOptions): Promise<string | string[] | null> {
  const start = options.defaultPath ?? lastFolder(purpose);
  const picked = await open(start ? { ...options, defaultPath: start } : options);
  const first = Array.isArray(picked) ? picked[0] : picked;
  if (typeof first === 'string' && first) rememberFolder(purpose, options.directory ? first : folderOf(first));
  return picked;
}

/** A save window for `purpose`. `defaultPath`: a full path is used as it is (a project's own
 * file); a bare file name starts in the last folder of the purpose. */
export async function saveFor(purpose: DialogPurpose, options: SaveDialogOptions): Promise<string | null> {
  const asked = options.defaultPath;
  const folder = lastFolder(purpose);
  const defaultPath = asked && folderOf(asked) ? asked : folder ? joinPath(folder, asked ? fileName(asked) : '') : asked;
  const picked = await save(defaultPath ? { ...options, defaultPath } : options);
  if (typeof picked === 'string' && picked) rememberFolder(purpose, folderOf(picked));
  return picked;
}
