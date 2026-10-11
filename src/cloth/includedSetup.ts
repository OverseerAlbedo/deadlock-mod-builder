import baseline from '../../physics-references/Vindicta-R12-Balanced/baseline.json';
import type { Project } from '../models';
import type { ProjectInspection } from '../build/models';
import { modelEnabled, testedPhysicsEnabled } from '../build/readiness';

export function includedClothHero(project: Project) {
  return modelEnabled(project) && project.original_model.replace(/_c$/, '') === 'models/heroes_staging/hornet_v3/hornet.vmdl';
}
const record = (value: unknown): value is Record<string, unknown> => typeof value === 'object' && value !== null;
/** A hero name, bone count, or saved filename alone is not compatibility evidence. */
export function qualifiedIncludedCloth(project: Project, report: ProjectInspection | null, fresh: boolean) {
  if (!includedClothHero(project) || !testedPhysicsEnabled(project) || !fresh || !report || !('dmx' in report) || !record(report.original)) return false;
  const contract = report.original.tested_physics;
  return record(contract) && contract.source_sha256 === report.source_sha256 && contract.reference === project.customization.physics?.source && record(contract.selection) && contract.selection.reference_sha256 === baseline.reference_sha256;
}
