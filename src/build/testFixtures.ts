import type { Project } from '../models';
import type { InspectionReport } from './models';
import reference from '../../src-tauri/crates/deadlock-project/tests/fixtures/reference.json';

// Non-playable UI fixtures. These do not establish native model compatibility.
export const reviewedProject = {
  ...reference.cases[1].expected, name: 'Vindicta Balanced', dmx: 'C:\\Models\\Collection.dmx',
  materials: { 'body.vmat': { mode: 'generate', confirmed: true } }, meshes: { body: 'body' },
  customization: {}, lod_strategy: 'reuse', review_lods: true,
} as unknown as Project;
export const inspected: InspectionReport = {
  project: reviewedProject.name, hero_root: 'models/heroes_staging/hornet_v3', needs_review: false, warnings: [],
  model: { meshes: [{ name: 'body', filename: null, lower_lod: false }], lod_groups: 2, attachments: 0, ag2: [] },
  dmx: { encoding: 'binary 9', format: 'model', format_version: 22, meshes: [{ name: 'body', element: 1, vertices: 10, indexed_vertices: 30, weight_values: 40, joint_index_values: 40, has_skin_weights: true, vertex_attributes: [] }], materials: ['body.vmat'], material_identities: [], bones: ['root'], elements: 1 },
  geometry: { meshes: [], source_triangles: 100 }, mappings: { body: 'body' }, project_mappings: { body: 'body' }, source_mappings: { body: 'body' },
  unresolved_meshes: [], unskinned_meshes: [], materials: reviewedProject.materials,
  vertex_preflight: { errors: [], warnings: [], count_scope: 'Source fixture', automatic_subdivision: false },
  automatic_split: null, source_sha256: 'a'.repeat(64), original: null,
};
