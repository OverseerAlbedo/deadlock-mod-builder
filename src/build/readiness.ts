import { basename } from '../api';
import { heroLabel } from '../components/HeroPicker';
import type { StateTone } from '../components/Primitives';
import type { Project } from '../models';
import type { ProjectInspection } from './models';

export type ReviewDestination = 'project' | 'appearance' | 'physics' | 'motion' | 'extras' | 'build';
export interface ReadinessRow { key: string; label: string; value: string; tone: StateTone; page?: ReviewDestination; action?: string; section?: string; status?: string }
export interface ReviewIssue { text: string; page?: ReviewDestination; section?: string; action?: string }
export const modelEnabled = (project: Project) => !project.customization.enabled || project.customization.model_enabled !== false;
export const testedPhysicsEnabled = (project: Project) => Boolean(modelEnabled(project) && project.customization.enabled && project.customization.physics?.enabled);

/** Describes native inspection and saved choices. It never substitutes for a build gate. */
export function projectReadiness(project: Project, report: ProjectInspection | null, fresh: boolean) {
  const model = modelEnabled(project);
  const matching = report && (model ? 'dmx' in report : 'model_enabled' in report);
  const current = fresh && matching ? report : null;
  const inspection = current && 'dmx' in current ? current : null;
  const hasInputs = Boolean(project.original_model && (!model || project.dmx));
  const issues: ReviewIssue[] = [];
  if (!project.original_model) issues.push({ text: 'Choose the character you want to replace.', page: 'project', section: 'character-selection', action: 'Choose character' });
  if (model && !project.dmx) issues.push({ text: 'Add a replacement model.', page: 'appearance', section: 'replacement-model', action: 'Add model' });
  const materials = inspection?.materials ?? project.materials;
  const materialReview = Object.values(materials).filter(material => material.mode === 'generate' && (!material.confirmed || Boolean(material.ambiguous?.length))).length;
  if (inspection) {
    inspection.vertex_preflight.errors.forEach(text => issues.push({ text, page: 'appearance', section: 'model-processing', action: 'Configure meshes' }));
    if (inspection.unresolved_meshes.length) issues.push({ text: `${inspection.unresolved_meshes.length} model mapping${inspection.unresolved_meshes.length === 1 ? '' : 's'} need a choice.`, section: 'mesh-mapping', action: 'Configure mappings' });
    if (materialReview) issues.push({ text: `${materialReview} material assignment${materialReview === 1 ? '' : 's'} need review.`, page: 'appearance', section: 'materials', action: 'Configure materials' });
    if (project.lod_strategy === 'choose' || ((inspection.model.lod_groups > 1 || inspection.geometry.source_triangles > 500000) && !project.review_lods)) issues.push({ text: 'Review the detail-level strategy and acknowledge its appearance and performance tradeoffs.', section: 'build-settings', action: 'Configure detail levels' });
    if (inspection.unskinned_meshes.length && inspection.dmx.bones.length && !project.allow_unskinned) issues.push({ text: 'Review meshes without skin weights in Model processing.', page: 'appearance', section: 'model-processing', action: 'Configure meshes' });
    if (inspection.needs_review && !issues.length) issues.push({ text: 'Review the native analysis notes before building.', section: 'analysis-details', action: 'View analysis notes' });
  }
  const packaging = project.build_vpk && (!model || project.compile_model);
  const invalidSlot = packaging && (!Number.isInteger(project.vpk_number) || project.vpk_number < 2 || project.vpk_number > 99);
  if (invalidSlot) issues.push({text:'Choose a VPK slot from 2 to 99 before packaging.',section:'output-settings',action:'Configure output'});
  const ready = Boolean(hasInputs && current && !issues.length && (!inspection || (!inspection.needs_review && !inspection.vertex_preflight.errors.length)));
  const physics = project.customization.physics;
  const tested = testedPhysicsEnabled(project);
  const generated = Boolean(model&&project.customization.enabled&&project.customization.secondary_motion?.enabled);
  const animations = project.customization.enabled ? (project.customization.animations ?? []).filter(row => row.enabled !== false).length : 0;
  const skeleton = inspection?.source_identity;
  const deferredSkeleton = skeleton?.identity?.required && !skeleton.identity.passed && !skeleton.extension;
  const rows: ReadinessRow[] = [
    { key: 'character', label: 'Character', value: project.original_model ? heroLabel({ name: project.hero_name, path: project.original_model, likely_hero: true }) : 'Choose a character', tone: project.original_model ? 'ready' : 'attention', page: 'project' },
    { key: 'model', label: 'Replacement model', value: !model ? 'Original model kept · extras only' : project.dmx ? basename(project.dmx) : 'Add a model', tone: !model ? 'neutral' : project.dmx ? 'ready' : 'attention', page: 'appearance' },
    { key: 'skeleton', label: 'Compatibility', value: !model ? current ? 'Extra assets inspected · build validates outputs' : 'Analyze extra assets before building' : !inspection ? 'Analyze to check compatibility' : deferredSkeleton ? 'Compiled skeleton check required' : 'Source preflight complete', tone: !current || deferredSkeleton ? 'neutral' : 'ready', page: 'build', status: !current ? 'Not analyzed' : deferredSkeleton ? 'Build check required' : 'Ready' },
    { key: 'materials', label: 'Materials', value: !model ? 'Original materials kept' : materialReview ? `${materialReview} need review` : `${Object.keys(materials).length} assignment${Object.keys(materials).length === 1 ? '' : 's'}${inspection ? ' checked' : ' saved'}`, tone: !model ? 'neutral' : materialReview ? 'attention' : inspection ? 'ready' : 'neutral', page: 'appearance' },
    { key: 'mapping', label: 'Model mapping', value: !model ? 'No model replacement' : inspection?.unresolved_meshes.length ? `${inspection.unresolved_meshes.length} unresolved` : `${Object.keys(inspection?.mappings ?? project.meshes).length} mapping${Object.keys(inspection?.mappings ?? project.meshes).length === 1 ? '' : 's'}${inspection ? ' checked' : ' saved'}`, tone: !model ? 'neutral' : inspection?.unresolved_meshes.length ? 'attention' : inspection ? 'ready' : 'neutral', page: 'build' },
    { key: 'processing', label: 'Mesh processing', value: !model ? 'No model processing' : inspection?.vertex_preflight.errors.length ? `${inspection.vertex_preflight.errors.length} processing issue${inspection.vertex_preflight.errors.length === 1 ? '' : 's'}` : project.auto_split ? 'Automatic splitting enabled' : 'Automatic splitting disabled', tone: inspection?.vertex_preflight.errors.length ? 'attention' : model && (project.auto_split || inspection) ? 'ready' : 'neutral', page: 'appearance' },
    { key: 'physics', label: 'Physics', value: !model ? 'Original physics kept' : generated ? `${project.customization.secondary_motion?.preset==='tangtang_r2'?'TangTang R2 tuned':'Automatic secondary motion'}${inspection?.secondary_motion?' · groups checked':''}` : tested ? physics?.source ? `Replacement cloth setup${inspection ? ' · reference checked' : ' selected'}` : 'Choose a tested reference' : 'Preserve original physics', tone: generated ? inspection?.secondary_motion?'ready':'neutral' : tested && !physics?.source ? 'attention' : tested && !inspection ? 'neutral' : 'ready', page: 'physics' },
    { key: 'motion', label: 'Motion', value: animations ? `${animations} animation replacement${animations === 1 ? '' : 's'} enabled` : project.animation_policy === 'preserve' ? 'Original animations preserved' : 'Legacy animation references disabled', tone: animations || project.animation_policy !== 'preserve' ? 'neutral' : 'ready', page: 'motion' },
    { key: 'output', label: 'Output', value: invalidSlot ? 'Select a VPK slot (2–99)' : model && !project.compile_model ? 'Source preparation only' : !project.build_vpk ? 'Compile only · no VPK' : `pak${String(project.vpk_number).padStart(2, '0')}_dir.vpk · ${project.install ? 'build and install' : 'build only'}`, tone: invalidSlot ? 'attention' : 'neutral', status: invalidSlot ? 'Needs attention' : 'Configured', page: 'build' },
  ];
  const actions: Record<string, [string, string?]> = {
    character: [project.original_model ? 'Change' : 'Choose', 'character-selection'],
    model: [project.dmx ? 'Change' : model ? 'Add model' : 'View', 'replacement-model'],
    skeleton: [current ? 'Re-analyze' : 'Analyze'], materials: ['Configure', 'materials'],
    mapping: ['Configure', 'mesh-mapping'], processing: ['Configure', 'model-processing'],
    physics: ['Change'], motion: ['Change'], output: ['Configure', 'output-settings'],
  };
  rows.forEach(row => {
    [row.action, row.section] = actions[row.key];
    if (!model && ['model', 'materials', 'mapping', 'processing'].includes(row.key)) row.status = 'Not applicable';
  });
  return {
    rows, issues, ready, hasInputs, current, model: inspection,
    tone: !hasInputs || issues.length ? 'attention' as const : ready ? 'ready' as const : 'neutral' as const,
    title: !hasInputs ? 'Set up your project' : !current ? report ? 'Project changed · analyze again' : 'Ready to analyze' : ready ? model && !project.compile_model ? 'Ready to prepare sources' : 'Ready to build' : 'Needs attention',
    description: !hasInputs ? 'Choose a character and add the inputs needed for this project.' : !current ? 'Analyze the current project to check compatibility, materials and mappings.' : ready ? 'Build rechecks the current files and validates outputs before publishing.' : 'Resolve the items below, then analyze again.',
  };
}
