import type { ComponentCatalog } from './components/AssetCatalog';
import type { ExtraPreviewRequest } from './extras/models';
import type { PreparationAnalysis, PreparationOptions, PreparationResult, ResumedPreparation, ReviewIssue, Suggestions, Vec3, WeaponPreview } from './preparation/models';
import type { BuildResponse, InspectionResult, OriginalValidation, PrepareResult } from './build/models';
import { invoke, isTauri } from '@tauri-apps/api/core';
import type { Capabilities, ThemePreset, AnalysisResult, Autosave, ErrorHelp, FieldCheck, HeroRecord, LogEntry, PathKind, PhysicsChoices, PhysicsPreset, Project, ProjectDocument, Settings, SessionRecovery, SetupStatus, SetupTest, StorageClearRequest, StorageClearResult, StorageReport, TimingReport } from './models';

export const isDesktop = () => isTauri();
export interface ToolLogFile {path:string;bytes:number;kind:string}
export interface WeaponModel {path:string;target:string;label:string;hero:string;supported:boolean;requires_model_project:boolean;kind?:string}
export interface ToolLogPage {text:string;next_offset:number;eof:boolean}
let imageQueue:Promise<unknown>=Promise.resolve();
export interface VpkSlots {configured_slot:number;configured_slot_occupied:boolean;available_slot:number|null;occupied_slots:number[];previous_outputs:number[];scanned_folders:string[]}
export const api = {
  previewImage: (path:string,maxDimension=1024) => {const result=imageQueue.then(()=>invoke<ArrayBuffer>('preview_image',{path,maxDimension}));imageQueue=result.catch(()=>undefined);return result;},
  previewSource: (path:string) => invoke<ArrayBuffer>('preview_source',{path}),
  chooseFreeSlot: (project:Project) => invoke<{slot:number;filename:string;occupied_slots:number[];previous_outputs:number[];scanned_folders:string[]}>('choose_free_vpk_slot',{project}),
  /** Scan the game's addons folder and the project's output folder for VPK slots (fresh every call). */
  vpkSlots: (project:Project) => invoke<VpkSlots>('get_vpk_slots',{project}),
  /** Save the theme in settings.json at once and switch the window icon to its D (`icon`: original, violet or mint). */
  setTheme: (theme:string,icon?:string) => invoke<Settings>('set_theme',{theme,icon}),
  /** Save the custom themes (Settings → Appearance → Customize) in settings.json at once. */
  saveThemePresets: (presets:ThemePreset[]) => invoke<Settings>('save_theme_presets',{presets}),
  openOutput: (project:Project) => invoke<string>('open_output_folder',{project}),
  discoverComponents: (project:Project) => invoke<ComponentCatalog>('discover_components',{project}),
  discoverExtras: (project:Project) => invoke<ComponentCatalog>('discover_extras',{project}),
  previewExtra: (request:ExtraPreviewRequest) => invoke<ArrayBuffer>('preview_extra',{request}),
  toolLogs: () => invoke<ToolLogFile[]>('list_tool_logs'),
  toolLog: (path:string,offset=0,limit=65536) => invoke<ToolLogPage>('read_tool_log',{path,offset,limit}),
  buildProject: (project:Project,overwriteOutput=false,overwriteInstall=false,projectPath:string|null=null) => invoke<BuildResponse>('build_project',{project,overwriteOutput,overwriteInstall,projectPath}),
  installProject: (project:Project,path:string,overwrite=false) => invoke<string>('install_project',{project,path,overwrite}),
  preparationInspect: (project:Project,source:string,rotation:Vec3) => invoke<PreparationAnalysis>('preparation_inspect',{project,source,rotation}),
  preparationSuggest: (analysis:PreparationAnalysis,options:PreparationOptions) => invoke<Suggestions>('preparation_suggest',{analysis,options}),
  preparationSaveMappingPreset: (analysis:PreparationAnalysis,options:PreparationOptions) => invoke<void>('preparation_save_mapping_preset',{analysis,options}),
  preparationReview: (project:Project,analysis:PreparationAnalysis,options:PreparationOptions) => invoke<ReviewIssue[]>('preparation_review',{project,analysis,options}),
  preparationWeaponPreview: (project:Project,analysis:PreparationAnalysis,options:PreparationOptions) => invoke<WeaponPreview>('preparation_weapon_preview',{project,analysis,options}),
  discoverWeaponModels: (project:Project) => invoke<WeaponModel[]>('discover_weapon_models',{project}),
  preparationConvert: (project:Project,analysis:PreparationAnalysis,options:PreparationOptions,correction=false,correctionBlend?:string) => invoke<PreparationResult>('preparation_convert',{project,analysis,options,correction,correctionBlend}),
  preparationAccept: (project:Project,result:PreparationResult) => invoke<Project>('preparation_accept',{project,result}),
  preparationResume: (project:Project) => invoke<ResumedPreparation>('preparation_resume',{project}),
  preparationOpenBlend: (project:Project) => invoke<void>('preparation_open_blend',{project}),
  outfitSave: (project:Project,path:string,name:string) => invoke<void>('outfit_save',{project,path,name}),
  outfitLoad: (project:Project,path:string) => invoke<Project>('outfit_load',{project,path}),
  inspectProject: (project: Project) => invoke<InspectionResult>('inspect_project', { project }),
  prepareProject: (project: Project, offline:boolean) => invoke<PrepareResult>('prepare_project', { project, offline }),
  validateOriginal: (project: Project, compile:boolean) => invoke<OriginalValidation>('validate_original', { project, compile }),
  analyzeMaterials: (project: Project) => invoke<Project>('analyze_materials', { project }),
  importSourcePackage: (path: string, project: Project) => invoke<Project>('import_source_package', { path, project }),
  detectSettings: (settings: Settings, extraRoots: string[] = []) => invoke<Settings>('detect_settings', { settings, extraRoots }),
  setupStatus: (settings: Settings) => invoke<SetupStatus>('get_setup_status', { settings }),
  /** What this build includes; the Physics page hides the TangTang presets when they are not compiled in. */
  capabilities: () => invoke<Capabilities>('get_capabilities'),
  checkSettingFields: (settings: Settings) => invoke<FieldCheck[]>('check_setting_fields', { settings }),
  heroProportions: (path: string) => invoke<HeroProportions | null>('hero_proportions', { path }),
  prebuildWarnings: () => invoke<string[]>('prebuild_warnings'),
  operationDurations: () => invoke<Record<string, number>>('operation_durations'),
  fixSettingFields: (settings: Settings) => invoke<Settings>('fix_setting_fields', { settings }),
  /** Autosave and recovery (plain file writes; never blocks other work). */
  autosaveProject: (document: ProjectDocument) => invoke<Autosave>('autosave_project', { document }),
  clearAutosaves: (id: string) => invoke<void>('clear_autosaves', { id }),
  sessionRecovery: () => invoke<SessionRecovery>('session_recovery'),
  dismissRecovery: () => invoke<void>('dismiss_recovery'),
  restoreAutosave: (id: string, file: string) => invoke<ProjectDocument>('restore_autosave', { id, file }),
  /** Saved physics presets (fine-tuning per part type, on this PC). */
  physicsPresets: () => invoke<PhysicsPreset[]>('physics_presets'),
  savePhysicsPreset: (name: string, tuning: Record<string, Record<string, number>>) => invoke<PhysicsPreset[]>('save_physics_preset', { name, tuning }),
  deletePhysicsPreset: (name: string) => invoke<PhysicsPreset[]>('delete_physics_preset', { name }),
  /** Compiles one test material and asks the model compiler for its format (about 10 seconds). */
  testSetup: (settings: Settings) => invoke<SetupTest>('test_setup', { settings }),
  /** Movement preview packet: prepared skeleton and the hero's stand, run and jump clips. */
  motionPreview: (project: Project) => invoke<ArrayBuffer>('motion_preview', { project }),
  preview: (path: string) => invoke<ArrayBuffer>('preview_dmx', { path }),
  heroes: (allModels = false) => invoke<HeroRecord[]>('get_heroes', { allModels }),
  selectHero: (project: Project, path: string) => invoke<Project>('select_hero', { project, path }),
  includedPhysics: (heroPath: string) => invoke<PhysicsChoices>('included_physics', { heroPath }),
  validateAssetPath: (path: string, kind: PathKind) => invoke<string>('validate_asset_path', { path, kind }),
  settings: () => invoke<Settings>('get_settings'),
  saveSettings: (settings: Settings) => invoke<void>('save_settings', { settings }),
  newProject: () => invoke<ProjectDocument>('new_project'),
  openProject: (path: string) => invoke<ProjectDocument>('open_project', { path }),
  saveProject: (path: string, project: Project) => invoke<ProjectDocument>('save_project', { path, project }),
  analyze: (path: string) => invoke<AnalysisResult>('analyze_dmx', { path }),
  cancel: () => invoke<void>('cancel_operation'),
  latestTimings: () => invoke<TimingReport | null>('get_timing_report'),
  timings: (path: string) => invoke<TimingReport>('read_timing_report', { path }),
  logs: () => invoke<LogEntry[]>('get_log_entries'),
  logText: () => invoke<string>('get_log_text'),
  saveLog: (path: string) => invoke<void>('save_log', { path }),
  storageReport: (project: Project | null) => invoke<StorageReport>('storage_report', { project }),
  storageClear: (request: StorageClearRequest, project: Project | null) => invoke<{result: StorageClearResult; report: StorageReport}>('storage_clear', { request, project }),
  storageFindProjects: (folder: string) => invoke<number>('storage_find_projects', { folder }),
};

/** Arm and leg lengths of a hero as shares of its pelvis-to-head length (backend hero_fit.rs). */
export interface HeroProportions { extremes: ('arms'|'legs')[]; arm_left: number; arm_right: number; legs: number }
/** Hide the Windows user name in text people copy for support (C:\Users\name becomes C:\Users\<you>). */
export function scrubUser(text: string): string {
  return text.replace(/([A-Za-z]:[\\/]+Users[\\/]+)(?!<you>)[^\\/\s"':;,]+/gi, '$1<you>');
}
/** The troubleshooting help the backend attached to an error, if any. */
export function errorHelp(error: unknown): ErrorHelp | null {
  if (typeof error !== 'object' || error === null || !('help' in error)) return null;
  const help = (error as {help?: unknown}).help;
  return help && typeof help === 'object' && Array.isArray((help as ErrorHelp).steps) ? help as ErrorHelp : null;
}
export function errorText(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === 'string') return error;
  return JSON.stringify(error, null, 2) ?? 'Unknown error';
}
export function basename(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).at(-1) ?? path;
}
