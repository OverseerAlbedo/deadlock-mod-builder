// i18n-area: build
import { basename } from '../api';
import { heroLabel, shownPickerLabel } from '../components/HeroPicker';
import type { StateTone } from '../components/Primitives';
import type { Project } from '../models';
import { m, t, tp } from '../i18n';
import type { InspectionReport, ProjectInspection } from './models';
import { nameList } from './words';

export type ReviewDestination = 'project' | 'appearance' | 'physics' | 'motion' | 'extras' | 'build';
/** A row's `label`, `action` and `status` and an issue's `action` stay English (they are compared
 * and read as names); they are translated where they are drawn. `value`, `text`, `title` and
 * `description` are only ever drawn and come translated. */
export interface ReadinessRow { key: string; label: string; value: string; tone: StateTone; page?: ReviewDestination; action?: string; section?: string; status?: string }
export interface ReviewIssue { text: string; page?: ReviewDestination; section?: string; action?: string;
  /** A second way on, beside `action`: build the extras without a replacement model (`model_enabled: false`). */
  withoutModel?: string }
/** Build & install's second action when no replacement model is chosen; the same switch as Extras → Colors' "Recolor the game's own … instead". */
export const BUILD_WITHOUT_MODEL = m('Build the extras without a replacement model');
export const modelEnabled = (project: Project) => !project.customization.enabled || project.customization.model_enabled !== false;
/** Enabled hero colors (Extras → Colors) that the next build includes. */
export const heroColorCount = (project: Project): number => !project.customization.enabled || project.customization.hero_materials?.enabled === false ? 0 : (project.customization.hero_materials?.materials ?? []).filter(row => row.enabled !== false).length;
export const testedPhysicsEnabled =(project: Project) => Boolean(modelEnabled(project) && project.customization.enabled && project.customization.physics?.enabled);

/** The VPK slot scan (`api.vpkSlots`) as far as readiness needs it: refreshed at
 * every Analyze, so a slot another mod took is reported before the build. */
export interface SlotScan { configured_slot: number; configured_slot_occupied: boolean; available_slot: number | null }

/** An item of the review list. `open` blocks the build; `done` was on the list of the
 * last analysis and is handled now; `changed` is a result only the backend can decide
 * (mesh mappings, vertex limits) whose inputs changed since: Build and Analyze check it
 * again. Only a kept analysis (see `keptAnalysis`) has `done` and `changed` items. */
export type ReviewState = 'open' | 'done' | 'changed';
export interface ReviewItem extends ReviewIssue { key: string; state: ReviewState }

/** The analysis behind a report that is no longer current: the project as it was
 * analyzed, whether a build has checked the live project since (`verified`), and
 * whether a build since refused because something still needs a choice (`refused`,
 * cleared by the next Analyze). */
export interface PreviousAnalysis { analyzed: Project | null; verified?: boolean; refused?: boolean }

/** The build's own check refusing what still needs review (`build.rs build_owned`).
 * Keep in step with the matching rule in `src-tauri/src/troubleshooting.rs`. */
export const BUILD_REFUSALS = ['review mesh mappings, materials, lods and unskinned meshes before building', 'model compilation stopped by vertex preflight'] as const;
/** Whether a failed build was that refusal, matched on message and details like the help rule. */
export function buildRefused(error: unknown): boolean {
  if (!error || typeof error !== 'object') return false;
  const { message, details } = error as { message?: unknown; details?: unknown };
  const text = `${typeof message === 'string' ? message : ''}\n${typeof details === 'string' ? details : ''}`.toLowerCase();
  return BUILD_REFUSALS.some(phrase => text.includes(phrase));
}

/** Key-order independent comparison of project values (an edit may rebuild an object). */
function canonical(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`;
  if (value && typeof value === 'object') return `{${Object.keys(value as Record<string, unknown>).sort().filter(key => (value as Record<string, unknown>)[key] !== undefined).map(key => `${JSON.stringify(key)}:${canonical((value as Record<string, unknown>)[key])}`).join(',')}}`;
  return JSON.stringify(value ?? null);
}
const same = (a: unknown, b: unknown) => canonical(a) === canonical(b);

/** A report stays the working list after project edits while it is for the same
 * replacement model, hero and mode (the model switch) as the live project. */
export function keptAnalysis(project: Project, analyzed: Project | null | undefined): boolean {
  if (!analyzed) return false;
  const model = modelEnabled(project);
  return model === modelEnabled(analyzed) && project.original_model === analyzed.original_model && (!model || project.dmx === analyzed.dmx);
}

/** The inputs of Analyze's mesh mapping decision (`suggest_mappings`, then the
 * equipment choices): any change means only the next check can tell. */
const mappingInputs = (p: Project) => [p.meshes, p.lod_strategy, p.auto_map, p.customization.enabled, p.customization.equipment];
const preflightInputs = (p: Project) => [...mappingInputs(p), p.auto_split, p.split_target];
const leftOutInputs = (p: Project) => [...mappingInputs(p), p.preparation_options.preserve_physics_bones, p.customization.model_enabled, p.customization.secondary_motion?.enabled];
/** Only the fields of an issue (items carry their key, state and inputs besides). */
const asIssue = ({ text, page, section, action }: ReviewIssue): ReviewIssue => Object.fromEntries(Object.entries({ text, page, section, action }).filter(([, value]) => value !== undefined)) as unknown as ReviewIssue;
const materialReviewCount = (materials: Project['materials']) => Object.values(materials).filter(material => material.mode === 'generate' && (!material.confirmed || Boolean(material.ambiguous?.length))).length;

/** The review items of a model report for `project`. Project flags (LOD strategy and
 * acknowledgement, unskinned acknowledgement) and `materials` are read as given;
 * backend results (vertex preflight, unresolved and left-out meshes) as reported.
 * `backend` gives the inputs of a result only the backend decides; `acknowledged`
 * marks such an item that a project flag still ticks off. */
type Draft = ReviewItem & { backend?: (p: Project) => unknown[]; acknowledged?: boolean };
function reviewItems(inspection: InspectionReport, project: Project, materials: Project['materials']): Draft[] {
  const items: Draft[] = [];
  const open = 'open' as const;
  inspection.vertex_preflight.errors.forEach((text, index) => items.push({ key: `preflight:${index}`, state: open, backend: preflightInputs, text, page: 'appearance', section: 'model-processing', action: m('Configure meshes') }));
  const unresolved = inspection.unresolved_meshes.length;
  if (unresolved) items.push({ key: 'unresolved', state: open, backend: mappingInputs, text: tp(unresolved, { one: '{count} model mapping need a choice.', other: '{count} model mappings need a choice.' }), section: 'mesh-mapping', action: m('Configure mappings') });
  const leftOut = inspection.secondary_meshes_left_out ?? [];
  if (leftOut.length) items.push({ key: 'left-out', state: open, backend: leftOutInputs, text: tp(leftOut.length, { one: 'Moving-part bones kept from your rig are only in meshes left out of the build: {names}. Add it in Mesh mapping.', other: 'Moving-part bones kept from your rig are only in meshes left out of the build: {names}. Add them in Mesh mapping.' }, { names: nameList(leftOut) }), section: 'mesh-mapping', action: m('Configure mappings') });
  const materialReview = materialReviewCount(materials);
  if (materialReview) items.push({ key: 'materials', state: open, text: tp(materialReview, { one: '{count} material assignment need review.', other: '{count} material assignments need review.' }), page: 'appearance', section: 'materials', action: m('Configure materials') });
  if (project.lod_strategy === 'choose' || ((inspection.model.lod_groups > 1 || inspection.geometry.source_triangles > 500000) && !project.review_lods)) items.push({ key: 'lod', state: open, text: t('Review the detail-level strategy and acknowledge its appearance and performance tradeoffs.'), section: 'build-settings', action: m('Configure detail levels') });
  // The frozen list depends on the selected meshes: only the acknowledgement ticks it
  // off, and changed mappings leave it to the next check.
  if (inspection.unskinned_meshes.length && inspection.dmx.bones.length && !project.allow_unskinned) items.push({ key: 'unskinned', state: open, backend: mappingInputs, acknowledged: true, text: t('Review meshes without skin weights in Model processing.'), page: 'appearance', section: 'model-processing', action: m('Configure meshes') });
  return items;
}

/** The list of the last analysis against the live project: items of the analyzed
 * project in their order, each open, done or changed, then items that are open now
 * but were not then (a LOD strategy changed, a material no longer confirmed). */
function keptItems(inspection: InspectionReport, analyzed: Project, project: Project): ReviewItem[] {
  const then = reviewItems(inspection, analyzed, inspection.materials);
  const now = reviewItems(inspection, project, project.materials);
  const live = new Map(now.map(item => [item.key, item]));
  const strip = (item: ReviewItem): ReviewItem => ({ ...asIssue(item), key: item.key, state: item.state });
  const items = then.map(item => {
    const current = live.get(item.key);
    if ((!item.backend || item.acknowledged) && !current) return strip({ ...item, state: 'done' });
    if (item.backend) return strip({ ...item, state: same(item.backend(analyzed), item.backend(project)) ? 'open' : 'changed' });
    return strip(current!);
  });
  now.filter(item => !then.some(old => old.key === item.key)).forEach(item => items.push(strip(item)));
  return items;
}

/** Describes native inspection and saved choices. It never substitutes for a build gate.
 * `replaceInstalled` is Build's "Replace an existing installed VPK" choice: with it, a
 * slot another mod uses is noted but does not block. `previous` (Build & install only)
 * keeps the list of a report that is not current as a working list (`keptAnalysis`);
 * without it a report that is not current only asks for Analyze, as before. */
export function projectReadiness(project: Project, report: ProjectInspection | null, fresh: boolean, slots: SlotScan | null = null, replaceInstalled = false, previous: PreviousAnalysis | null = null) {
  const model = modelEnabled(project);
  const matching = report && (model ? 'dmx' in report : 'model_enabled' in report);
  const current = fresh && matching ? report : null;
  const inspection = current && 'dmx' in current ? current : null;
  // Model reports only: an assets-only build does not analyze the project again first.
  const keptReport = !current && matching && model && report && 'dmx' in report && keptAnalysis(project, previous?.analyzed) ? report : null;
  const kept = Boolean(keptReport);
  const verified = kept && Boolean(previous?.verified);
  // The last build's own check said something still needs a choice: Analyze shows what.
  const refused = kept && Boolean(previous?.refused);
  const hasInputs = Boolean(project.original_model && (!model || project.dmx));
  const issues: ReviewIssue[] = [];
  const items: ReviewItem[] = [];
  if (!project.original_model) issues.push({ text: t('Choose the character you want to replace.'), page: 'project', section: 'character-selection', action: m('Choose character') });
  if (model && !project.dmx) issues.push({ text: t('Add a replacement model.'), page: 'appearance', section: 'replacement-model', action: m('Add model'), withoutModel: BUILD_WITHOUT_MODEL });
  const materials = inspection?.materials ?? project.materials;
  const materialReview = materialReviewCount(materials);
  if (inspection) {
    reviewItems(inspection, project, materials).forEach(item => issues.push(asIssue(item)));
    if (inspection.needs_review && !issues.length) issues.push({ text: t('Review the native analysis notes before building.'), section: 'analysis-details', action: m('View analysis notes') });
  }
  // A kept report's frozen needs_review adds nothing: every reason for it is an item.
  if (keptReport && previous?.analyzed) {
    // A build checked the live project since: what only the backend decides passed.
    keptItems(keptReport, previous.analyzed, project).forEach(item => items.push(verified && item.state === 'changed' ? { ...item, state: 'done' } : item));
    items.filter(item => item.state === 'open').forEach(item => issues.push(asIssue(item)));
  }
  const packaging = project.build_vpk && (!model || project.compile_model);
  const invalidSlot = packaging && (!Number.isInteger(project.vpk_number) || project.vpk_number < 2 || project.vpk_number > 99);
  if (invalidSlot) issues.push({text:t('Choose a VPK slot from 2 to 99 before packaging.'),section:'output-settings',action:m('Configure output')});
  const slotTaken = Boolean(packaging && !invalidSlot && slots && slots.configured_slot === project.vpk_number && slots.configured_slot_occupied);
  if (slotTaken && !replaceInstalled) issues.push({text:slots?.available_slot ? t("VPK slot {slot} is in use by another mod in the game's addons folder (slot {free} is free).", {slot: project.vpk_number, free: slots.available_slot}) : t("VPK slot {slot} is in use by another mod in the game's addons folder.", {slot: project.vpk_number}),section:'output-settings',action:m('Choose free slot')});
  // Output issues are live; on a kept list they are items like the others.
  if (kept) issues.slice(issues.length - Number(Boolean(invalidSlot)) - Number(slotTaken && !replaceInstalled)).forEach((issue, index) => items.push({ ...issue, key: `output:${index}`, state: 'open' }));
  // A kept analysis with nothing open may build: the build analyzes the project again
  // first and refuses what still needs review (`build_owned`), so `changed` items do not block.
  const ready = Boolean(hasInputs && !issues.length && (current ? !inspection || (!inspection.needs_review && !inspection.vertex_preflight.errors.length) : kept && !refused));
  const open = issues.length;
  const physics = project.customization.physics;
  const tested = testedPhysicsEnabled(project);
  const generated = Boolean(model&&project.customization.enabled&&project.customization.secondary_motion?.enabled);
  const animations = project.customization.enabled ? (project.customization.animations ?? []).filter(row => row.enabled !== false).length : 0;
  const heroColors = heroColorCount(project);
  const skeleton = inspection?.source_identity;
  const deferredSkeleton = skeleton?.identity?.required && !skeleton.identity.passed && !skeleton.extension;
  const keptCheck = verified ? t('Project changed since the last analysis · the last build checked it') : t('Project changed since the last analysis · Build checks it again first');
  const assignments = Object.keys(materials).length, mappings = Object.keys(inspection?.mappings ?? project.meshes).length;
  const processingIssues = inspection?.vertex_preflight.errors.length ?? 0;
  const motionName = project.customization.secondary_motion?.preset==='tangtang_r2' ? t('TangTang R2 tuned') : t('Automatic secondary motion');
  const slotNote = slotTaken ? replaceInstalled ? t('slot in use by another mod, will be replaced') : t('slot in use by another mod') : slots && slots.configured_slot === project.vpk_number ? t('slot free') : '';
  const rows: ReadinessRow[] = [
    { key: 'character', label: m('Character'), value: project.original_model ? shownPickerLabel(heroLabel({ name: project.hero_name, path: project.original_model, likely_hero: true })) : t('Choose a character'), tone: project.original_model ? 'ready' : 'attention', page: 'project' },
    { key: 'model', label: m('Replacement model'), value: !model ? t('Original model kept · extras only') : project.dmx ? basename(project.dmx) : t('Add a model'), tone: !model ? 'neutral' : project.dmx ? 'ready' : 'attention', page: 'appearance' },
    { key: 'skeleton', label: m('Compatibility'), value: !model ? current ? t('Extra assets inspected · build validates outputs') : t('Analyze extra assets before building') : kept ? keptCheck : !inspection ? t('Analyze to check compatibility') : deferredSkeleton ? t('Compiled skeleton check required') : t('Source preflight complete'), tone: !current || deferredSkeleton ? 'neutral' : 'ready', page: 'build', status: kept ? verified ? m('Checked by the last build') : m('Build checks again') : !current ? m('Not analyzed') : deferredSkeleton ? m('Build check required') : m('Ready') },
    { key: 'materials', label: m('Materials'), value: !model ? heroColors ? tp(heroColors, { one: '{count} hero color · game textures kept', other: '{count} hero colors · game textures kept' }) : t('Original materials kept') : materialReview ? tp(materialReview, { one: '{count} need review', other: '{count} need review' }) : inspection ? tp(assignments, { one: '{count} assignment checked', other: '{count} assignments checked' }) : tp(assignments, { one: '{count} assignment saved', other: '{count} assignments saved' }), tone: !model ? 'neutral' : materialReview ? 'attention' : inspection ? 'ready' : 'neutral', page: 'appearance' },
    { key: 'mapping', label: m('Model mapping'), value: !model ? t('No model replacement') : inspection?.unresolved_meshes.length ? tp(inspection.unresolved_meshes.length, { one: '{count} unresolved', other: '{count} unresolved' }) : inspection ? tp(mappings, { one: '{count} mapping checked', other: '{count} mappings checked' }) : tp(mappings, { one: '{count} mapping saved', other: '{count} mappings saved' }), tone: !model ? 'neutral' : inspection?.unresolved_meshes.length ? 'attention' : inspection ? 'ready' : 'neutral', page: 'build' },
    { key: 'processing', label: m('Mesh processing'), value: !model ? t('No model processing') : processingIssues ? tp(processingIssues, { one: '{count} processing issue', other: '{count} processing issues' }) : project.auto_split ? t('Automatic splitting enabled') : t('Automatic splitting disabled'), tone: inspection?.vertex_preflight.errors.length ? 'attention' : model && (project.auto_split || inspection) ? 'ready' : 'neutral', page: 'appearance' },
    { key: 'physics', label: m('Physics'), value: !model ? t('Original physics kept') : generated ? inspection?.secondary_motion ? t('{name} · groups checked', { name: motionName }) : motionName : tested ? physics?.source ? inspection ? t('Replacement cloth setup · reference checked') : t('Replacement cloth setup selected') : t('Choose a tested reference') : t('Preserve original physics'), tone: generated ? inspection?.secondary_motion?'ready':'neutral' : tested && !physics?.source ? 'attention' : tested && !inspection ? 'neutral' : 'ready', page: 'physics' },
    { key: 'motion', label: m('Motion'), value: animations ? tp(animations, { one: '{count} animation replacement enabled', other: '{count} animation replacements enabled' }) : project.animation_policy === 'preserve' ? t('Original animations preserved') : t('Legacy animation references disabled'), tone: animations || project.animation_policy !== 'preserve' ? 'neutral' : 'ready', page: 'motion' },
    { key: 'output', label: m('Output'), value: invalidSlot ? t('Select a VPK slot (2–99)') : model && !project.compile_model ? t('Source preparation only') : !project.build_vpk ? t('Compile only · no VPK') : [`pak${String(project.vpk_number).padStart(2, '0')}_dir.vpk`, project.install ? t('build and install') : t('build only'), slotNote].filter(Boolean).join(' · '), tone: invalidSlot || (slotTaken && !replaceInstalled) ? 'attention' : 'neutral', status: invalidSlot || (slotTaken && !replaceInstalled) ? m('Needs attention') : m('Configured'), page: 'build' },
  ];
  const actions: Record<string, [string, string?]> = {
    character: [project.original_model ? m('Change') : m('Choose'), 'character-selection'],
    model: [project.dmx ? m('Change') : model ? m('Add model') : m('View'), 'replacement-model'],
    skeleton: [current ? m('Re-analyze') : kept ? m('Analyze again') : m('Analyze')], materials: [m('Configure'), 'materials'],
    mapping: [m('Configure'), 'mesh-mapping'], processing: [m('Configure'), 'model-processing'],
    physics: [m('Change')], motion: [m('Change')], output: [m('Configure'), 'output-settings'],
  };
  rows.forEach(row => {
    [row.action, row.section] = actions[row.key];
    if (!model && row.key === 'materials' && heroColors) { row.page = 'extras'; row.action = m('Change'); row.section = undefined; }
    else if (!model && ['model', 'materials', 'mapping', 'processing'].includes(row.key)) row.status = m('Not applicable');
  });
  return {
    rows, issues, ready, hasInputs, current, model: inspection,
    /** The report is kept as the working list (`items`); `verified`: a build checked the live project since. */
    kept, verified, refused, items, open,
    tone: !hasInputs || issues.length ? 'attention' as const : ready ? 'ready' as const : 'neutral' as const,
    title: !hasInputs ? t('Set up your project') : kept ? open || refused ? t('Needs attention') : model && !project.compile_model ? t('Ready to prepare sources') : t('Ready to build') : !current ? report ? t('Project changed · analyze again') : t('Ready to analyze') : ready ? model && !project.compile_model ? t('Ready to prepare sources') : t('Ready to build') : t('Needs attention'),
    description: !hasInputs ? t('Choose a character and add the inputs needed for this project.') : kept ? refused ? t('The last build found something that still needs a choice. Analyze to see what is left.') : open ? t('Resolve the items below. Build checks the project again before it starts.') : verified ? t('The last build checked this project. Analyze again is optional.') : t('The project changed since the last analysis. Build checks it again before it starts; Analyze again is optional.') : !current ? t('Analyze the current project to check compatibility, materials and mappings.') : ready ? t('Build rechecks the current files and validates outputs before publishing.') : t('Resolve the items below, then analyze again.'),
  };
}
