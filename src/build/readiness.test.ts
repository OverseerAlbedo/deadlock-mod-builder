import { describe, expect, it } from 'vitest';
import { inspected, reviewedProject } from './testFixtures';
import { modelEnabled, projectReadiness, testedPhysicsEnabled } from './readiness';

describe('Readiness is based on current native evidence', () => {
  it('never equates populated saved settings or an old inspection with a ready build', () => {
    expect(projectReadiness(reviewedProject, null, false).ready).toBe(false);
    const stale = projectReadiness(reviewedProject, inspected, false);
    expect(stale.ready).toBe(false);
    expect(stale.title).toContain('analyze again');
    expect(stale.rows.find(row => row.key === 'skeleton')?.tone).toBe('neutral');
    expect(projectReadiness(reviewedProject, inspected, true).ready).toBe(true);
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
