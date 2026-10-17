import { useState } from 'react';
import { ask } from '@tauri-apps/plugin-dialog';
import { api } from '../api';
import type { InstalledMod, ModUpdateReport } from '../models';
import { Section, StateLabel } from '../components/Primitives';

const STATUS: Record<InstalledMod['status'], {label: string; tone: 'ready' | 'attention' | 'neutral'}> = {
  up_to_date: {label: 'Up to date', tone: 'ready'},
  needs_update: {label: 'Needs update', tone: 'attention'},
  no_hero_model: {label: 'No hero model', tone: 'neutral'},
  unknown: {label: 'Left alone', tone: 'neutral'},
  cannot_update: {label: 'Rebuild needed', tone: 'attention'},
};
const megabytes = (bytes: number) => `${(bytes / 1048576).toFixed(bytes < 10 * 1048576 ? 1 : 0)} MB`;
function describe(error: unknown): string {
  if (error && typeof error === 'object' && 'message' in error) return String((error as {message: unknown}).message);
  return String(error);
}

/** Installed mods after a game update: every VPK in the game's addons folder with the state of
 * its hero models, and an in-place update for the ones compiled before the game's October 16,
 * 2026 attachment change (camera pivots rotated by 90 degrees in a match). */
export function InstalledMods({busy = false}: {busy?: boolean}) {
  const [mods, setMods] = useState<InstalledMod[] | null>(null);
  const [working, setWorking] = useState<'scan' | 'update' | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [reports, setReports] = useState<ModUpdateReport[]>([]);
  const affected = (mods ?? []).filter(mod => mod.status === 'needs_update');
  async function scan() {
    setWorking('scan'); setError(null); setReports([]);
    try { setMods(await api.scanInstalledMods()); } catch (cause) { setError(describe(cause)); } finally { setWorking(null); }
  }
  async function updateAll() {
    const names = affected.map(mod => mod.file);
    let confirmed = true;
    try {
      confirmed = await ask(`Update ${names.length === 1 ? names[0] : `${names.length} mods`} in the game's addons folder? The hero models inside take the game's current attachment values; the previous files are kept as backups.`, {title: 'Update installed mods', kind: 'warning'});
    } catch { /* no dialog outside the desktop app */ }
    if (!confirmed) return;
    setWorking('update'); setError(null);
    try {
      const done = await api.updateInstalledMods(affected.map(mod => mod.path));
      setReports(done);
      setMods(await api.scanInstalledMods());
    } catch (cause) { setError(describe(cause)); } finally { setWorking(null); }
  }
  const disabled = busy || working !== null;
  return <Section title="Installed mods" id="installed-mods" actions={<div className="row">
      <button disabled={disabled} onClick={() => void scan()}>{working === 'scan' ? 'Checking…' : mods ? 'Check again' : 'Check installed mods'}</button>
      {affected.length > 0 && <button className="primary" disabled={disabled} onClick={() => void updateAll()}>{working === 'update' ? 'Updating…' : `Update ${affected.length} ${affected.length === 1 ? 'mod' : 'mods'}`}</button>}
    </div>}>
    <p className="hint">The game update of October 16, 2026 changed how camera attachments are stored in hero models. Mods built before it show the camera 90 degrees off in a match. This checks every VPK in the game's addons folder and can update the affected ones in place, whichever tool made them; the previous files are kept as backups.</p>
    {error && <p className="error-text" role="alert">{error}</p>}
    {reports.length > 0 && <p role="status" className="ok-text">Updated {reports.map(report => `${report.file} (${report.updated.length} ${report.updated.length === 1 ? 'model' : 'models'})`).join(', ')}. Start the game and check the camera in a match; the previous files are in the app's backups folder.</p>}
    {mods && mods.length === 0 && <p role="status">No mods are installed in the game's addons folder.</p>}
    {mods && mods.length > 0 && <div className="table-scroll"><table><caption className="sr-only">Installed mods</caption>
      <thead><tr><th scope="col">File</th><th scope="col">Hero</th><th scope="col">Status</th><th scope="col">Note</th></tr></thead>
      <tbody>{mods.map(mod => <tr key={mod.path}>
        <th scope="row">{mod.file} <span className="muted">{megabytes(mod.bytes)}</span></th>
        <td>{mod.models.length === 0 ? '—' : Array.from(new Set(mod.models.map(model => model.hero ?? model.path))).join(', ')}</td>
        <td><StateLabel tone={STATUS[mod.status].tone}>{STATUS[mod.status].label}</StateLabel></td>
        <td className="validation-detail">{mod.note}</td>
      </tr>)}</tbody></table></div>}
  </Section>;
}
