import { open } from '@tauri-apps/plugin-dialog';
import { Fragment } from 'react';
import type { Settings, SetupStatus } from '../models';
import { PageTitle, Section, StateLabel } from './Primitives';
import { CompactPathField } from './Paths';
import { ToolDownloads, ToolLink } from '../setup/ToolDownloads';
import type { ToolLinkId } from '../setup/toolLinks';

const pathFields: {key: keyof Settings; label: string; directory: boolean; download?: ToolLinkId; hint?: string}[] = [
  {key: 'workspace', label: 'Workspace folder', directory: true},
  {key: 'deadlock', label: 'Deadlock installation', directory: true, download: 'deadlock'},
  {key: 'pak', label: 'Deadlock game archive', directory: false},
  {key: 'source2viewer', label: 'Source 2 Viewer CLI', directory: false, download: 'source2viewer', hint: 'Choose the Windows x64 command-line download and select Source2Viewer-CLI.exe.'},
  {key: 'resourcecompiler', label: 'Resource compiler', directory: false, hint: 'Included in Reduced CSDK 12. Select its game/bin_cs2/win64/resourcecompiler.exe for materials.'},
  {key: 'csdk', label: 'Community SDK', directory: true, download: 'csdk'},
];
const advancedPathFields: typeof pathFields = [
  {key: 'steam', label: 'Steam installation', directory: true, download: 'steam'},
  {key: 'model_game', label: 'Model compiler game folder', directory: true},
  {key: 'cswin', label: 'Model compiler toolset', directory: true},
  {key: 'cswin_resourcecompiler', label: 'Model compiler executable', directory: false},
  {key: 'optional_cs2', label: 'Optional Counter-Strike 2 installation', directory: true, download: 'cs2'},
  {key: 'vpk_tool', label: 'External VPK tool', directory: false, hint: 'Leave blank with the included builtin packaging backend.'},
  {key: 'ag2_adapter', label: 'Animation graph adapter', directory: false},
  {key: 'blender', label: 'Blender executable', directory: false, download: 'blender', hint: 'Optional for a prepared DMX. The model-preparation bridge supports Blender 4.2–4.5, 5.0 and 5.2 LTS; the link opens 5.2 LTS.'},
];
export function SettingsPage({ settings, update, persist, busy, dirty, fail, detect, status, check, onboarding, modelSetup, embedded = false, verify }: {
  settings: Settings | null; update: (settings: Settings) => void; persist: () => void;
  detect: (extraRoot?:boolean)=>void; status: SetupStatus|null; check: ()=>void;
  busy: boolean; dirty: boolean; fail: (title: string, error: unknown) => void;
  onboarding?: () => void; modelSetup?: () => void;
  embedded?: boolean; verify?: () => void;
}) {
  async function choose(field: typeof pathFields[number]) {
    try {
      const path = await open({title: field.label, directory: field.directory, multiple: false});
      if (typeof path === 'string' && settings) update({...settings, [field.key]: path});
    } catch (error) { fail('Could not choose a path', error); }
  }
  const renderPaths = (fields: typeof pathFields) => fields.map(field => <div key={field.key}><CompactPathField label={field.label} value={settings?.[field.key] ?? ''} disabled={busy} update={value => settings && update({...settings, [field.key]: value})} browse={() => choose(field)} placeholder="Choose a location…" />{(field.download || field.hint) && <p className="hint tool-field-help">{field.hint && <span>{field.hint} </span>}{field.download && <ToolLink id={field.download} disabled={busy}/>}</p>}</div>);
  return <>
    {!embedded && <PageTitle eyebrow="Preferences & tools" title="Settings" subtitle="Set up your tools and choose where projects are built." action={<div className="settings-save-action"><span className="muted" role="status">{!settings?'Loading settings…':dirty?'Unsaved changes':'Settings saved'}</span><button className="primary" disabled={busy || !settings || !dirty} onClick={persist}>Save settings</button></div>} />}
    {!settings ? <p className="empty-state">Settings are not available. Open the desktop application to load your configuration.</p> : <>
      {!embedded && (onboarding || modelSetup) && <div className="setup-entry"><div><h2>Getting started</h2><p>Reopen the setup guide or configure your model compiler.</p></div><div className="row">{onboarding && <button disabled={busy} onClick={onboarding}>Open new-user setup</button>}{modelSetup && <button className="tertiary" disabled={busy} onClick={modelSetup}>Set up model compiler…</button>}</div></div>}
      <ToolDownloads busy={busy} expanded={embedded} modelSetup={modelSetup}/>
      <Section title="Folders and tools" actions={<div className="row"><button disabled={busy} onClick={()=>detect()}>Detect tools</button><button disabled={busy} onClick={()=>detect(true)}>Search a folder…</button>{embedded && verify ? <button className="primary" disabled={busy} onClick={verify}>Save and check tools</button> : <button disabled={busy} onClick={check}>Check paths</button>}</div>}><div className="settings-fields">{renderPaths(pathFields)}</div></Section>
      {status&&<Section title="Path check"><dl className="summary-list">{Object.entries(status.files_found).map(([name,found])=><Fragment key={name}><dt>{name}</dt><dd><StateLabel tone={found?'ready':'attention'}>{found?"Found":"Not found"}</StateLabel></dd></Fragment>)}</dl><p className="hint">{status.model_status}</p><p className="hint">File presence is checked against the current fields. Compilation and compatibility still require project validation.</p></Section>}
      <details className="disclosure"><summary>Advanced configuration</summary><div className="disclosure-content settings-fields">{renderPaths(advancedPathFields)}
        {([{key:'model_backend', label:'Model compiler backend', values:['auto','cswin','csdk']},{key:'model_format',label:'ModelDoc format',values:['auto','40','41']},{key:'vpk_backend',label:'Packaging backend',values:['builtin','valve']}] as const).map(field => <div className="field" key={field.key}><label htmlFor={`settings-${field.key}`}>{field.label}</label><select id={`settings-${field.key}`} disabled={busy} value={settings[field.key]} onChange={event => update({...settings, [field.key]: event.target.value})}>{[...new Set([...field.values, settings[field.key]])].map(value => <option key={value} value={value}>{value}</option>)}</select></div>)}
      </div></details>
      <p className="hint">Changing a tool path does not certify that tool or validate a project. The build still validates the selected tools and current project.</p>
    </>}
  </>;
}
