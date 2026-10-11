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
}
export type OperationStatus = 'running' | 'succeeded' | 'failed' | 'cancelled' | 'skipped' | 'cached';
export type StepStatus = OperationStatus;
export interface SetupStatus { files_found: Record<string,boolean>; model_status: string; model_files_found?: boolean }
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

export interface HeroRecord { name: string; path: string; likely_hero: boolean }
export type AssetKind = 'icons' | 'animations' | 'effects' | 'audio';
export type PathKind = 'texture_directory' | 'texture' | 'physics' | 'garment' | 'icon' | 'animation' | 'effect' | 'audio' | 'prop' | 'variant_project';
export interface ResourceReplacement {
  enabled?: boolean; source?: string; target?: string; allow_shared?: boolean;
  original_sha256?: string; fit?: 'contain' | 'cover' | 'stretch';
  volume_percent?: number; audio_fit?: 'exact' | 'pad' | 'trim';
  [key: string]: JsonValue | undefined;
}
export interface SecondaryMotionChoices { enabled?: boolean; preset?: 'automatic'|'tangtang_r2'|'tangtang_reference'; roots?: string[]; profiles?: Record<string,'automatic'|'balanced'|'stiff'|'soft'>; part_kinds?:Record<string,'hair'|'streamer'|'tail'|'ear'|'cloth'|'long_skirt'|'accessory'>; [key:string]: JsonValue|undefined }
export interface PhysicsChoices { enabled?: boolean; source?: string; hero_path?: string; [key: string]: JsonValue | undefined }
export interface ClothingChoices { enabled?: boolean; mode?: 'weighted' | 'simulated'; source?: string; [key: string]: JsonValue | undefined }
export interface ClothSetupChoices { meshes?: string[]; attachment?: 'top' | 'bottom'; collision?: 'legs' | 'none'; [key: string]: JsonValue | undefined }
export interface AppearanceMaterial {
  enabled?: boolean; target?: string; color?: [number,number,number]; roughness?: number;
  metalness?: number; opacity?: number; emission?: number; [key: string]: JsonValue | undefined;
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
