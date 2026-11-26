// i18n-area: settings
import { openFor } from '../dialogs';
import { Fragment, useEffect, useState } from 'react';
import type { FieldCheck, Settings, SetupStatus, SetupTest, ThemePreset } from '../models';
import { defaultFit } from '../models';
import { api, errorHelp, errorText, isDesktop } from '../api';
import { Disclosure, ErrorNotice, PageTitle, Section, StateLabel } from './Primitives';
import { CompactPathField } from './Paths';
import { Check } from './CustomizationPages';
import { themeById, type Theme } from '../theme/themes';
import { ThemeCards, ThemeCustomizer } from '../theme/ThemeCustomizer';
import { ToolDownloads, ToolLink } from '../setup/ToolDownloads';
import type { ToolLinkId } from '../setup/toolLinks';
import { m, mc, tc, tp, useT, type Language } from '../i18n';
import { T } from '../i18n/T';
import { LanguageRow } from '../i18n/LanguagePicker';

type PathKey = {[K in keyof Settings]-?: Settings[K] extends string ? K : never}[keyof Settings];
type PathField = {key: PathKey; label: string; directory: boolean; example: string; download?: ToolLinkId; hint?: string};
/** Each field says what to pick and shows a real example path. */
/** The names the backend's path check reports (`files_found` keys, kept English), as drawn: worded as the fields above. */
const PATH_CHECK_NAMES: Record<string, string> = {
  Deadlock: mc('path check', 'Deadlock'), 'Original VPK': mc('path check', 'Original VPK'), CSDK: mc('path check', 'CSDK'),
  'Resource compiler': mc('path check', 'Resource compiler'), 'Source 2 Viewer CLI': mc('path check', 'Source 2 Viewer CLI'),
};
const pathCheckName = (name: string) => Object.prototype.hasOwnProperty.call(PATH_CHECK_NAMES, name) ? tc('path check', PATH_CHECK_NAMES[name]) : name;
const pathFields: PathField[] = [
  {key: 'workspace', label: m('Workspace folder'), directory: true, example: String.raw`C:\Users\<you>\AppData\Local\DeadlockModBuilder\workspaces`, hint: m('Where the app keeps project files and builds. Detect tools fills in a default.')},
  {key: 'deadlock', label: m('Deadlock installation'), directory: true, download: 'deadlock', example: String.raw`C:\Program Files (x86)\Steam\steamapps\common\Deadlock`, hint: m('The Deadlock folder itself (it contains a folder called game). In Steam: right-click Deadlock → Manage → Browse local files.')},
  {key: 'pak', label: m('Deadlock game archive'), directory: false, example: String.raw`...\steamapps\common\Deadlock\game\citadel\pak01_dir.vpk`, hint: m('The file pak01_dir.vpk inside game\\citadel. Not one of the numbered pak01_000.vpk files.')},
  {key: 'blender', label: m('Blender executable'), directory: false, download: 'blender', example: String.raw`C:\Program Files\Blender Foundation\Blender 5.2\blender.exe`, hint: m('Needed to prepare Blender, FBX, glTF and MMD models. Supported: Blender 4.2–4.5, 5.0, 5.2 LTS (recommended) and 5.3. MMD models (.pmx, .pmd) also need the MMD Tools add-on in this Blender.')},
  {key: 'source2viewer', label: 'Source 2 Viewer CLI', directory: false, download: 'source2viewer', example: String.raw`C:\Tools\Source2Viewer\Source2Viewer-CLI.exe`, hint: m('From the cli-windows-x64 download: Source2Viewer-CLI.exe. Source2Viewer.exe (the viewer app) does not work here.')},
  {key: 'csdk', label: m('Community SDK'), directory: true, download: 'csdk', example: String.raw`C:\CSDK13`, hint: m('The extracted Reduced_CSDK_13 folder, the one that directly contains game and content. It compiles the materials and the hero model. A short path such as C:\\CSDK13 avoids long-path errors.')},
  {key: 'resourcecompiler', label: m('Resource compiler'), directory: false, example: String.raw`C:\CSDK13\game\bin\win64\resourcecompiler.exe`, hint: m('The Community SDK’s own resourcecompiler.exe (in game\\bin\\win64, not the bin_server copy). Detect tools or the suggested path fills it in.')},
];
const advancedPathFields: PathField[] = [
  {key: 'steam', label: m('Steam installation'), directory: true, download: 'steam', example: String.raw`C:\Program Files (x86)\Steam`},
  {key: 'vpk_tool', label: m('External VPK tool'), directory: false, example: m('Leave empty'), hint: m('Leave blank with the included builtin packaging backend.')},
  {key: 'ag2_adapter', label: m('Animation graph adapter'), directory: false, example: m('Leave empty')},
];
/** Only for setups that still compile hero models with a toolset downloaded from Steam. */
const toolsetPathFields: PathField[] = [
  {key: 'cswin', label: m('Model compiler toolset'), directory: true, example: String.raw`C:\DeadlockModdingTools\DMBTools-1\toolset`, hint: m('The toolset folder from Set up model compiler…. Used only when no Reduced CSDK 13 is chosen as the Community SDK, or with the backend set to cswin. Never the Community SDK.')},
  {key: 'cswin_resourcecompiler', label: m('Model compiler executable'), directory: false, example: String.raw`C:\DeadlockModdingTools\DMBTools-1\toolset\game\bin\win64\resourcecompiler.exe`, hint: m('The compiler inside the toolset above.')},
  {key: 'model_game', label: m('Model compiler game folder'), directory: true, example: String.raw`C:\DeadlockModdingTools\DMBTools-1\toolset\game\csgo`, hint: m('Filled by Use this toolset in settings.')},
  {key: 'optional_cs2', label: m('Optional Counter-Strike 2 installation'), directory: true, download: 'cs2', example: String.raw`C:\Program Files (x86)\Steam\steamapps\common\Counter-Strike Global Offensive`},
];

/** Shortcuts handled in App.tsx. */
const SHORTCUTS: [string,string][] = [['Ctrl+S',m('Save the project')],['Ctrl+Shift+S',m('Save the project as…')],['Ctrl+O',m('Open a project')],['Ctrl+N',m('New project')],['Ctrl+B',m('Open Build & install and build when nothing blocks it')],['Ctrl+1 … Ctrl+5',m('Go to step 1 to 5')]];
/** The check under a path field, with a one-click fix when one is known. */
function FieldStatus({check, apply, busy}: {check?: FieldCheck; apply: (value: string) => void; busy: boolean}) {
  const t = useT();
  if (!check || check.state === 'optional') return null;
  return <div className={`field-check field-check-${check.state}`} role={check.state === 'problem' ? 'alert' : undefined}>
    <span className="field-check-mark" aria-hidden="true">{check.state === 'ok' ? '✓' : check.state === 'warning' ? '!' : '✕'}</span>
    <span>{check.message}</span>
    {check.fix && <button className="tertiary" disabled={busy} onClick={() => apply(check.fix!)} title={check.fix}>{t('Use {path}', {path: check.fix.length > 64 ? '…' + check.fix.slice(-60) : check.fix})}</button>}
  </div>;
}

/** MMD Tools for the chosen Blender: a note under Blender (only MMD models
 * need it), never a warning or a problem. */
function MmdToolsNote({check, busy}: {check?: FieldCheck; busy: boolean}) {
  if (!check) return null;
  // The add-on's page explains installing it; not one of the four installs.
  return <p className="hint mmd-tools-note" aria-label="MMD Tools">{check.state === 'ok' ? '✓ ' : ''}{check.message}{check.state !== 'ok' && <> <ToolLink id="mmdtools" disabled={busy}/></>}</p>;
}

/** Test my setup: runs the three checks and shows each result with its fix. */
export function SetupTestPanel({ test, busy, go }: { test: () => Promise<SetupTest | undefined>; busy: boolean; go?: (page: string) => void }) {
  const t = useT();
  const [result, setResult] = useState<SetupTest | null>(null);
  const [running, setRunning] = useState(false);
  async function start() {
    setRunning(true);
    try { const value = await test(); if (value) setResult(value); } finally { setRunning(false); }
  }
  return <div className="setup-test" aria-label={t('Test my setup')}>
    <div className="setup-test-head"><div><h3>{t('Test my setup')}</h3><p className="hint">{t('Checks every path, compiles one small test material with the Community SDK and asks the model compiler which format it reads (about 10 seconds). Nothing is kept.')}</p></div><button className="primary" disabled={busy || running} onClick={() => void start()}>{running ? t('Testing…') : result ? t('Test again') : t('Test my setup')}</button></div>
    {result && <><p className={`settings-check-summary ${result.passed ? 'all-good' : 'has-problems'}`} role="status">{result.passed ? t('Your setup works. Build your hero once to be sure the model compiles.') : t('Something needs fixing before the first build. See below.')}</p>
      <ol className="setup-test-steps">{result.steps.map(step => <li key={step.name} className={step.passed ? 'passed' : 'failed'}><strong>{step.passed ? '✓' : '✕'} {step.name}</strong><span>{step.passed ? step.detail : ''}</span>{!step.passed && (step.error ? <ErrorNotice title={step.detail} details={errorText(step.error)} help={errorHelp(step.error)} go={go}/> : <span>{step.detail}</span>)}</li>)}</ol></>}
  </div>;
}

/** Settings → About this build (Expert): version, licence and where the source is. */
export function AboutBuild({ version }: { version: string }) {
  const t = useT();
  return <Disclosure title={t('About this build')} hint={t('Version, license and source code')} id="app-about"><p>Deadlock Mod Builder {version}</p><p className="hint"><T text="Licensed under the GNU Affero General Public License v3.0 or later. Source code of this version: <code>{url}</code>, tag <code>v{version}</code>. Third-party notices are in the <code>licenses</code> folder beside the app." tags={{code: chunk => <code>{chunk}</code>}} values={{url: 'https://github.com/OverseerAlbedo/deadlock-mod-builder', version}}/></p></Disclosure>;
}

export function SettingsPage({ settings, update, persist, busy, dirty, fail, detect, status, check, onboarding, modelSetup, embedded = false, verify, expert = true, testSetup, go, theme = 'original', chooseTheme, guideShown, setGuideShown, customThemes = [], saveCustomThemes, fields: liftedFields, ignoredNotifications = 0, showIgnoredNotifications, savePreference, chooseLanguage }: {
  /** Applies and saves the interface language at once (the Language row is shown when given). */ chooseLanguage?: (language: Language) => void;
  /** Saves Default fit for new models or Load 3D previews automatically at once (shown when given). */ savePreference?: (values: Pick<Settings, 'default_fit_mode' | 'auto_previews'>) => void;
  /** The live field checks when the app runs them (undefined: this page runs its own; null: not read yet). */ fields?: FieldCheck[] | null;
  /** How many sidebar notifications are ignored for good, and how to show them again. */ ignoredNotifications?: number; showIgnoredNotifications?: () => void;
  /** Opens a page from an error's fix button. */ go?: (page: string) => void;
  /** The look (Appearance): saved at once, not with the tool paths. */ theme?: string; chooseTheme?: (theme: string) => void;
  /** Whether New-user setup is listed in the sidebar (hidden after Done, or here). */ guideShown?: boolean; setGuideShown?: (shown: boolean) => void;
  /** The custom themes (saved at once) and how to save the list. */ customThemes?: ThemePreset[]; saveCustomThemes?: (presets: ThemePreset[]) => void;
  /** Expert shows the compiler backend and format choices. */ expert?: boolean;
  /** Runs Test my setup with these settings (shown when given). */ testSetup?: () => Promise<SetupTest | undefined>;
  settings: Settings | null; update: (settings: Settings) => void; persist: () => void;
  detect: (extraRoot?:boolean)=>void; status: SetupStatus|null; check: ()=>void;
  busy: boolean; dirty: boolean; fail: (title: string, error: unknown) => void;
  onboarding?: () => void; modelSetup?: () => void;
  embedded?: boolean; verify?: () => void;
}) {
  const t = useT();
  const [fields, setFields] = useState<FieldCheck[] | null>(null);
  /** The theme editor, when open: the theme it starts from and the custom theme it edits. */
  const [editor, setEditor] = useState<{start: Theme; editing?: ThemePreset} | null>(null);
  // Live checks while typing or after choosing a path (files only; cheap), unless the app passes its own.
  const ownChecks = liftedFields === undefined;
  useEffect(() => {
    if (!ownChecks || !settings || !isDesktop()) return;
    let active = true;
    const timer = window.setTimeout(() => { void Promise.resolve().then(() => api.checkSettingFields(settings)).then(result => { if (active) setFields(result ?? null); }).catch(() => { if (active) setFields(null); }); }, 300);
    return () => { active = false; window.clearTimeout(timer); };
  }, [settings, ownChecks]);
  const checks = (ownChecks ? fields : liftedFields) ?? status?.fields ?? null;
  const checkFor = (key: string) => checks?.find(item => item.field === key);
  const fixable = (checks ?? []).filter(item => item.fix && item.state !== 'ok');
  const problems = (checks ?? []).filter(item => item.state === 'problem');
  async function choose(field: PathField) {
    try {
      const path = await openFor('tools-folder', {title: t(field.label), directory: field.directory, multiple: false});
      if (typeof path === 'string' && settings) update({...settings, [field.key]: path});
    } catch (error) { fail(m('Could not choose a path'), error); }
  }
  async function fixAll() {
    if (!settings) return;
    try { update(await api.fixSettingFields(settings)); } catch (error) { fail(m('Could not apply the suggested paths'), error); }
  }
  const renderPaths = (list: PathField[]) => list.map(field => <div key={field.key} className="settings-path-field">
    <CompactPathField fullPath label={t(field.label)} value={settings?.[field.key] ?? ''} disabled={busy} update={value => settings && update({...settings, [field.key]: value})} browse={() => choose(field)} placeholder={t('Example: {example}', {example: t(field.example)})} />
    <FieldStatus check={checkFor(field.key)} busy={busy} apply={value => settings && update({...settings, [field.key]: value})}/>
    {(field.download || field.hint) && <p className="hint tool-field-help">{field.hint && <span>{t(field.hint)} </span>}<span className="field-example"><T text="Example: <code>{example}</code>" tags={{code: chunk => <code>{chunk}</code>}} values={{example: t(field.example)}}/> </span>{field.download && <ToolLink id={field.download} disabled={busy}/>}</p>}
    {field.key === 'blender' && <MmdToolsNote check={checkFor('mmd_tools')} busy={busy}/>}
  </div>);
  const modelCheck = checkFor('cswin');
  return <>
    {!embedded && <PageTitle eyebrow={t('Preferences & tools')} title={t('Settings')} subtitle={t('Set up your tools and choose where projects are built.')} action={<div className="settings-save-action"><span className="muted" role="status">{!settings?t('Loading settings…'):dirty?t('Unsaved changes'):t('Settings saved')}</span><button className="primary" disabled={busy || !settings || !dirty} onClick={persist}>{t('Save settings')}</button></div>} />}
    {!settings ? <p className="empty-state">{t('Settings are not available. Open the desktop application to load your configuration.')}</p> : <>
      {!embedded && onboarding && <div className="setup-entry"><div><h2>{t('Getting started')}</h2><p>{t('Reopen the setup guide.')}</p></div><div className="row"><button disabled={busy} onClick={onboarding}>{t('Open new-user setup')}</button></div></div>}
      <ToolDownloads busy={busy} expanded={embedded}/>
      {!embedded && chooseTheme && <Section title={t('Appearance')} id="appearance">
        {chooseLanguage && <LanguageRow choose={chooseLanguage} disabled={busy}/>}
        <ThemeCards active={theme} customs={customThemes} disabled={busy} onChoose={chooseTheme} onCustomize={() => setEditor({start: themeById(theme, customThemes)})} onEdit={preset => setEditor({start: themeById(preset.id, customThemes), editing: preset})} onDelete={id => { if (saveCustomThemes) { saveCustomThemes(customThemes.filter(custom => custom.id !== id)); if (id === theme) chooseTheme('original'); } }}/>
        {editor && <ThemeCustomizer start={editor.start} editing={editor.editing} customs={customThemes} disabled={busy} onCancel={() => setEditor(null)} onSave={preset => { saveCustomThemes?.([...customThemes.filter(custom => custom.id !== preset.id), preset]); chooseTheme(preset.id); setEditor(null); }}/>}
        <p className="hint">{t('A theme changes at once and is kept with your settings, so it survives restarts and reinstalls. The mark in the sidebar and the window icon (title bar and taskbar) follow it. Customize… makes your own from any theme: a color picker or a hex code for each color, with a live preview.')}</p>
        {setGuideShown && <><Check label={t('Show New-user setup in the sidebar (Getting started)')} value={guideShown !== false} onChange={setGuideShown} disabled={busy}/><p className="hint">{t("Hide it once you know the five steps; Done on the guide's last step hides it as well. It stays available from Open new-user setup below.")}</p></>}
        {savePreference && <div className="settings-preferences">
          <div className="field"><label htmlFor="settings-default-fit">{t('Default fit for new models')}</label>
            <select id="settings-default-fit" disabled={busy} value={defaultFit(settings)} onChange={event => savePreference({default_fit_mode: event.target.value})}>
              <option value="preserve_proportions">{t('Preserve source proportions')}</option>
              <option value="match_target">{t('Match hero proportions')}</option>
            </select></div>
          <p className="hint">{t('Model fitting for a model checked for the first time in a project (Prepare model → Fit & review, where you can still change it). Projects checked before keep their own fit.')}</p>
          <Check label={t('Load 3D previews automatically')} value={settings.auto_previews ?? false} onChange={auto_previews => savePreference({auto_previews})} disabled={busy}/>
          <p className="hint">{t('Prepare model, Appearance & weapons and Physics & preview then load their 3D view by themselves when the page is shown: once per model, and only when nothing else is running. Previews of Blender, FBX and MMD files start Blender, which takes a while on slower PCs. Off: each view loads with its button.')}</p>
        </div>}
        {ignoredNotifications > 0 && showIgnoredNotifications && <div className="row"><button disabled={busy} onClick={showIgnoredNotifications}>{t('Show ignored notifications again')}</button><span className="hint">{tp(ignoredNotifications, {one: "One notification is ignored in the sidebar's Notifications.", other: "{count} notifications are ignored in the sidebar's Notifications."})}</span></div>}
      </Section>}
      <Section title={t('Folders and tools')} actions={<div className="row"><button disabled={busy} onClick={()=>detect()}>{t('Detect tools')}</button><button disabled={busy} onClick={()=>detect(true)}>{t('Search a folder…')}</button>{fixable.length > 0 && <button disabled={busy} onClick={() => void fixAll()}>{tp(fixable.length, {one: 'Fix the marked path', other: 'Fix all {count} marked paths'})}</button>}{embedded && verify ? <button className="primary" disabled={busy} onClick={verify}>{t('Save and check tools')}</button> : <button disabled={busy} onClick={check}>{t('Check paths')}</button>}</div>}>
        {checks && <p className={`settings-check-summary ${problems.length ? 'has-problems' : 'all-good'}`} role="status">{problems.length === 0 ? t('All required paths look right.') : fixable.length
          ? tp(problems.length, {one: '{count} path needs fixing. Each one says what is wrong; use the suggested path where offered.', other: '{count} paths need fixing. Each one says what is wrong; use the suggested path where offered.'})
          : tp(problems.length, {one: '{count} path needs fixing. Each one says what is wrong.', other: '{count} paths need fixing. Each one says what is wrong.'})}</p>}
        <div className="settings-fields">{renderPaths(pathFields)}</div>
        {modelCheck && <div className="settings-model-check"><strong>{t('Model compiler')}</strong><FieldStatus check={modelCheck} busy={busy} apply={value => settings && update({...settings, cswin: value})}/>{modelCheck.backend && settings.model_backend !== modelCheck.backend && <button className="primary" disabled={busy} onClick={() => update({...settings, model_backend: modelCheck.backend!})}>{t('Compile models with Reduced CSDK 13')}</button>}</div>}
        {testSetup && <SetupTestPanel test={testSetup} busy={busy} go={go}/>}
      </Section>
      {status&&<Section title={t('Path check')}><dl className="summary-list">{Object.entries(status.files_found).map(([name,found])=><Fragment key={name}><dt>{pathCheckName(name)}</dt><dd><StateLabel tone={found?'ready':'attention'}>{found?t('Found'):t('Not found')}</StateLabel></dd></Fragment>)}<dt>{t('Model compiler')}</dt><dd><StateLabel tone={status.model_files_found?'ready':'attention'}>{status.model_files_found?t('Found'):t('Not set up')}</StateLabel></dd></dl><p className="hint">{status.model_status}</p></Section>}
      <details className="disclosure"><summary>{t('Advanced configuration')}</summary><div className="disclosure-content settings-fields">{renderPaths(advancedPathFields)}
        <details className="disclosure settings-toolset"><summary>{t('Downloaded model toolset (older setups)')}</summary><div className="disclosure-content">
          <p className="hint">{t('Not needed with Reduced CSDK 13. Only for setups that still compile hero models with a toolset downloaded from Steam.')}</p>
          {modelSetup && <div className="row"><button disabled={busy} onClick={modelSetup}>{t('Set up model compiler…')}</button></div>}
          <div className="settings-fields">{renderPaths(toolsetPathFields)}</div>
        </div></details>
        {expert&&([{key:'model_backend', label:m('Model compiler backend'), values:['auto','cswin','csdk']},{key:'model_format',label:m('ModelDoc format'),values:['auto','40','41']},{key:'vpk_backend',label:m('Packaging backend'),values:['builtin','valve']}] as const).map(field => <div className="field" key={field.key}><label htmlFor={`settings-${field.key}`}>{t(field.label)}</label><select id={`settings-${field.key}`} disabled={busy} value={settings[field.key]} onChange={event => update({...settings, [field.key]: event.target.value})}>{[...new Set([...field.values, settings[field.key]])].map(value => <option key={value} value={value}>{value}</option>)}</select></div>)}
        {expert&&<p className="hint">{t('Leave the backend on auto: Reduced CSDK 13 compiles the models; without it, a downloaded model toolset is used. cswin forces the toolset, csdk forces the Community SDK.')}</p>}
      </div></details>
      {!embedded&&<details className="disclosure"><summary>{t('Keyboard shortcuts')}</summary><div className="disclosure-content"><dl className="summary-list shortcut-list">{SHORTCUTS.map(([keys,what])=><Fragment key={keys}><dt><kbd>{keys}</kbd></dt><dd>{t(what)}</dd></Fragment>)}</dl></div></details>}
    </>}
  </>;
}
