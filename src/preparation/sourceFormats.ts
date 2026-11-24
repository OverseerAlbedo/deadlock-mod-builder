/** Character model files Model preparation takes (Rust `source::SOURCE_SUFFIXES`).
 * MMD models (.pmx, .pmd) are imported with the MMD Tools add-on in the
 * Blender set in Settings. */
export const SOURCE_EXTENSIONS = ['blend', 'glb', 'gltf', 'fbx', 'pmx', 'pmd'] as const;
/** MikuMikuDance model files. */
export const MMD_EXTENSIONS = ['pmx', 'pmd'] as const;
/** A gun in its own file: the same formats (MMD weapon models too). */
export const GUN_EXTENSIONS = SOURCE_EXTENSIONS;
/** The formats in words, for hints and messages. */
export const SOURCE_FORMATS_TEXT = 'Blender, GLB, glTF, FBX or MMD (.pmx, .pmd)';

/** Whether `path` is a character model file Model preparation takes. */
export function isSourceModel(path: string): boolean {
  const dot = path.lastIndexOf('.');
  return dot >= 0 && (SOURCE_EXTENSIONS as readonly string[]).includes(path.slice(dot + 1).toLowerCase());
}
