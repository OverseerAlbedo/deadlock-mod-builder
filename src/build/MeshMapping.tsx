import type { InspectionReport } from './models';

type Mapping = string | string[];
type Mappings = Record<string, Mapping>;

const special = (value: Mapping | undefined): value is string => typeof value === 'string' && value.startsWith('@');
export const mappedTargets = (value: Mapping | undefined): string[] =>
  value === undefined || special(value) ? [] : Array.isArray(value) ? value : [value];
const asMapping = (list: string[]): Mapping | undefined => list.length === 0 ? undefined : list.length === 1 ? list[0] : list;
export const lodBase = (name: string) => name.replace(/[_. -]?lod[_. -]?\d+$/i, '');

/** Saved choices win; otherwise the analysis suggestion is what the build will use. */
export function effectiveMappings(report: InspectionReport, saved: Mappings): Mappings {
  const result: Mappings = {};
  for (const node of report.model.meshes) {
    const value = saved[node.name] ?? report.project_mappings?.[node.name];
    if (value !== undefined) result[node.name] = value;
  }
  return result;
}

/** Source meshes that no render node uses. They would be missing in game. */
export function unusedSourceMeshes(report: InspectionReport, saved: Mappings): string[] {
  const used = new Set(Object.values(effectiveMappings(report, saved)).flatMap(mappedTargets));
  return report.dmx.meshes.map(mesh => mesh.name).filter(name => !used.has(name));
}

const canonical = (name: string) => name.toLowerCase().replace(/[^a-z0-9]/g, '');
const weaponNode = (name: string) => /gun|weapon/.test(canonical(name));
/** The main body node: the first base node that is not a gun (by its name or `gun_part`),
 * whatever it shows, so a mesh the user put on another part never turns that part into the body. */
function mainNode(report: InspectionReport) {
  return report.model.meshes.find(node => !node.lower_lod && !weaponNode(node.name) && !node.gun_part) ?? null;
}

/** Adds meshes to the main render node and its lower-detail levels; Keep/Disable rows are untouched. */
export function addToMainNode(report: InspectionReport, saved: Mappings, meshes: string[]): { meshes: Mappings; node: string | null } {
  const effective = effectiveMappings(report, saved);
  // When the user set the main node to Keep or Disable, the first other body part that shows
  // meshes; never a gun.
  const first = mainNode(report);
  const main = first && !special(effective[first.name]) ? first
    : report.model.meshes.find(node => !node.lower_lod && !weaponNode(node.name) && !node.gun_part && mappedTargets(effective[node.name]).length > 0) ?? null;
  if (!main) return { meshes: saved, node: null };
  const next = { ...saved };
  for (const node of report.model.meshes) {
    if (node.name !== main.name && !(node.lower_lod && lodBase(node.name) === main.name)) continue;
    if (special(effective[node.name])) continue;
    // A detail level without a choice yet follows the main node, as the suggestion does.
    const current = effective[node.name] ?? effective[main.name];
    const list = [...new Set([...mappedTargets(current), ...meshes])];
    const value = asMapping(list);
    if (value !== undefined) next[node.name] = value;
  }
  return { meshes: next, node: main.name };
}

/**
 * A proposal for render nodes that have no mapping yet, applied only when you choose it.
 * Prepared characters name meshes after the source (Body, Face), not after the hero's render
 * nodes: every source mesh goes on the main body node, the original's other parts are hidden,
 * its gun and its carried objects (`carried`: parts the game shows only while the hero carries
 * something, decided at Analyze) stay, and lower detail levels follow the LOD strategy. Parts of
 * the hero's gun whose names do not say so (`gun_part`: nearly all of their skin weight on weapon
 * bones, decided at Analyze; ammunition, a bow's clips) follow the gun: kept while the hero's own
 * gun is kept, disabled once a gun node shows the user's meshes or the gun is disabled (a fitted gun from
 * Prepare model writes these rows itself: the user's gun meshes on the gun's main node and on the
 * nodes the game shows instead of it, Disable on the parts it shows beside it). When every source mesh is
 * named like a render node, the classic name matching is kept instead. When the main node and
 * its detail levels show meshes and other source meshes are left over, the suggestion adds them
 * there. Analyze saves its own matches as the project's rows (`project_mappings`), so a row the
 * analysis matched cannot be told from one the user set: the suggestion only ever adds unused
 * meshes to rows that show meshes, and never removes or replaces what a row shows.
 */
export function suggestedMappings(report: InspectionReport, saved: Mappings, strategy: string): Mappings {
  const effective = effectiveMappings(report, saved);
  const nodes = report.model.meshes;
  const sources = report.dmx.meshes.map(mesh => mesh.name);
  const namedLikeNode = (source: string) => nodes.some(node => canonical(source) === canonical(node.name) || canonical(source) === canonical(lodBase(node.name)));
  const named = sources.length > 0 && sources.every(namedLikeNode);

  // The hero's own gun is kept unless a gun node shows the user's meshes, or the gun node named
  // as such is disabled. A part disabled on its own decides nothing for the others.
  const gunKept = !nodes.some(node => !node.lower_lod && (weaponNode(node.name) || node.gun_part) && (
    mappedTargets(effective[node.name]).length > 0 || (effective[node.name] === '@disable' && weaponNode(node.name) && node.gun_part)));
  const main = mainNode(report);

  const result: Mappings = {};
  if (named || !main) return result;
  const unused = unusedSourceMeshes(report, saved);
  if (mappedTargets(effective[main.name]).length > 0 && unused.length > 0) {
    for (const node of nodes) {
      if (node.name !== main.name && !(node.lower_lod && lodBase(node.name) === main.name)) continue;
      const targets = mappedTargets(effective[node.name]);
      if (targets.length === 0) continue;
      const value = asMapping([...new Set([...targets, ...unused])]);
      if (value !== undefined) result[node.name] = value;
    }
  }
  for (const node of nodes.filter(node => !node.lower_lod && effective[node.name] === undefined)) {
    const value = node.name === main.name ? asMapping(unused)
      : weaponNode(node.name) || node.carried || (node.gun_part && gunKept) ? '@keep' : '@disable';
    if (value !== undefined) result[node.name] = value;
  }
  for (const node of nodes.filter(node => node.lower_lod && effective[node.name] === undefined)) {
    const base = result[lodBase(node.name)] ?? effective[lodBase(node.name)];
    if (base === undefined) continue;
    result[node.name] = strategy === 'preserve' ? '@keep' : strategy === 'disable' || strategy === 'single' ? '@disable' : base;
  }
  return result;
}

export function MeshMapping({ report, meshes, busy, update, strategy = 'choose' }: {
  report: InspectionReport; meshes: Mappings; busy: boolean; update: (meshes: Mappings) => void; strategy?: string;
}) {
  const effective = effectiveMappings(report, meshes);
  const sources = report.dmx.meshes.map(mesh => mesh.name);
  const suggestion = suggestedMappings(report, meshes, strategy);
  const suggested = Object.keys(suggestion);
  const unused = unusedSourceMeshes(report, meshes);
  const main = addToMainNode(report, meshes, unused).node;
  const shown = (name: string) => { const value = suggestion[name]; return value === undefined ? '' : special(value) ? (value === '@keep' ? 'keep original' : 'disable') : mappedTargets(value).join(' + '); };
  const set = (name: string, value: Mapping | undefined) => {
    const next = { ...meshes };
    if (value === undefined) delete next[name]; else next[name] = value;
    update(next);
  };
  return <>
    <p className="hint">Map source meshes to each original render node. A render node can use several source meshes; tick every mesh it should show. Saved mappings are rechecked during preparation.</p>
    {suggested.length > 0 && <div className="callout mesh-suggestion" role="status">
      <p><strong>Suggested mapping</strong> for {suggested.length} render node{suggested.length === 1 ? '' : 's'}{suggested.every(name => effective[name] === undefined) ? ' without a choice' : ''}: {suggested.slice(0, 6).map(name => `${name} → ${shown(name)}`).join('; ')}{suggested.length > 6 ? ' …' : ''}.</p>
      <button disabled={busy} onClick={() => update({ ...meshes, ...suggestion })}>Apply suggested mapping</button>
    </div>}
    {/* Shown even while a suggestion is pending, so an unused mesh always has its button (issue #6). */}
    {unused.length > 0 && <div className="callout warning mesh-unused" role="status">
      <p><strong>Not used by any render node, so missing in game:</strong> {unused.join(', ')}</p>
      {main && <button disabled={busy} onClick={() => update(addToMainNode(report, meshes, unused).meshes)}>Add {unused.length === 1 ? 'it' : 'them'} to {main} and its lower-detail levels</button>}
    </div>}
    <div className="table-scroll"><table className="mapping-table"><thead><tr><th>Original render node</th><th>Action</th><th>Replacement meshes</th></tr></thead><tbody>
      {report.model.meshes.map(node => {
        const value = effective[node.name];
        const fromAnalysis = meshes[node.name] === undefined && value !== undefined;
        const selected = mappedTargets(value);
        return <tr key={node.name}>
          <td>{node.name}{node.lower_lod && <small className="muted"> · Lower detail</small>}{fromAnalysis && <small className="muted"> · Suggested</small>}</td>
          <td><select aria-label={`Action for ${node.name}`} value={special(value) ? value : 'map'} disabled={busy}
            onChange={event => set(node.name, event.target.value === 'map' ? asMapping(selected.length ? selected : sources.slice(0, 1)) : event.target.value)}>
            <option value="map">Map replacement meshes</option><option value="@keep">Keep original</option><option value="@disable">Disable</option>
          </select></td>
          <td>{special(value) ? <span className="muted">{value === '@keep' ? 'Original geometry retained' : 'Render node disabled'}</span>
            : <div className="mesh-choices" role="group" aria-label={`Replacement meshes for ${node.name}`}>
              {sources.map(source => <label key={source} className="checkbox">
                <input type="checkbox" checked={selected.includes(source)} disabled={busy}
                  onChange={event => set(node.name, asMapping(event.target.checked ? [...selected, source] : selected.filter(name => name !== source)))} />
                <span>{source}</span>
              </label>)}
            </div>}</td>
        </tr>;
      })}
    </tbody></table></div>
  </>;
}
