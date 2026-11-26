import { ask } from '@tauri-apps/plugin-dialog';
import { openFor } from '../dialogs';
import { Fragment, useState } from 'react';
import { api, basename } from '../api';
import type { Project, Settings, StorageClearRequest, StorageKind, StorageProject, StorageReport } from '../models';
import { Section, StateLabel } from './Primitives';
import { m, t, tp, useT, type Values } from '../i18n';
import { T } from '../i18n/T';
// i18n-area: settings

export function formatBytes(bytes: number): string {
  if (bytes < 1024 * 1024) return `${Math.max(0, Math.round(bytes / 1024))} KB`;
  if (bytes < 1024 ** 3) return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
  return `${(bytes / 1024 ** 3).toFixed(2)} GB`;
}

/** Each kind's name and line of help, in English (drawn translated). */
const kinds: Record<StorageKind, {label: string; hint: (settings: Settings) => {text: string; values?: Values}}> = {
  preparation: {label: m('Model preparation'), hint: s => ({text: s.keep_previous_preparation === false ? m('Imported sources and converted models. Clear keeps the run the project uses.') : m('Imported sources and converted models. Clear keeps the run the project uses and the previous one.')})},
  originals: {label: m('Original hero files'), hint: () => ({text: m('Extracted from the game. Extracted again when needed.')})},
  backups: {label: m('Backups'), hint: s => ({text: m('Replaced VPKs and earlier original files. Clear keeps the newest {count} per file.'), values: {count: s.keep_backups ?? 1}})},
  output: {label: m('Built VPK'), hint: () => ({text: m('The finished mod. Kept until you remove the project data.')})},
  build_data: {label: m('Build caches and staging'), hint: () => ({text: m('Recreated by the next Analyze or Build.')})},
  compiler: {label: m('Compiler working folders'), hint: () => ({text: m('Copies in the Community SDK (material and, with Reduced CSDK 13, model folders) and in the model toolset when one is used. Recreated by the next build.')})},
  temporary: {label: m('Leftovers'), hint: () => ({text: m('From interrupted operations.')})},
  records: {label: m('Records and analysis'), hint: () => ({text: m('Project snapshots, build records and texture analysis.')})},
};
/** What the last removal or search did (drawn in the interface language). */
type Outcome = {kind: 'removed'; bytes: number; skipped: number} | {kind: 'found'; count: number};
function outcomeText(outcome: Outcome): string {
  if (outcome.kind === 'found') return outcome.count ? tp(outcome.count, {other: 'Found {count} project file(s).'}) : t('No project files were found in that folder.');
  return outcome.skipped ? tp(outcome.skipped, {other: 'Removed {size}; {count} item(s) could not be removed (see Logs).'}, {size: formatBytes(outcome.bytes)}) : t('Removed {size}.', {size: formatBytes(outcome.bytes)});
}
const clearable: StorageKind[] = ['preparation', 'originals', 'backups', 'build_data', 'compiler', 'temporary'];
const cleanup: StorageKind[] = ['preparation', 'backups', 'build_data', 'compiler', 'temporary'];

function projectLabel(entry: StorageProject) {
  if (entry.name) return entry.hero ? `${entry.name} · ${entry.hero}` : entry.name;
  return t('Project {id}', {id: entry.id.slice(0, 8)});
}
function projectState(entry: StorageProject) {
  if (entry.locked) return <StateLabel tone="attention">{t('Build lock present')}</StateLabel>;
  if (entry.open) return <StateLabel tone="ready">{t('Open now')}</StateLabel>;
  if (entry.project_file) return <StateLabel tone="ready">{t('Saved · {file}', {file: basename(entry.project_file)})}</StateLabel>;
  if (!entry.workspace) return <StateLabel tone="neutral">{t('Only compiler folders remain')}</StateLabel>;
  return <StateLabel tone="neutral">{t('No saved project file known')}</StateLabel>;
}

export function StorageSection({ settings, update, project, busy, dirty, operate }: {
  settings: Settings; update: (settings: Settings) => void; project: Project | null;
  busy: boolean; dirty: boolean;
  operate: <T,>(label: string, operation: () => Promise<T>, canCancel?: boolean) => Promise<T | undefined>;
}) {
  useT();
  const [report, setReport] = useState<StorageReport | null>(null);
  const [message, setMessage] = useState<Outcome | null>(null);
  const [expanded, setExpanded] = useState<string | null>(null);
  const measure = () => operate(m('Could not measure stored data'), async () => { setReport(await api.storageReport(project)); setMessage(null); }, true);
  /** `question`: the question as it is shown (in the interface language). */
  const clear = async (request: StorageClearRequest, question: string) => {
    if (!await ask(question, {title: t('Clear stored data'), kind: 'warning', okLabel: t('Remove'), cancelLabel: t('Keep')})) return;
    await operate(m('Could not clear stored data'), async () => {
      const {result, report} = await api.storageClear(request, project);
      setReport(report);
      setMessage({kind: 'removed', bytes: result.removed_bytes, skipped: result.skipped.length});
    }, true);
  };
  const find = async () => {
    const folder = await openFor('project', {title: t('Folder with saved projects'), directory: true, multiple: false});
    if (typeof folder !== 'string') return;
    await operate(m('Could not search for projects'), async () => {
      const found = await api.storageFindProjects(folder);
      setReport(await api.storageReport(project));
      setMessage({kind: 'found', count: found});
    }, true);
  };
  const unusedShared = report?.shared.filter(entry => !entry.used_by.length) ?? [];
  return <Section title={t('Storage')} actions={<div className="row">
    <button disabled={busy} onClick={() => void measure()}>{report ? t('Measure again') : t('Measure storage')}</button>
    {report && <button disabled={busy || !report.reclaimable} onClick={() => void clear({kinds: cleanup, unused_shared: true}, t('Remove data the builder can recreate in every project (about {size})? Built VPKs, the data each project uses and the newest backups stay.', {size: formatBytes(report.reclaimable)}))}>{t('Clean up all projects…')}</button>}
  </div>}>
    <p className="hint"><T text="Projects are built in <code>{folder}</code>. The builder also keeps working copies per project in the Community SDK folder (and in the model toolset folder when one is used)." values={{folder: settings.workspace || t('the workspace folder')}}/></p>
    <div className="settings-fields storage-retention">
      <div className="field"><label htmlFor="storage-keep-backups">{t('Backups kept per file')}</label>
        <select id="storage-keep-backups" disabled={busy} value={settings.keep_backups ?? 1} onChange={event => update({...settings, keep_backups: Number(event.target.value)})}>
          {[0, 1, 2, 3, 5].map(value => <option key={value} value={value}>{value === 0 ? t('None') : value}</option>)}
        </select></div>
      <label className="checkbox checkbox-line"><input type="checkbox" disabled={busy} checked={settings.keep_previous_preparation ?? true} onChange={event => update({...settings, keep_previous_preparation: event.target.checked})} /> {t('Keep the previous model preparation')}</label>
      <label className="checkbox checkbox-line"><input type="checkbox" disabled={busy} checked={settings.clear_build_data_after_build ?? false} onChange={event => update({...settings, clear_build_data_after_build: event.target.checked})} /> {t('Clear build caches and compiler folders after each successful build')}</label>
      {dirty && <p className="hint">{t('Save settings to apply these choices.')}</p>}
    </div>
    {message && <p role="status">{outcomeText(message)}</p>}
    {report && <>
      <dl className="summary-list">
        <dt>{t('Total')}</dt><dd>{formatBytes(report.bytes)}</dd>
        <dt>{t('Can be cleared')}</dt><dd>{formatBytes(report.reclaimable)}</dd>
      </dl>
      <div className="table-scroll"><table className="storage-table">
        <thead><tr><th>{t('Project')}</th><th>{t('State')}</th><th>{t('Size')}</th><th>{t('Can clear')}</th><th><span className="sr-only">{t('Actions')}</span></th></tr></thead>
        <tbody>{report.projects.map(entry => <Fragment key={entry.id}>
          <tr>
            <th scope="row" title={entry.workspace ?? entry.id}>{projectLabel(entry)}</th>
            <td>{projectState(entry)}</td>
            <td>{formatBytes(entry.bytes)}</td>
            <td>{formatBytes(entry.reclaimable)}</td>
            <td className="row">
              <button className="tertiary" aria-expanded={expanded === entry.id} onClick={() => setExpanded(expanded === entry.id ? null : entry.id)}>{t('Details')}</button>
              <button className="tertiary" disabled={busy || entry.open || entry.locked} title={entry.open ? t('Open another project first') : undefined}
                onClick={() => void clear({project: entry.id, remove_project: true}, entry.project_file
                  ? t('Remove all stored data of “{project}” ({size}), including its built VPK and compiler folders? Its saved project file stays, but it must be prepared and built again. Installed mods in Deadlock are not touched.', {project: projectLabel(entry), size: formatBytes(entry.bytes)})
                  : t('Remove all stored data of “{project}” ({size}), including its built VPK and compiler folders? No saved project file is known for it. Installed mods in Deadlock are not touched.', {project: projectLabel(entry), size: formatBytes(entry.bytes)}))}>{t('Remove…')}</button>
            </td>
          </tr>
          {expanded === entry.id && <tr className="storage-details"><td colSpan={5}><table>
            <tbody>{entry.usage.map(usage => <tr key={usage.kind}>
              <th scope="row">{t(kinds[usage.kind].label)}<p className="hint">{(hint => t(hint.text, hint.values))(kinds[usage.kind].hint(settings))}</p></th>
              <td>{formatBytes(usage.bytes)}</td>
              <td>{clearable.includes(usage.kind) ? <button disabled={busy || entry.locked || !usage.reclaimable}
                onClick={() => void clear({project: entry.id, kinds: [usage.kind]}, t('Remove {size} of {what} from “{project}”?', {size: formatBytes(usage.reclaimable), what: t(kinds[usage.kind].label).toLowerCase(), project: projectLabel(entry)}))}>
                {t('Clear {size}', {size: formatBytes(usage.reclaimable)})}</button> : <span className="muted">{t('Kept')}</span>}</td>
            </tr>)}</tbody>
          </table></td></tr>}
        </Fragment>)}</tbody>
      </table></div>
      {report.shared.length > 0 && <>
        <h3>{t('Shared original hero files')}</h3>
        <p className="hint">{t('Extracted once per hero and game version and used by every project of that hero.')}</p>
        <div className="table-scroll"><table className="storage-table"><tbody>{report.shared.map(entry => <tr key={entry.key}>
          <th scope="row">{entry.model ? basename(entry.model) : entry.key}</th>
          <td>{entry.used_by.length ? tp(entry.used_by.length, {other: '{count} project(s)'}) : t('Not used')}</td>
          <td>{formatBytes(entry.bytes)}</td>
        </tr>)}</tbody></table></div>
        {unusedShared.length > 0 && <button disabled={busy} onClick={() => void clear({unused_shared: true}, tp(unusedShared.length, {other: 'Remove {count} unused original hero file set(s) ({size})?'}, {size: formatBytes(unusedShared.reduce((sum, entry) => sum + entry.bytes, 0))}))}>{t('Remove unused')}</button>}
      </>}
      <p className="hint">{t('Projects saved before this version are recognised once you open or save them.')} <button className="tertiary" disabled={busy} onClick={() => void find()}>{t('Find saved projects in a folder…')}</button></p>
    </>}
  </Section>;
}
