import { describe, expect, it } from 'vitest';
import { inspected, reviewedProject } from './testFixtures';
import { buildRefused, keptAnalysis, modelEnabled, projectReadiness, testedPhysicsEnabled } from './readiness';
import type { Project } from '../models';
import type { InspectionReport } from './models';

// The last Analyze saw the LOD acknowledgement off and one material unconfirmed.
const analyzed = { ...reviewedProject, review_lods: false, materials: { 'body.vmat': { mode: 'generate', confirmed: false } } } as unknown as Project;
const report: InspectionReport = { ...inspected, needs_review: true, materials: analyzed.materials };
const kept = (project: Project, inspection: InspectionReport = report, from: Project = analyzed, verified = false) => projectReadiness(project, inspection, false, null, false, { analyzed: from, verified });
const states = (state: ReturnType<typeof projectReadiness>) => Object.fromEntries(state.items.map(item => [item.key, item.state]));

describe('The list of the last analysis is kept until Analyze runs again', () => {
  it('is kept for the same model, hero and mode and dropped for another', () => {
    const edited = { ...analyzed, review_lods: true };
    expect(keptAnalysis(edited, analyzed)).toBe(true);
    expect(kept(edited).kept).toBe(true);
    for (const other of [{ ...edited, dmx: 'C:\\Models\\Other.dmx' }, { ...edited, original_model: 'models/heroes/other.vmdl' }]) {
      const dropped = kept(other as Project);
      expect(dropped.kept).toBe(false);
      expect(dropped.items).toEqual([]);
      expect(dropped.ready).toBe(false);
      expect(dropped.title).toBe('Project changed · analyze again');
    }
    // Another mode (model switched off) never keeps a model report.
    const extras = { ...edited, customization: { enabled: true, model_enabled: false } } as unknown as Project;
    expect(keptAnalysis(extras, analyzed)).toBe(false);
    expect(kept(extras).kept).toBe(false);
    // Without the analyzed project (Character & model), a stale report only asks for Analyze, as before.
    const before = projectReadiness(edited, report, false);
    expect(before).toMatchObject({ kept: false, items: [], ready: false, title: 'Project changed · analyze again' });
  });
  it('ticks the LOD acknowledgement off live and reads materials from the live project', () => {
    const lod = kept({ ...analyzed, review_lods: true });
    expect(states(lod)).toEqual({ materials: 'open', lod: 'done' });
    expect(lod.open).toBe(1);
    expect(lod.ready).toBe(false);
    expect(lod.issues.map(issue => issue.text)).toEqual(['1 material assignment need review.']);
    // The report's own copy of the materials is frozen; the live project's is what counts.
    const both = kept({ ...analyzed, review_lods: true, materials: { 'body.vmat': { mode: 'generate', confirmed: true, ambiguous: [] } } } as unknown as Project);
    expect(states(both)).toEqual({ materials: 'done', lod: 'done' });
    // The frozen needs_review adds no "native analysis notes" item.
    expect(both.issues).toEqual([]);
    expect(both.ready).toBe(true);
    expect(both.title).toBe('Ready to build');
    expect(both.description).toContain('Build checks it again before it starts');
    // A choice still ambiguous keeps the material item open.
    expect(states(kept({ ...analyzed, materials: { 'body.vmat': { mode: 'generate', confirmed: true, ambiguous: ['normal'] } } } as unknown as Project)).materials).toBe('open');
  });
  it('ticks meshes without skin weights off only through the acknowledgement', () => {
    const unskinned = { ...report, unskinned_meshes: ['prop'], materials: reviewedProject.materials };
    const from = { ...analyzed, materials: reviewedProject.materials, review_lods: true, allow_unskinned: false };
    expect(states(kept({ ...from, name: 'Renamed' }, unskinned, from))).toEqual({ unskinned: 'open' });
    expect(states(kept({ ...from, allow_unskinned: true }, unskinned, from))).toEqual({ unskinned: 'done' });
    // Its list depends on the selected meshes: changed mappings wait for the next check.
    expect(states(kept({ ...from, meshes: { body: 'body', prop: 'prop' } } as unknown as Project, unskinned, from))).toEqual({ unskinned: 'changed' });
  });
  it('stops being ready after the build itself refused, until Analyze', () => {
    const handled = { ...analyzed, review_lods: true, materials: reviewedProject.materials };
    const state = projectReadiness(handled, report, false, null, false, { analyzed, refused: true });
    expect(state).toMatchObject({ kept: true, refused: true, open: 0, ready: false, title: 'Needs attention' });
    expect(state.description).toBe('The last build found something that still needs a choice. Analyze to see what is left.');
    expect(states(state)).toEqual({ materials: 'done', lod: 'done' });
    // Only the two refusals of the build's own check count, matched like the help rule.
    expect(buildRefused({ kind: 'validation', message: 'Review mesh mappings, materials, LODs and unskinned meshes before building. Inspection: C:\\w\\inspection.json', details: '' })).toBe(true);
    expect(buildRefused({ kind: 'validation', message: 'Operation failed', details: 'Model compilation stopped by vertex preflight:\nSkinned DMX mesh "Body"' })).toBe(true);
    expect(buildRefused({ kind: 'tool', message: 'Compiler stopped', details: 'resourcecompiler failed' })).toBe(false);
    expect(buildRefused({ kind: 'cancelled', message: 'Cancelled', details: '' })).toBe(false);
  });
  it('never keeps an assets-only report: that build does not analyze again first', () => {
    const extras = { ...reviewedProject, dmx: '', customization: { enabled: true, model_enabled: false } } as unknown as Project;
    const components = { model_enabled: false as const, icons: [], animations: [], effects: [], audio: [], clothing: null, native_compile_required: true, warnings: [], native_validated: false as const, in_game_validated: false as const };
    const state = projectReadiness({ ...extras, name: 'Renamed' }, components, false, null, false, { analyzed: extras });
    expect(state).toMatchObject({ kept: false, ready: false, title: 'Project changed · analyze again' });
  });
  it('lists open items that were not on the list (a LOD strategy chosen again)', () => {
    const from = { ...analyzed, materials: reviewedProject.materials, review_lods: true };
    const state = kept({ ...from, lod_strategy: 'choose' } as Project, { ...report, needs_review: false, materials: from.materials }, from);
    expect(states(state)).toEqual({ lod: 'open' });
    expect(state.ready).toBe(false);
  });
  it('never ticks backend results off: changed inputs wait for the next check and do not block', () => {
    const from = { ...analyzed, materials: reviewedProject.materials, review_lods: true, meshes: { body: 'body', hair: 'hair' } } as unknown as Project;
    const unresolved = { ...report, unresolved_meshes: ['face'], secondary_meshes_left_out: ['skirt'], materials: from.materials, vertex_preflight: { ...report.vertex_preflight, errors: ['Body exceeds vertex limit'] } };
    // An unrelated edit, and the same mappings in another key order, leave them open.
    const same = kept({ ...from, name: 'Renamed', meshes: { hair: 'hair', body: 'body' } } as unknown as Project, unresolved, from);
    expect(states(same)).toEqual({ 'preflight:0': 'open', unresolved: 'open', 'left-out': 'open' });
    expect(same.ready).toBe(false);
    const mapped = kept({ ...from, meshes: { body: 'body', hair: 'hair', face: 'face' } } as unknown as Project, unresolved, from);
    expect(states(mapped)).toEqual({ 'preflight:0': 'changed', unresolved: 'changed', 'left-out': 'changed' });
    expect(mapped.issues).toEqual([]);
    expect(mapped.ready).toBe(true);
    // Vertex preflight also depends on automatic splitting.
    expect(states(kept({ ...from, auto_split: !from.auto_split }, unresolved, from))).toEqual({ 'preflight:0': 'changed', unresolved: 'open', 'left-out': 'open' });
    // A build checked the live project since: its own analysis passed them.
    expect(states(kept({ ...from, meshes: { body: 'body', hair: 'hair', face: 'face' } } as unknown as Project, unresolved, from, true))).toEqual({ 'preflight:0': 'done', unresolved: 'done', 'left-out': 'done' });
  });
  it('names meshes left out of the build on a current analysis too', () => {
    const state = projectReadiness(reviewedProject, { ...inspected, needs_review: true, secondary_meshes_left_out: ['skirt'] }, true);
    expect(state.issues.map(issue => issue.text)).toEqual([expect.stringContaining('only in meshes left out of the build: skirt')]);
  });
  it('keeps the live VPK slot check on the kept list', () => {
    const slot = reviewedProject.vpk_number;
    const state = projectReadiness({ ...analyzed, review_lods: true, materials: reviewedProject.materials }, report, false, { configured_slot: slot, configured_slot_occupied: true, available_slot: 7 }, false, { analyzed });
    expect(state.items.find(item => item.state === 'open')?.action).toBe('Choose free slot');
    expect(state.ready).toBe(false);
  });
});

describe('Readiness is based on current native evidence', () => {
  it('never equates populated saved settings or an old inspection with a ready build', () => {
    expect(projectReadiness(reviewedProject, null, false).ready).toBe(false);
    const stale = projectReadiness(reviewedProject, inspected, false);
    expect(stale.ready).toBe(false);
    expect(stale.title).toContain('analyze again');
    expect(stale.rows.find(row => row.key === 'skeleton')?.tone).toBe('neutral');
    expect(projectReadiness(reviewedProject, inspected, true).ready).toBe(true);
  });
  it('reports a VPK slot another mod uses as an issue once the slot scan has run', () => {
    const slot = reviewedProject.vpk_number;
    const taken = projectReadiness(reviewedProject, inspected, true, {configured_slot: slot, configured_slot_occupied: true, available_slot: 7});
    expect(taken.ready).toBe(false);
    expect(taken.issues.map(issue => issue.text)).toContainEqual(expect.stringContaining(`slot ${slot} is in use by another mod`));
    expect(taken.issues.find(issue => issue.action === 'Choose free slot')?.text).toContain('slot 7 is free');
    expect(taken.rows.find(row => row.key === 'output')?.tone).toBe('attention');
    const free = projectReadiness(reviewedProject, inspected, true, {configured_slot: slot, configured_slot_occupied: false, available_slot: slot});
    expect(free.ready).toBe(true);
    expect(free.rows.find(row => row.key === 'output')?.value).toContain('slot free');
    // A scan of another slot (the project changed slot since) says nothing.
    expect(projectReadiness(reviewedProject, inspected, true, {configured_slot: slot + 1, configured_slot_occupied: true, available_slot: null}).ready).toBe(true);
    // Replace an existing installed VPK (Build's choice) keeps the note but not the block.
    const replacing = projectReadiness(reviewedProject, inspected, true, {configured_slot: slot, configured_slot_occupied: true, available_slot: 7}, true);
    expect(replacing.ready).toBe(true);
    expect(replacing.rows.find(row => row.key === 'output')?.value).toContain('will be replaced');
  });
  it('shows native blockers and review actions without hiding them behind a ready badge', () => {
    const report = { ...inspected, needs_review: true, unresolved_meshes: ['body'], materials: { 'body.vmat': { mode: 'generate', confirmed: false } }, vertex_preflight: { ...inspected.vertex_preflight, errors: ['Body exceeds vertex limit'] } };
    const state = projectReadiness({ ...reviewedProject, review_lods: false }, report, true);
    expect(state.ready).toBe(false);
    expect(state.issues.map(issue => issue.text).join(' ')).toMatch(/Body exceeds vertex limit.*mapping.*material.*detail-level/);
    expect(state.issues.some(issue => issue.section === 'build-settings')).toBe(true);
  });
  it('does not claim compiled skeleton compatibility when the backend defers that check', () => {
    const state = projectReadiness(reviewedProject, { ...inspected, source_identity: { identity: { required: true, passed: false }, extension: null } }, true);
    expect(state.rows.find(row => row.key === 'skeleton')).toMatchObject({ tone: 'neutral', value: 'Compiled skeleton check required' });
  });
  it('respects the backend master switch and rejects a report for another project mode', () => {
    const inactive = { ...reviewedProject, customization: { enabled: false, model_enabled: false, physics: { enabled: true, source: 'saved.vmdl_c' } } };
    expect(modelEnabled(inactive)).toBe(true);
    expect(testedPhysicsEnabled(inactive)).toBe(false);
    const components = { model_enabled: false as const, icons: [], animations: [], effects: [], audio: [], clothing: null, native_compile_required: true, warnings: [], native_validated: false as const, in_game_validated: false as const };
    expect(projectReadiness(inactive, components, true).ready).toBe(false);
    expect(projectReadiness({ ...inactive, dmx: '', customization: { enabled: true, model_enabled: false } }, components, true).ready).toBe(true);
  });
});
