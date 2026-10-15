import type { Material, ModelSummary, Project } from '../models';
export interface InspectionReport {
  /** The movement preview's moving parts, also while mesh mapping is still open. */
  motion_preview_nodes?: {bone:string;root:string;kind:string;goal:number;drag:number;gravity:number;travel:number}[];
  secondary_motion?: {preset:string;source_sha256:string;proxy_sha256:string;groups:{source_root:string;root:string;anchor:string;bones:string[];weighted_bones:string[]}[];proxy_positions:number;parts?:{root:string;kind:string;reason:string;roll_locked:boolean;review_recommended:boolean}[];/** Each moving bone's motion values before Fine-tune, for the movement preview. */preview_nodes?:{bone:string;root:string;kind:string;goal:number;drag:number;gravity:number;travel:number}[];hair_motion?:{policy:string;shared_profile_groups:string[];custom_profile_groups:string[];review_groups:string[];length_adjusted_groups?:{root:string;strand_length_inches:number;short_hair_limit_inches:number;goal_strength_multiplier:number;scale_reference:string}[];jump_protected_groups?:{root:string;anchor:string;stretch_tethers:boolean}[]};long_skirt?:{policy:string;groups:{root:string}[];links:{bones:[string,string];supporting_render_triangles:number}[];stretch_tethers:boolean;body_collisions_authored:boolean;stability?:{bones:string[];hinges:[string,string,string][];seams?:[string,string][]}};reused_accepted_reference?:boolean}|null;
  project: string; hero_root: string; needs_review: boolean; warnings: string[];
  model: {meshes:{name:string;filename:string|null;lower_lod:boolean}[];lod_groups:number;attachments:number;ag2:unknown[]};
  dmx: Omit<ModelSummary,'path'>;
  geometry:{meshes:{name:string;vertices:number;polygons:number;triangles:number;corners:number}[];source_triangles:number};
  mappings:Record<string,string|string[]>; project_mappings:Record<string,string|string[]>;
  source_mappings:Record<string,string|string[]>; unresolved_meshes:string[]; unskinned_meshes:string[];
  materials:Record<string,Material>;
  vertex_preflight:{errors:string[];warnings:string[];count_scope:string;automatic_subdivision:boolean};
  automatic_split:unknown|null; source_sha256:string; original:unknown;
  source_identity?: { blocked?: boolean; identity?: { required?: boolean; passed?: boolean; errors?: string[] }; extension?: unknown };
}
export interface InspectionResult {project:Project;report:ProjectInspection}
export interface PreparedModel {content:string;game:string;model:string;generated_materials:string[];mesh_sources:Record<string,string>;dmx_sha256:string;automatic_split:unknown|null;tested_physics:unknown|null}
export interface PrepareResult {project:Project;prepared:PreparedModel}
export interface OriginalValidation {compiled:boolean;compiled_model?:string;replacement_dmx_used:boolean;packaged:boolean;installed:boolean;compiler_plan:unknown;original:unknown;toolchain_evidence?:unknown;validation_evidence?:unknown}

export interface BuildResult {prepared:string|null;compiled:boolean;vpk:string|null;installed:string|null;manifest:string|null}
export interface BuildResponse {project:Project;result:BuildResult;path:string}
export interface ComponentInspection {model_enabled:false;icons:unknown[];animations:unknown[];effects:unknown[];audio:unknown[];clothing:unknown|null;native_compile_required:boolean;warnings:string[];native_validated:false;in_game_validated:false}
export type ProjectInspection = InspectionReport | ComponentInspection;
