/** IPC contracts are snake_case to match the Serde domain types.
 * Legacy extension values are retained without frontend interpretation. */
export type JsonValue = null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue };
export interface Material {
  preset?: string;
  color?: [number,number,number];
  confirmed?: boolean;
  roughness?: number | null;
  metalness?: number | null;
  mode?: string;
  textures?: Record<string, string>;
  existing?: boolean;
  ambiguous?: string[];
  candidates?: Record<string,string[]>;
  [key: string]: JsonValue | undefined;
}
export interface Project {
  schema: number;
  id: string;
  name: string;
  hero_name: string;
  original_model: string;
  dmx: string;
  textures: string;
  source_root: string;
  model_input_mode: string;
  preparation_source: string;
  preparation_blend: string;
  preparation_report: string;
  preparation_options: Record<string, JsonValue>;
  customization: Customization;
  imported_source: string;
  original_compiled: string;
  source_overlay: string;
  mesh_template: string;
  material_mapping: string;
  material_remap: string;
  skeleton_reference: string;
  expected_dmx_sha256: string;
  meshes: Record<string, string | string[]>;
  materials: Record<string, Material>;
  lod_strategy: string;
  ag2_mode: string;
  animation_policy: string;
  vpk_number: number;
  auto_decompile: boolean;
  auto_map: boolean;
  generate_materials: boolean;
  compile_model: boolean;
  build_vpk: boolean;
  install: boolean;
  review_lods: boolean;
  allow_unskinned: boolean;
  auto_split: boolean;
  split_target: number;
}
export interface ProjectDocument { path: string | null; project: Project }
export interface Settings {
  steam: string;
  deadlock: string;
  pak: string;
  csdk: string;
  resourcecompiler: string;
  cswin: string;
  cswin_resourcecompiler: string;
  model_backend: string;
  model_game: string;
  model_format: string;
  optional_cs2: string;
  source2viewer: string;
  vpk_tool: string;
  vpk_backend: string;
  ag2_adapter: string;
  workspace: string;
  last_project: string;
  blender: string;
  /** Replaced VPKs and original-source backups kept per file. */
  keep_backups?: number;
  keep_previous_preparation?: boolean;
  clear_build_data_after_build?: boolean;
  /** The app's look: "original", "violet", "mint" or a custom theme's id. Saved at once, not with the tool paths. */
  theme?: string;
  /** Themes made in Settings → Appearance → Customize (tokens by name). */
  custom_themes?: ThemePreset[];
}
export interface ThemePreset { id: string; name: string; icon?: string; base?: Record<string, string> | null; tokens: Record<string, string> }
export type StorageKind = 'preparation' | 'originals' | 'backups' | 'output' | 'build_data' | 'compiler' | 'temporary' | 'records';
export interface StorageUsage { kind: StorageKind; bytes: number; reclaimable: number }
export interface StorageProject { id: string; name: string; hero: string; project_file: string | null; open: boolean; locked: boolean; workspace: string | null; modified: number | null; bytes: number; reclaimable: number; usage: StorageUsage[] }
export interface StorageShared { key: string; model: string; bytes: number; used_by: string[] }
export interface StorageReport { workspace: string; bytes: number; reclaimable: number; projects: StorageProject[]; shared: StorageShared[]; other_bytes: number }
export interface StorageClearRequest { project?: string; kinds?: StorageKind[]; remove_project?: boolean; unused_shared?: boolean }
export interface StorageClearResult { removed_bytes: number; removed: string[]; skipped: string[] }
export type OperationStatus = 'running' | 'succeeded' | 'failed' | 'cancelled' | 'skipped' | 'cached';
export type StepStatus = OperationStatus;
/** One Settings path field: right, wrong (with a one-click fix) or optional. */
export interface FieldCheck { field: string; state: 'ok'|'warning'|'problem'|'optional'; message: string; fix?: string|null }
/** An autosave of a project (state/autosave/<id>/<saved_ms>.json). */
export interface Autosave { file: string; id: string; name: string; path: string | null; saved_ms: number }
/** Whether the last session crashed, and the newest autosave of each project. */
export interface SessionRecovery { unclean: boolean; autosaves: Autosave[] }
/** Test my setup: every path, one material compile and the model compiler probe. */
export interface SetupTestStep { name: string; passed: boolean; detail: string; error?: unknown }
export interface SetupTest { passed: boolean; steps: SetupTestStep[] }
/** What this copy of the app was built with (backend get_capabilities). */
export interface Capabilities { tangtang_presets: boolean; vindicta_reference: boolean }
/** Installed mods after a game update (backend mod_update.rs): a VPK in the game's addons folder and its hero models. */
export type AttachmentConvention = 'current' | 'previous' | 'unknown' | 'no_attachments';
export interface ModModel { path: string; hero?: string | null; convention: AttachmentConvention }
export interface InstalledMod { path: string; file: string; bytes: number; models: ModModel[]; status: 'up_to_date' | 'needs_update' | 'no_hero_model' | 'unknown' | 'cannot_update'; note: string }
export interface ModUpdateReport { path: string; file: string; updated: string[]; converted: string[]; backup: string }
export interface SetupStatus { files_found: Record<string,boolean>; model_status: string; model_files_found?: boolean; fields?: FieldCheck[] }
/** What went wrong and what to do (backend troubleshooting.rs). */
export interface ErrorHelp { cause: string; steps: string[]; action?: { label: string; page: string } | null }
export interface TimingStep {
  id: string;
  parent_id: string | null;
  name: string;
  started_utc: string;
  finished_utc: string | null;
  status: StepStatus;
  inclusive_ns: number;
  exclusive_ns: number;
  error: string | null;
  reason: string | null;
}
export interface TimingEvent {
  sequence: number;
  event: 'started' | 'finished';
  step_id: string;
  parent_id: string | null;
  name: string;
  timestamp_utc: string;
  offset_ns: number;
  status: StepStatus;
  error: string | null;
  reason: string | null;
}
export interface TimingReport {
  schema_version: number;
  operation: string;
  run_id: string;
  started_utc: string;
  finished_utc: string | null;
  status: OperationStatus;
  total_ns: number;
  steps: TimingStep[];
  events: TimingEvent[];
}
export interface LogEntry {
  sequence: number;
  timestamp_utc: string;
  level: 'info' | 'warning' | 'error';
  message: string;
}
export interface MeshSummary {
  name: string;
  element: number;
  vertices: number;
  indexed_vertices: number;
  weight_values: number;
  joint_index_values: number;
  has_skin_weights: boolean;
  vertex_attributes: string[];
}
export interface ModelSummary {
  path: string;
  encoding: string;
  format: string;
  format_version: number;
  meshes: MeshSummary[];
  materials: string[];
  material_identities: { name: string; uuid: string; path: string }[];
  bones: string[];
  elements: number;
}
export interface AnalysisResult { summary: ModelSummary; timings: TimingReport }

/** What the installed game does with a model, from its hero definitions (m_strModelName). */
export type GameTarget = {state:'active';heroes:string[]}|{state:'pre_release';heroes:string[]}|{state:'unused';model:string}|{state:'ambiguous';models:string[]}|{state:'disabled';heroes:string[]};
export interface HeroRecord { name: string; path: string; likely_hero: boolean; hero?: string; game?: GameTarget }
export type AssetKind = 'icons' | 'animations' | 'effects' | 'audio';
export type PathKind = 'texture_directory' | 'texture' | 'physics' | 'garment' | 'icon' | 'animation' | 'effect' | 'audio' | 'prop' | 'variant_project' | 'weapon';
export interface ResourceReplacement {
  enabled?: boolean; source?: string; target?: string; allow_shared?: boolean;
  original_sha256?: string; fit?: 'contain' | 'cover' | 'stretch';
  volume_percent?: number; audio_fit?: 'exact' | 'pad' | 'trim';
  [key: string]: JsonValue | undefined;
}
export interface SecondaryMotionChoices { enabled?: boolean; preset?: 'automatic'|'tangtang_r2'|'tangtang_reference'; roots?: string[]; profiles?: Record<string,'automatic'|'balanced'|'stiff'|'soft'>; part_kinds?:Record<string,'hair'|'streamer'|'tail'|'ear'|'cloth'|'long_skirt'|'accessory'|'breast'|'butt'|'charm'|'rigid'>; /** Fine-tuning per part type (hair, tail …): stiffness, damping, gravity, swing from -1 to 1, 0 = automatic. */tuning?:Record<string,Record<string,number>>;/** Start each moving part from the source rig's own MMD/VRM spring settings (preparation_options.physics_springs). */use_source_springs?:boolean; [key:string]: JsonValue|undefined }
export type MotionTuning = Partial<Record<'stiffness'|'damping'|'gravity'|'swing', number>>;
/** A named set of fine-tuning values saved on this PC. */
export interface PhysicsPreset { name: string; tuning: Record<string, Record<string, number>> }
export interface PhysicsChoices { enabled?: boolean; source?: string; hero_path?: string; [key: string]: JsonValue | undefined }
export interface ClothingChoices { enabled?: boolean; mode?: 'weighted' | 'simulated'; source?: string; [key: string]: JsonValue | undefined }
export interface ClothSetupChoices { meshes?: string[]; attachment?: 'top' | 'bottom'; collision?: 'legs' | 'none'; [key: string]: JsonValue | undefined }
export interface AppearanceMaterial {
  enabled?: boolean; target?: string; color?: [number,number,number]; roughness?: number;
  metalness?: number; opacity?: number; emission?: number;
  /** Experimental color editor: hue shift in degrees (-180–180); saturation and brightness factors (0–2, 1 keeps the original). */
  hue?: number; saturation?: number; brightness?: number; [key: string]: JsonValue | undefined;
}
export interface EquipmentMesh {
  target?: string; action?: 'keep' | 'hide' | 'replace'; sources?: string[]; include_lods?: boolean;
  [key: string]: JsonValue | undefined;
}
export interface PropChoices { enabled?: boolean; source?: string; bone?: string; translation?: [number,number,number]; rotation?: [number,number,number]; scale?: number; [key: string]: JsonValue | undefined }
export interface VariantChoices { enabled?: boolean; model?: string; project?: string; [key: string]: JsonValue | undefined }
export interface Customization {
  schema?: number; enabled?: boolean; model_enabled?: boolean; hero_path?: string;
  icons?: ResourceReplacement[]; animations?: ResourceReplacement[]; effects?: ResourceReplacement[]; audio?: ResourceReplacement[];
  secondary_motion?: SecondaryMotionChoices; physics?: PhysicsChoices; clothing?: ClothingChoices; cloth_setup?: ClothSetupChoices;
  appearance?: { enabled?: boolean; materials?: AppearanceMaterial[]; [key: string]: unknown };
  equipment?: { enabled?: boolean; meshes?: EquipmentMesh[]; props?: PropChoices[]; variants?: VariantChoices[]; [key: string]: unknown };
  face?: { enabled?: boolean; mode?: string; weights?: Record<string,number>; [key: string]: unknown };
  [key: string]: unknown;
}
