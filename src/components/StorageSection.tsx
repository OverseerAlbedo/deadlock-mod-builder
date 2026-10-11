import { ask, open } from '@tauri-apps/plugin-dialog';
import { Fragment, useState } from 'react';
import { api, basename } from '../api';
import type { Project, Settings, StorageClearRequest, StorageKind, StorageProject, StorageReport } from '../models';
import { Section, StateLabel } from './Primitives';

export function formatBytes(bytes: number): string {
  if (bytes < 1024 * 1024) return `${Math.max(0, Math.round(bytes / 1024))} KB`;
  if (bytes < 1024 ** 3) return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
  return `${(bytes / 1024 ** 3).toFixed(2)} GB`;
}

const kinds: Record<StorageKind, {label: string; hint: (settings: Settings) => string}> = {
  preparation: {label: 'Model preparation', hint: s => `Imported sources and converted models. Clear keeps the run the project uses${s.keep_previous_preparation === false ? '' : ' and the previous one'}.`},
  originals: {label: 'Original hero files', hint: () => 'Extracted from the game. Extracted again when needed.'},
  backups: {label: 'Backups', hint: s => `Replaced VPKs and earlier original files. Clear keeps the newest ${s.keep_backups ?? 1} per file.`},
  output: {label: 'Built VPK', hint: () => 'The finished mod. Kept until you remove the project data.'},
  build_data: {label: 'Build caches and staging', hint: () => 'Recreated by the next Analyze or Build.'},
  compiler: {label: 'Compiler working folders', hint: () => 'Copies in the CSDK and model toolset. Recreated by the next build.'},
  temporary: {label: 'Leftovers', hint: () => 'From interrupted operations.'},
  records: {label: 'Records and analysis', hint: () => 'Project snapshots, build records and texture analysis.'},
};
const clearable: StorageKind[] = ['preparation', 'originals', 'backups', 'build_data', 'compiler', 'temporary'];
const cleanup: StorageKind[] = ['preparation', 'backups', 'build_data', 'compiler', 'temporary'];

function projectLabel(entry: StorageProject) {
  if (entry.name) return entry.hero ? `${entry.name} · ${entry.hero}` : entry.name;
  return `Project ${entry.id.slice(0, 8)}`;
}
function projectState(entry: StorageProject) {
  if (entry.locked) return <StateLabel tone="attention">Build lock present</StateLabel>;
  if (entry.open) return <StateLabel tone="ready">Open now</StateLabel>;
  if (entry.project_file) return <StateLabel tone="ready">Saved · {basename(entry.project_file)}</StateLabel>;
  if (!entry.workspace) return <StateLabel tone="neutral">Only compiler folders remain</StateLabel>;
  return <StateLabel tone="neutral">No saved project file known</StateLabel>;
}

export function StorageSection({ settings, update, project, busy, dirty, operate }: {
  settings: Settings; update: (settings: Settings) => void; project: Project | null;
  busy: boolean; dirty: boolean;
  operate: <T,>(label: string, operation: () => Promise<T>, canCancel?: boolean) => Promise<T | undefined>;
}) {
  const [report, setReport] = useState<StorageReport | null>(null);
  const [message, setMessage] = useState('');
  const [expanded, setExpanded] = useState<string | null>(null);
  const measure = () => operate('Could not measure stored data', async () => { setReport(await api.storageReport(project)); setMessage(''); }, true);
  const clear = async (request: StorageClearRequest, question: string) => {
    if (!await ask(question, {title: 'Clear stored data', kind: 'warning', okLabel: 'Remove', cancelLabel: 'Keep'})) return;
    await operate('Could not clear stored data', async () => {
      const {result, report} = await api.storageClear(request, project);
      setReport(report);
      setMessage(`Removed ${formatBytes(result.removed_bytes)}${result.skipped.length ? `; ${result.skipped.length} item(s) could not be removed (see Logs)` : ''}.`);
    }, true);
  };
  const find = async () => {
    const folder = await open({title: 'Folder with saved projects', directory: true, multiple: false});
    if (typeof folder !== 'string') return;
    await operate('Could not search for projects', async () => {
      const found = await api.storageFindProjects(folder);
      setReport(await api.storageReport(project));
      setMessage(found ? `Found ${found} project file(s).` : 'No project files were found in that folder.');
    }, true);
  };
  const unusedShared = report?.shared.filter(entry => !entry.used_by.length) ?? [];
  return <Section title="Storage" actions={<div className="row">
    <button disabled={busy} onClick={() => void measure()}>{report ? 'Measure again' : 'Measure storage'}</button>
    {report && <button disabled={busy || !report.reclaimable} onClick={() => void clear({kinds: cleanup, unused_shared: true}, `Remove data the builder can recreate in every project (about ${formatBytes(report.reclaimable)})? Built VPKs, the data each project uses and the newest backups stay.`)}>Clean up all projects…</button>}
  </div>}>
    <p className="hint">Projects are built in <code>{settings.workspace || 'the workspace folder'}</code>. The builder also keeps a working copy per project in the CSDK and model toolset folders.</p>
    <div className="settings-fields storage-retention">
      <div className="field"><label htmlFor="storage-keep-backups">Backups kept per file</label>
        <select id="storage-keep-backups" disabled={busy} value={settings.keep_backups ?? 1} onChange={event => update({...settings, keep_backups: Number(event.target.value)})}>
          {[0, 1, 2, 3, 5].map(value => <option key={value} value={value}>{value === 0 ? 'None' : value}</option>)}
        </select></div>
      <label className="checkbox checkbox-line"><input type="checkbox" disabled={busy} checked={settings.keep_previous_preparation ?? true} onChange={event => update({...settings, keep_previous_preparation: event.target.checked})} /> Keep the previous model preparation</label>
      <label className="checkbox checkbox-line"><input type="checkbox" disabled={busy} checked={settings.clear_build_data_after_build ?? false} onChange={event => update({...settings, clear_build_data_after_build: event.target.checked})} /> Clear build caches and compiler folders after each successful build</label>
      {dirty && <p className="hint">Save settings to apply these choices.</p>}
    </div>
    {message && <p role="status">{message}</p>}
    {report && <>
      <dl className="summary-list">
        <dt>Total</dt><dd>{formatBytes(report.bytes)}</dd>
        <dt>Can be cleared</dt><dd>{formatBytes(report.reclaimable)}</dd>
      </dl>
      <div className="table-scroll"><table className="storage-table">
        <thead><tr><th>Project</th><th>State</th><th>Size</th><th>Can clear</th><th><span className="sr-only">Actions</span></th></tr></thead>
        <tbody>{report.projects.map(entry => <Fragment key={entry.id}>
          <tr>
            <th scope="row" title={entry.workspace ?? entry.id}>{projectLabel(entry)}</th>
            <td>{projectState(entry)}</td>
            <td>{formatBytes(entry.bytes)}</td>
            <td>{formatBytes(entry.reclaimable)}</td>
            <td className="row">
              <button className="tertiary" aria-expanded={expanded === entry.id} onClick={() => setExpanded(expanded === entry.id ? null : entry.id)}>Details</button>
              <button className="tertiary" disabled={busy || entry.open || entry.locked} title={entry.open ? 'Open another project first' : undefined}
                onClick={() => void clear({project: entry.id, remove_project: true}, `Remove all stored data of “${projectLabel(entry)}” (${formatBytes(entry.bytes)}), including its built VPK and compiler folders? ${entry.project_file ? 'Its saved project file stays, but it must be prepared and built again.' : 'No saved project file is known for it.'} Installed mods in Deadlock are not touched.`)}>Remove…</button>
            </td>
          </tr>
          {expanded === entry.id && <tr className="storage-details"><td colSpan={5}><table>
            <tbody>{entry.usage.map(usage => <tr key={usage.kind}>
              <th scope="row">{kinds[usage.kind].label}<p className="hint">{kinds[usage.kind].hint(settings)}</p></th>
              <td>{formatBytes(usage.bytes)}</td>
              <td>{clearable.includes(usage.kind) ? <button disabled={busy || entry.locked || !usage.reclaimable}
                onClick={() => void clear({project: entry.id, kinds: [usage.kind]}, `Remove ${formatBytes(usage.reclaimable)} of ${kinds[usage.kind].label.toLowerCase()} from “${projectLabel(entry)}”?`)}>
                Clear {formatBytes(usage.reclaimable)}</button> : <span className="muted">Kept</span>}</td>
            </tr>)}</tbody>
          </table></td></tr>}
        </Fragment>)}</tbody>
      </table></div>
      {report.shared.length > 0 && <>
        <h3>Shared original hero files</h3>
        <p className="hint">Extracted once per hero and game version and used by every project of that hero.</p>
        <div className="table-scroll"><table className="storage-table"><tbody>{report.shared.map(entry => <tr key={entry.key}>
          <th scope="row">{entry.model ? basename(entry.model) : entry.key}</th>
          <td>{entry.used_by.length ? `${entry.used_by.length} project(s)` : 'Not used'}</td>
          <td>{formatBytes(entry.bytes)}</td>
        </tr>)}</tbody></table></div>
        {unusedShared.length > 0 && <button disabled={busy} onClick={() => void clear({unused_shared: true}, `Remove ${unusedShared.length} unused original hero file set(s) (${formatBytes(unusedShared.reduce((sum, entry) => sum + entry.bytes, 0))})?`)}>Remove unused</button>}
      </>}
      <p className="hint">Projects saved before this version are recognised once you open or save them. <button className="tertiary" disabled={busy} onClick={() => void find()}>Find saved projects in a folder…</button></p>
    </>}
  </Section>;
}
