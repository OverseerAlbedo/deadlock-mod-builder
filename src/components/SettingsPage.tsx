import { open } from '@tauri-apps/plugin-dialog';
import { Fragment, useEffect, useState } from 'react';
import type { FieldCheck, Settings, SetupStatus, SetupTest } from '../models';
import { api, errorHelp, errorText, isDesktop } from '../api';
import { ErrorNotice, PageTitle, Section, StateLabel } from './Primitives';
import { CompactPathField } from './Paths';
import { ToolDownloads, ToolLink } from '../setup/ToolDownloads';
import type { ToolLinkId } from '../setup/toolLinks';

type PathKey = {[K in keyof Settings]-?: Settings[K] extends string ? K : never}[keyof Settings];
type PathField = {key: PathKey; label: string; directory: boolean; example: string; download?: ToolLinkId; hint?: string};
/** Each field says what to pick and shows a real example path. */
const pathFields: PathField[] = [
  {key: 'workspace', label: 'Workspace folder', directory: true, example: String.raw`C:\Users\<you>\AppData\Local\DeadlockModBuilder\workspaces`, hint: 'Where the app keeps project files and builds. Detect tools fills in a default.'},
  {key: 'deadlock', label: 'Deadlock installation', directory: true, download: 'deadlock', example: String.raw`C:\Program Files (x86)\Steam\steamapps\common\Deadlock`, hint: 'The Deadlock folder itself (it contains a folder called game). In Steam: right-click Deadlock → Manage → Browse local files.'},
  {key: 'pak', label: 'Deadlock game archive', directory: false, example: String.raw`...\steamapps\common\Deadlock\game\citadel\pak01_dir.vpk`, hint: 'The file pak01_dir.vpk inside game\\citadel. Not one of the numbered pak01_000.vpk files.'},
  {key: 'source2viewer', label: 'Source 2 Viewer CLI', directory: false, download: 'source2viewer', example: String.raw`C:\Tools\Source2Viewer\Source2Viewer-CLI.exe`, hint: 'From the cli-windows-x64 download: Source2Viewer-CLI.exe. Source2Viewer.exe (the viewer app) does not work here.'},
  {key: 'resourcecompiler', label: 'Resource compiler', directory: false, example: String.raw`C:\CSDK12\game\bin_cs2\win64\resourcecompiler.exe`, hint: 'The material compiler: resourcecompiler.exe in the Community SDK’s game\\bin_cs2\\win64 folder. The SDK has other copies; use the bin_cs2 one.'},
  {key: 'csdk', label: 'Community SDK', directory: true, download: 'csdk', example: String.raw`C:\CSDK12`, hint: 'The extracted Reduced_CSDK_12 folder, the one that directly contains game and content. A short path such as C:\\CSDK12 avoids long-path errors.'},
];
const advancedPathFields: PathField[] = [
  {key: 'steam', label: 'Steam installation', directory: true, download: 'steam', example: String.raw`C:\Program Files (x86)\Steam`},
  {key: 'model_game', label: 'Model compiler game folder', directory: true, example: String.raw`C:\DeadlockModdingTools\DMBTools-1\toolset\game\csgo`, hint: 'Filled by Use this toolset in settings.'},
  {key: 'cswin', label: 'Model compiler toolset', directory: true, example: String.raw`C:\DeadlockModdingTools\DMBTools-1\toolset`, hint: 'The toolset folder from Set up model compiler…. Never the Community SDK.'},
  {key: 'cswin_resourcecompiler', label: 'Model compiler executable', directory: false, example: String.raw`C:\DeadlockModdingTools\DMBTools-1\toolset\game\bin\win64\resourcecompiler.exe`, hint: 'The compiler inside the toolset above.'},
  {key: 'optional_cs2', label: 'Optional Counter-Strike 2 installation', directory: true, download: 'cs2', example: String.raw`C:\Program Files (x86)\Steam\steamapps\common\Counter-Strike Global Offensive`},
  {key: 'vpk_tool', label: 'External VPK tool', directory: false, example: 'Leave empty', hint: 'Leave blank with the included builtin packaging backend.'},
  {key: 'ag2_adapter', label: 'Animation graph adapter', directory: false, example: 'Leave empty'},
  {key: 'blender', label: 'Blender executable', directory: false, download: 'blender', example: String.raw`C:\Program Files\Blender Foundation\Blender 5.2\blender.exe`, hint: 'Needed to prepare Blender, FBX and glTF models. Supported: Blender 4.2–4.5, 5.0 and 5.2 LTS.'},
];

/** Shortcuts handled in App.tsx. */
const SHORTCUTS: [string,string][] = [['Ctrl+S','Save the project'],['Ctrl+Shift+S','Save the project as…'],['Ctrl+O','Open a project'],['Ctrl+N','New project'],['Ctrl+B','Open Build & install and build when nothing blocks it'],['Ctrl+1 … Ctrl+5','Go to step 1 to 5']];
/** The check under a path field, with a one-click fix when one is known. */
function FieldStatus({check, apply, busy}: {check?: FieldCheck; apply: (value: string) => void; busy: boolean}) {
  if (!check || check.state === 'optional') return null;
  return <div className={`field-check field-check-${check.state}`} role={check.state === 'problem' ? 'alert' : undefined}>
    <span className="field-check-mark" aria-hidden="true">{check.state === 'ok' ? '✓' : check.state === 'warning' ? '!' : '✕'}</span>
    <span>{check.message}</span>
    {check.fix && <button className="tertiary" disabled={busy} onClick={() => apply(check.fix!)} title={check.fix}>Use {check.fix.length > 64 ? '…' + check.fix.slice(-60) : check.fix}</button>}
  </div>;
}

/** Test my setup: runs the three checks and shows each result with its fix. */
export function SetupTestPanel({ test, busy, go }: { test: () => Promise<SetupTest | undefined>; busy: boolean; go?: (page: string) => void }) {
  const [result, setResult] = useState<SetupTest | null>(null);
  const [running, setRunning] = useState(false);
  async function start() {
    setRunning(true);
    try { const value = await test(); if (value) setResult(value); } finally { setRunning(false); }
  }
  return <div className="setup-test" aria-label="Test my setup">
    <div className="setup-test-head"><div><h3>Test my setup</h3><p className="hint">Checks every path, compiles one small test material with the Community SDK and asks the model compiler which format it reads (about 10 seconds). Nothing is kept.</p></div><button className="primary" disabled={busy || running} onClick={() => void start()}>{running ? 'Testing…' : result ? 'Test again' : 'Test my setup'}</button></div>
    {result && <><p className={`settings-check-summary ${result.passed ? 'all-good' : 'has-problems'}`} role="status">{result.passed ? 'Your setup works. Build your hero once to be sure the model compiles.' : 'Something needs fixing before the first build. See below.'}</p>
      <ol className="setup-test-steps">{result.steps.map(step => <li key={step.name} className={step.passed ? 'passed' : 'failed'}><strong>{step.passed ? '✓' : '✕'} {step.name}</strong><span>{step.passed ? step.detail : ''}</span>{!step.passed && (step.error ? <ErrorNotice title={step.detail} details={errorText(step.error)} help={errorHelp(step.error)} go={go}/> : <span>{step.detail}</span>)}</li>)}</ol></>}
  </div>;
}

export function SettingsPage({ settings, update, persist, busy, dirty, fail, detect, status, check, onboarding, modelSetup, embedded = false, verify, expert = true, testSetup, go }: {
  /** Opens a page from an error's fix button. */ go?: (page: string) => void;
  /** Expert shows the compiler backend and format choices. */ expert?: boolean;
  /** Runs Test my setup with these settings (shown when given). */ testSetup?: () => Promise<SetupTest | undefined>;
  settings: Settings | null; update: (settings: Settings) => void; persist: () => void;
  detect: (extraRoot?:boolean)=>void; status: SetupStatus|null; check: ()=>void;
  busy: boolean; dirty: boolean; fail: (title: string, error: unknown) => void;
  onboarding?: () => void; modelSetup?: () => void;
  embedded?: boolean; verify?: () => void;
}) {
  const [fields, setFields] = useState<FieldCheck[] | null>(null);
  // Live checks while typing or after choosing a path (files only; cheap).
  useEffect(() => {
    if (!settings || !isDesktop()) return;
    let active = true;
    const timer = window.setTimeout(() => { void Promise.resolve().then(() => api.checkSettingFields(settings)).then(result => { if (active) setFields(result ?? null); }).catch(() => { if (active) setFields(null); }); }, 300);
    return () => { active = false; window.clearTimeout(timer); };
  }, [settings]);
  const checks = fields ?? status?.fields ?? null;
  const checkFor = (key: string) => checks?.find(item => item.field === key);
  const fixable = (checks ?? []).filter(item => item.fix && item.state !== 'ok');
  const problems = (checks ?? []).filter(item => item.state === 'problem');
  async function choose(field: PathField) {
    try {
      const path = await open({title: field.label, directory: field.directory, multiple: false});
      if (typeof path === 'string' && settings) update({...settings, [field.key]: path});
    } catch (error) { fail('Could not choose a path', error); }
  }
  async function fixAll() {
    if (!settings) return;
    try { update(await api.fixSettingFields(settings)); } catch (error) { fail('Could not apply the suggested paths', error); }
  }
  const renderPaths = (list: PathField[]) => list.map(field => <div key={field.key} className="settings-path-field">
    <CompactPathField fullPath label={field.label} value={settings?.[field.key] ?? ''} disabled={busy} update={value => settings && update({...settings, [field.key]: value})} browse={() => choose(field)} placeholder={`Example: ${field.example}`} />
    <FieldStatus check={checkFor(field.key)} busy={busy} apply={value => settings && update({...settings, [field.key]: value})}/>
    {(field.download || field.hint) && <p className="hint tool-field-help">{field.hint && <span>{field.hint} </span>}<span className="field-example">Example: <code>{field.example}</code> </span>{field.download && <ToolLink id={field.download} disabled={busy}/>}</p>}
  </div>);
  const modelCheck = checkFor('cswin');
  return <>
    {!embedded && <PageTitle eyebrow="Preferences & tools" title="Settings" subtitle="Set up your tools and choose where projects are built." action={<div className="settings-save-action"><span className="muted" role="status">{!settings?'Loading settings…':dirty?'Unsaved changes':'Settings saved'}</span><button className="primary" disabled={busy || !settings || !dirty} onClick={persist}>Save settings</button></div>} />}
    {!settings ? <p className="empty-state">Settings are not available. Open the desktop application to load your configuration.</p> : <>
      {!embedded && (onboarding || modelSetup) && <div className="setup-entry"><div><h2>Getting started</h2><p>Reopen the setup guide or configure your model compiler.</p></div><div className="row">{onboarding && <button disabled={busy} onClick={onboarding}>Open new-user setup</button>}{modelSetup && <button className="tertiary" disabled={busy} onClick={modelSetup}>Set up model compiler…</button>}</div></div>}
      <ToolDownloads busy={busy} expanded={embedded} modelSetup={modelSetup}/>
      <Section title="Folders and tools" actions={<div className="row"><button disabled={busy} onClick={()=>detect()}>Detect tools</button><button disabled={busy} onClick={()=>detect(true)}>Search a folder…</button>{fixable.length > 0 && <button disabled={busy} onClick={() => void fixAll()}>Fix {fixable.length === 1 ? 'the marked path' : `all ${fixable.length} marked paths`}</button>}{embedded && verify ? <button className="primary" disabled={busy} onClick={verify}>Save and check tools</button> : <button disabled={busy} onClick={check}>Check paths</button>}</div>}>
        {checks && <p className={`settings-check-summary ${problems.length ? 'has-problems' : 'all-good'}`} role="status">{problems.length === 0 ? 'All required paths look right.' : `${problems.length} ${problems.length === 1 ? 'path needs' : 'paths need'} fixing. Each one says what is wrong${fixable.length ? '; use the suggested path where offered' : ''}.`}</p>}
        <div className="settings-fields">{renderPaths(pathFields)}</div>
        {modelCheck && <div className="settings-model-check"><strong>Model compiler</strong><FieldStatus check={modelCheck} busy={busy} apply={value => settings && update({...settings, cswin: value})}/>{modelCheck.state !== 'ok' && modelSetup && <button className="primary" disabled={busy} onClick={modelSetup}>Set up model compiler…</button>}</div>}
        {testSetup && <SetupTestPanel test={testSetup} busy={busy} go={go}/>}
      </Section>
      {status&&<Section title="Path check"><dl className="summary-list">{Object.entries(status.files_found).map(([name,found])=><Fragment key={name}><dt>{name}</dt><dd><StateLabel tone={found?'ready':'attention'}>{found?"Found":"Not found"}</StateLabel></dd></Fragment>)}<dt>Model compiler</dt><dd><StateLabel tone={status.model_files_found?'ready':'attention'}>{status.model_files_found?'Found':'Not set up'}</StateLabel></dd></dl><p className="hint">{status.model_status}</p></Section>}
      <details className="disclosure"><summary>Advanced configuration</summary><div className="disclosure-content settings-fields">{renderPaths(advancedPathFields)}
        {expert&&([{key:'model_backend', label:'Model compiler backend', values:['auto','cswin','csdk']},{key:'model_format',label:'ModelDoc format',values:['auto','40','41']},{key:'vpk_backend',label:'Packaging backend',values:['builtin','valve']}] as const).map(field => <div className="field" key={field.key}><label htmlFor={`settings-${field.key}`}>{field.label}</label><select id={`settings-${field.key}`} disabled={busy} value={settings[field.key]} onChange={event => update({...settings, [field.key]: event.target.value})}>{[...new Set([...field.values, settings[field.key]])].map(value => <option key={value} value={value}>{value}</option>)}</select></div>)}
        {expert&&<p className="hint">Leave the backend on auto. The csdk backend cannot compile hero models; it exists for diagnostics.</p>}
      </div></details>
      {!embedded&&<details className="disclosure"><summary>Keyboard shortcuts</summary><div className="disclosure-content"><dl className="summary-list shortcut-list">{SHORTCUTS.map(([keys,what])=><Fragment key={keys}><dt><kbd>{keys}</kbd></dt><dd>{what}</dd></Fragment>)}</dl></div></details>}
    </>}
  </>;
}
