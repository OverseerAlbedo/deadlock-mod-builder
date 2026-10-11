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

/** Adds meshes to the main render node and its lower-detail levels; Keep/Disable rows are untouched. */
export function addToMainNode(report: InspectionReport, saved: Mappings, meshes: string[]): { meshes: Mappings; node: string | null } {
  const effective = effectiveMappings(report, saved);
  const bases = report.model.meshes.filter(node => !node.lower_lod);
  const main = bases.find(node => mappedTargets(effective[node.name]).length > 0)
    ?? bases.find(node => effective[node.name] === undefined) ?? null;
  if (!main) return { meshes: saved, node: null };
  const next = { ...saved };
  for (const node of report.model.meshes) {
    if (node.name !== main.name && !(node.lower_lod && lodBase(node.name) === main.name)) continue;
    if (special(effective[node.name])) continue;
    const list = [...new Set([...mappedTargets(effective[node.name]), ...meshes])];
    const value = asMapping(list);
    if (value !== undefined) next[node.name] = value;
  }
  return { meshes: next, node: main.name };
}

const canonical = (name: string) => name.toLowerCase().replace(/[^a-z0-9]/g, '');
const weaponNode = (name: string) => /gun|weapon/.test(canonical(name));

/**
 * A proposal for render nodes that have no mapping yet, applied only when you choose it.
 * Prepared characters name meshes after the source (Body, Face), not after the hero's render
 * nodes: every source mesh goes on the main body node, the original's other parts are hidden,
 * its gun stays, and lower detail levels follow the LOD strategy. Name-matched sources keep
 * the classic matching instead.
 */
export function suggestedMappings(report: InspectionReport, saved: Mappings, strategy: string): Mappings {
  const effective = effectiveMappings(report, saved);
  const nodes = report.model.meshes;
  const sources = report.dmx.meshes.map(mesh => mesh.name);
  const named = sources.some(source => nodes.some(node => canonical(source) === canonical(node.name) || canonical(source) === canonical(lodBase(node.name))));
  const main = nodes.find(node => !node.lower_lod && !weaponNode(node.name));
  const result: Mappings = {};
  if (named || !main || !nodes.some(node => effective[node.name] === undefined)) return result;
  const unused = unusedSourceMeshes(report, saved);
  for (const node of nodes.filter(node => !node.lower_lod && effective[node.name] === undefined)) {
    const value = node.name === main.name ? asMapping(unused) : weaponNode(node.name) ? '@keep' : '@disable';
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
      <p><strong>Suggested mapping</strong> for {suggested.length} render node{suggested.length === 1 ? '' : 's'} without a choice: {suggested.slice(0, 6).map(name => `${name} → ${shown(name)}`).join('; ')}{suggested.length > 6 ? ' …' : ''}.</p>
      <button disabled={busy} onClick={() => update({ ...meshes, ...suggestion })}>Apply suggested mapping</button>
    </div>}
    {suggested.length === 0 && unused.length > 0 && <div className="callout warning mesh-unused" role="status">
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
