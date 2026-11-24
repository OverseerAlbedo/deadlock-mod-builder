import type { ComponentCatalog } from './components/AssetCatalog';
import type { ExtraPreviewRequest } from './extras/models';
import type { FitReport, MotionCatalog } from './motion/motionTypes';
import type { EffectEdit, EffectPickRequest, EffectPreview, EffectTrees } from './extras/effectTypes';
import type { PlayerAnswer, PlayerControl, PlayerState, PlayerView } from './extras/effectPlayerTypes';
import type { PreparationAnalysis, PreparationOptions, PreparationResult, ResumedPreparation, ReviewIssue, Suggestions, Vec3, WeaponPreview } from './preparation/models';
import type { BuildResponse, InspectionResult, OriginalValidation, PrepareResult } from './build/models';
import { invoke, isTauri } from '@tauri-apps/api/core';
import type { HeroCopy } from './models';
import type { Capabilities, InstalledMod, ModUpdateReport, ThemePreset, AnalysisResult, Autosave, ErrorHelp, FieldCheck, HeroRecord, LogEntry, PathKind, PhysicsChoices, PhysicsPreset, Project, ProjectDocument, Settings, SessionRecovery, SetupStatus, SetupTest, StorageClearRequest, StorageClearResult, StorageReport, TimingReport } from './models';

export const isDesktop = () => isTauri();
export interface ToolLogFile {path:string;bytes:number;kind:string}
export interface WeaponModel {path:string;target:string;label:string;hero:string;supported:boolean;requires_model_project:boolean;kind?:string}
export interface ToolLogPage {text:string;next_offset:number;eof:boolean}
let imageQueue:Promise<unknown>=Promise.resolve();
export interface VpkSlots {configured_slot:number;configured_slot_occupied:boolean;configured_slot_own_install?:boolean;available_slot:number|null;occupied_slots:number[];previous_outputs:number[];scanned_folders:string[]}
export const api = {
  previewImage: (path:string,maxDimension=1024) => {const result=imageQueue.then(()=>invoke<ArrayBuffer>('preview_image',{path,maxDimension}));imageQueue=result.catch(()=>undefined);return result;},
  /** Size and modification time of each image (null where preview_image could not read it): tells the preview which textures changed. */
  fileStamps: (paths:string[]) => invoke<({len:number;modified_ms:number}|null)[]>('file_stamps',{paths}),
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
  /** Extras → Sounds editor: a replacement row rendered exactly as the build writes it (WAV body) with waveform peaks of the file, the result and the game's sound (`src/extras/soundEdit.ts`). No lease; the newest call wins (older ones fail with `superseded`). Needs `discoverExtras` first. */
  soundEditPreview: (request:import('./extras/soundEdit').SoundEditRequest) => invoke<ArrayBuffer>('sound_edit_preview',{request}),
  /** The game's items for Extras → Items, read when that tab opens and kept per game archive; `reload` reads them again. Pictures come from `previewExtra` with kind `items`. */
  itemCatalog: (reload = false) => invoke<import('./extras/itemCatalog').ItemCatalog>('item_catalog',{reload}),
  /** Extras → Items, renamed items: the game's item names (token → English name), the languages a build writes, and each saved row's state (`src/extras/itemNames.ts`). No lease; kept per game names folder. */
  itemNamesState: (rows: import('./extras/itemNames').ItemNameRow[]) => invoke<import('./extras/itemNames').ItemNamesState>('item_names_state',{rows}),
  /** Extras → Objects: the hero's ability objects (`src/extras/objectTypes.ts`), read when the page opens and kept per game archive and hero; no lease. `reload` reads them again. */
  abilityObjectCatalog: (project:Project, reload = false) => invoke<import('./extras/objectTypes').ObjectCatalog>('ability_object_catalog',{project,reload}),
  /** Prepares the user's file for one offered ability object (stages it and runs Blender; takes the operation lease). A current preparation of the same file is reused at once. */
  prepareAbilityObject: (project:Project, target:string, source:string) => invoke<import('./extras/objectTypes').PreparedObject>('prepare_ability_object',{project,target,source}),
  /** The fit view: `[u32 header length][{mime_type, details: ObjectFitView}][data]` with the game object's meshes and (with `source`) the user's prepared preview mesh as preview packets in the game object's frame, plus the fit's transform. The first call per game object decompiles it (operation lease); later calls take no lease. */
  abilityObjectFit: (project:Project, target:string, source:string|null, fit:import('./extras/objectTypes').ObjectFit) => invoke<ArrayBuffer>('ability_object_fit',{project,target,source,fit}),
  abilityObjectMotion: (project:Project, target:string, clip:string|null) => invoke<ArrayBuffer>('ability_object_motion',{project,target,clip}),
  /** The game's own hero as a preview packet (`src/preview/packet.ts`) for Extras → Colors: LOD 0 meshes without skin, bones or vertex colours; each mesh adds `target` (its Colors row's `.vmat_c` or null), `node`, `body_group`, `default_hidden`, `hidden_reason` and `weapon` (true only for parts of the hero's weapon); the metadata adds `material_groups`, `rows_without_mesh` and `materials_without_row`. Needs `discoverExtras` first. */
  heroScene: (project:Project) => invoke<ArrayBuffer>('hero_scene',{project}),
  /** The prepared replacement model (`project.dmx`) for Extras → Colors: the ordinary preview packet, with `weapon: true` on the meshes of the fitted gun the preparation receipt names. */
  replacementColorScene: (project:Project) => invoke<ArrayBuffer>('replacement_color_scene',{project}),
  /** The hero's effect trees for Extras → Effects (groups, parts, color strips, palette, main color, "Also used by"). Needs `discoverExtras` first; takes the operation lease and is slow only the first time per loaded catalog. */
  effectTrees: (project:Project) => invoke<EffectTrees>('effect_trees',{project}),
  /** One effect before and after an edit (fast, no lease; call while a slider moves). `main`: the main color the user chose (the answer's `main` and a pick use it). With `pick`, the edit's hue, saturation and brightness become the shift that maps the main color (`pick.main`, else `main`, else the detected one) onto `pick.color`. Needs `effectTrees` first. */
  effectPreview: (project:Project,target:string,edit:EffectEdit,pick?:EffectPickRequest|null,main?:[number,number,number]|null) => invoke<EffectPreview>('effect_preview',{project,target,edit,pick:pick??null,main:main??null}),
  /** Extras → Effects player: plays `target` with the page's current edit (saved or not); starts the helper when needed. Never takes the operation lease. */
  effectPlayerPlay: (project:Project,target:string,edit:EffectEdit,view:PlayerView|null) => invoke<PlayerAnswer>('effect_player_play',{project,target,edit,view}),
  /** Updates "yours" while the user edits (the newest call wins; older ones answer `superseded`). */
  effectPlayerEdit: (project:Project,target:string,edit:EffectEdit) => invoke<PlayerAnswer>('effect_player_edit',{project,target,edit}),
  effectPlayerControl: (control:PlayerControl) => invoke<PlayerAnswer>('effect_player_control',{control}),
  effectPlayerStop: () => invoke<void>('effect_player_stop'),
  effectPlayerState: () => invoke<PlayerState>('effect_player_state'),
  /** The newest frame after `after` as raw bytes (`effectPlayerTypes.parseFrame`); an empty buffer when none came within `waitMs`. */
  effectPlayerFrame: (after:number,waitMs=500) => invoke<ArrayBuffer>('effect_player_frame',{after,waitMs}),
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
  /** Every VPK in the game's addons folder with the state of its hero models (game update of October 16, 2026). */
  scanInstalledMods: () => invoke<InstalledMod[]>('scan_installed_mods'),
  /** Rewrite the hero models of these installed VPKs to the game's current attachment convention (backups kept). */
  updateInstalledMods: (paths: string[]) => invoke<ModUpdateReport[]>('update_installed_mods', { paths }),
  /** Keep a label for the installed VPK with this content identity (app state only; empty removes it). */
  setInstalledModLabel: (identity: string, file: string, label: string) => invoke<string | null>('set_installed_mod_label', { identity, file, label }),
  /** Open the game's addons folder, or show one of its files selected in Explorer. */
  showInstalledMod: (path?: string) => invoke<void>('show_installed_mod', { path: path ?? null }),
  /** Show a saved project file this app knows, selected in Explorer. */
  showProjectFile: (path: string) => invoke<void>('show_project_file', { path }),
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
  /** The movement preview's cloth packet the last Analyze wrote for this proxy (JSON text, `src/preview/clothPacket.ts`), or null when there is none (an older Analyze, a preset, another mode). Reads one file; no operation lease. */
  previewCloth: (project: Project, proxySha256: string) => invoke<string | null>('preview_cloth', { project, proxySha256 }),
  /** Motion page: the hero's clips (lease-free, kept per game archive and hero). */
  motionCatalog: (project: Project, reload = false) => invoke<MotionCatalog>('motion_catalog', { project, reload }),
  /** Motion page: whether a replacement file fits the hero's clip, as the build's checks (read-only, no lease). */
  clipFit: (project: Project, target: string, source: string) => invoke<FitReport>('clip_fit', { project, target, source }),
  /** Motion page: one clip on the prepared character (`which` game, or yours with `source`), binary. */
  clipPreview: (project: Project, target: string, which: 'game' | 'yours', source?: string) => invoke<ArrayBuffer>('clip_preview', { project, target, which, source: source ?? null }),
  /** Motion → Make it from my animation: whether this clip can be made from an animation and whether the prepared model can be used (read-only, no lease). */
  madeClipCheck: (project: Project, target: string) => invoke<import('./motion/makeTypes').MadeClipCheck>('made_clip_check', { project, target }),
  /** The actions of a Blender file and whether its skeleton is the prepared one (runs Blender; operation lease). */
  madeClipActions: (project: Project, source: string) => invoke<import('./motion/makeTypes').ActionList>('made_clip_actions', { project, source }),
  /** Samples the row's action in Blender (operation lease; reused while the file, action, mode and prepared model are unchanged) and makes the clip once as the build does. `row` is the animation row with `made`. */
  makeClip: (project: Project, target: string, row: import('./models').ResourceReplacement) => invoke<import('./motion/makeTypes').MadeClipAnswer>('make_clip', { project, target, row }),
  /** The report of the clip a row makes (no Blender, no lease; the newest request wins, an older one fails with MADE_SUPERSEDED). */
  madeClipReport: (project: Project, target: string, row: import('./models').ResourceReplacement) => invoke<import('./motion/makeTypes').MadeReport>('made_clip_report', { project, target, row }),
  /** The clip a row makes, played on the prepared character: the same binary packet as `clipPreview` (`which` "made"). No Blender, no lease; newest wins. */
  madeClipPreview: (project: Project, target: string, row: import('./models').ResourceReplacement) => invoke<ArrayBuffer>('made_clip_preview', { project, target, row }),
  /** Motion → Take it from another hero's animation: every roster hero but this one, each with whether its body can be carried onto this hero (lease-free, kept per game archive). */
  takenClipHeroes: (project: Project) => invoke<import('./motion/takenTypes').SourceHero[]>('taken_clip_heroes', { project }),
  /** Whether this hero's clip `target` can be taken from another hero at all: `refusal` is the engine's sentence when it cannot (a layer, or a clip stored in a form the app does not write). Lease-free. */
  takenClipCheck: (project: Project, target: string) => invoke<import('./motion/takenTypes').TakenClipCheck>('taken_clip_check', { project, target }),
  /** The other hero's clips for this hero's clip `target` (same name first, then group and direction) and the set it belongs to (lease-free). `hero` is the other hero's model path. */
  takenClipSources: (project: Project, target: string, hero: string) => invoke<import('./motion/takenTypes').TakenSources>('taken_clip_sources', { project, target, hero }),
  /** The report of the clip a taken row makes, as the build makes it (no lease; the newest request wins, an older one fails with TAKEN_SUPERSEDED). `row` is the animation row with `taken`. */
  takenClipReport: (project: Project, target: string, row: import('./models').ResourceReplacement) => invoke<import('./motion/takenTypes').TakenReport>('taken_clip_report', { project, target, row }),
  /** The clip a taken row makes, played on the prepared character: the same binary packet as `madeClipPreview` (`which` "taken"). No lease; newest wins. */
  takenClipPreview: (project: Project, target: string, row: import('./models').ResourceReplacement) => invoke<ArrayBuffer>('taken_clip_preview', { project, target, row }),
  preview: (path: string) => invoke<ArrayBuffer>('preview_dmx', { path }),
  heroes: (allModels = false) => invoke<HeroRecord[]>('get_heroes', { allModels }),
  selectHero: (project: Project, path: string) => invoke<Project>('select_hero', { project, path }),
  /** Make a copy for another hero: a new, unsaved project (`deadlock_project::hero_copy`). */
  copyProjectForHero: (project: Project, path: string) => invoke<HeroCopy>('copy_project_for_hero', { project, path }),
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
