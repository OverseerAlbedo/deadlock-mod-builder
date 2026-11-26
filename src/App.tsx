// i18n-area: shell
import { version as APP_VERSION } from "../package.json";
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { ask } from '@tauri-apps/plugin-dialog';
import { openFor, saveFor } from './dialogs';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { api, basename, errorHelp, errorText, isDesktop, type VpkSlots } from './api';
import { BrandMark } from './components/BrandMark';
import { applyTheme, themeById } from './theme/themes';
import type { ThemePreset } from './models';
import { defaultFit } from './models';
import { saveBuildRecord } from './build/changes';
import { runningText } from './operationText';
import type { Capabilities, FieldCheck, HeroRecord, LogEntry, ModelSummary, Project, ProjectDocument, Settings, SetupStatus, TimingEvent, TimingReport } from './models';
import { ProjectOverview } from './components/ProjectOverview';
import { ExtrasPage, PhysicsPage } from './components/CustomizationPages';
import { MotionPage } from './motion/MotionPage';
import { decodePreview, type PreviewScene } from './preview/packet';
import { ToolDiagnostics } from './setup/ToolDiagnostics';
import { ClothAuthoringPanel } from './cloth/ClothAuthoringPanel';
import { SetupPage } from './setup/SetupPage';
import { SteamQrDialog } from './setup/SteamQrDialog';
import { OnboardingPage } from './setup/OnboardingPage';
import { PreparationPage } from './preparation/PreparationPage';
import { GUN_EXTENSIONS, isSourceModel, SOURCE_EXTENSIONS } from './preparation/sourceFormats';
import { WeaponsSection } from './preparation/WeaponsSection';
import { BuildPage } from './build/BuildPage';
import { buildRefused } from './build/readiness';
import type { BuildResult, ProjectInspection, OriginalValidation, PreparedModel } from './build/models';
import { Appearance } from './components/Appearance';
import type { ComponentCatalog } from './components/AssetCatalog';
import { ToolLogs } from './components/ToolLogs';
import { LogView } from './components/LogView';
import { Disclosure, ErrorNotice, NavIcon, PageTitle } from './components/Primitives';
import { AboutBuild, SettingsPage } from './components/SettingsPage';
import { StorageSection } from './components/StorageSection';
import { CharacterPreview } from './components/CharacterPreview';
import { textureStore } from './preview/textures';
import { viewChoices } from './preview/viewChoices';
import { Timings } from './components/Timings';
import { addEvent, Badge, builtMessage, eventItems, shownDetails, shownTitle, MoveToCsdk13, Notifications, readIgnored, usesCsdk12, writeIgnored, type AppEvent, type ErrorRecord, type NewEvent, type NotificationItem, type StateNotification } from './components/Notifications';
import { revealSection } from './components/workflowNavigation';
import { HistoryButtons, HistoryContext, useProjectHistory, useUndoKeys } from './history/useProjectHistory';
import { copyMessages, HeroCopyNotice, type CopyNotice } from './components/HeroCopyNotice';
import { getLanguage, m, setLanguage, t, tp, useLanguage, windowsLanguage, type Language } from './i18n';
import { shownHero, useGameNames, useGameNamesSource } from './i18n/gameNames';
import { decideLanguage, isFirstStart, launchFacts, markOfferSeen, offerSeen } from './i18n/startup';
import { LanguageOffer } from './i18n/LanguagePicker';

/** How often a project with unsaved changes is autosaved. */
export const AUTOSAVE_MS = 3 * 60 * 1000;
/** The saved project file is current again: its autosaves are not needed. */
const forgetAutosaves = (id: string) => { void Promise.resolve().then(() => api.clearAutosaves(id)).catch(() => undefined); };
const GUIDE_SEEN = 'dmb.setupGuideSeen', GUIDE_SHOWN = 'dmb.setupGuideShownThisSession', GUIDE_DONE = 'dmb.setupGuideDone', UI_MODE = 'dmb.uiMode';
/** Pages only Expert mode lists in the sidebar. */
// Extras (icons, sounds, colors) stays in both modes; Motion and timings are Expert only.
const EXPERT_PAGES: Page[] = ['motion', 'timings'];
/** Paths a build cannot do without; a problem in one of them opens the setup guide. */
const REQUIRED_FIELDS = ['workspace', 'deadlock', 'pak', 'source2viewer', 'resourcecompiler', 'csdk', 'cswin'];
/** The title, details and help of a failed operation (shown at the top and kept by its notification). */
function errorRecord(title: string, cause: unknown, ownDetails?: string): ErrorRecord {
  const backend=typeof cause==='object'&&cause!==null&&'message' in cause&&typeof cause.message==='string';
  const message=backend?(cause as {message:string}).message:title;
  const record: ErrorRecord = {title:message, details:errorText(cause), help:errorHelp(cause)};
  // Without a message of the backend's the title is the app's own label (and `ownDetails` its own
  // sentence): those are drawn translated; the backend's words are drawn as they came.
  if (!backend || ownDetails) record.own = {...(!backend ? {title:{text:title}} : {}), ...(ownDetails ? {details:{text:ownDetails}} : {})};
  return record;
}
const errorKind = (cause: unknown) => typeof cause === 'object' && cause !== null && 'kind' in cause ? cause.kind : undefined;
const isCancelled = (cause: unknown) => errorKind(cause) === 'cancelled';
/** The backend's "Another operation is running" guard: shown, never logged. */
const isBusy = (cause: unknown) => errorKind(cause) === 'busy';
/** Notification titles for the failures of the main operations (by the title `run` gives `fail`);
 * other failures keep that title. */
const FAILURE_TITLES: Record<string, string> = {
  'Build did not complete': m('Build failed'), 'Installation did not complete': m('Installation failed'),
  'Project analysis did not complete': m('Analysis failed'), 'Test my setup': m('Setup test failed'),
  'Your model could not be prepared': m('Model preparation failed'), 'Your model could not be prepared automatically': m('Model preparation failed'),
  'Model conversion did not complete': m('Model preparation failed'), 'Prepared model could not be accepted': m('Model preparation failed'),
  'Your object file could not be prepared': m('Object preparation failed'),
};
/** How long a running operation has left, from earlier runs of the same operation. */
export type TimeLeft = {kind: 'longer'} | {kind: 'almost'} | {kind: 'seconds'; count: number} | {kind: 'minutes'; count: number};
export function timeLeftParts(typical: number | undefined, startedUtc: string | undefined, now: number): TimeLeft | null {
  if (!typical || !startedUtc) return null;
  const elapsed = (now - Date.parse(startedUtc)) / 1000;
  if (!Number.isFinite(elapsed)) return null;
  const left = typical - elapsed;
  if (left < -Math.max(30, typical * 0.25)) return {kind: 'longer'};
  if (left <= 15) return {kind: 'almost'};
  return left < 90 ? {kind: 'seconds', count: Math.round(left / 10) * 10} : {kind: 'minutes', count: Math.round(left / 60)};
}
/** "about 2 minutes left" from earlier runs of the same operation (English). */
export function timeLeft(typical: number | undefined, startedUtc: string | undefined, now: number): string | null {
  const parts = timeLeftParts(typical, startedUtc, now);
  if (!parts) return null;
  return parts.kind === 'longer' ? 'taking longer than usual' : parts.kind === 'almost' ? 'almost done' : `about ${parts.count} ${parts.kind} left`;
}
/** The status bar's words for `timeLeftParts`, in the interface language. */
function timeLeftText(parts: TimeLeft): string {
  if (parts.kind === 'longer') return t('taking longer than usual');
  if (parts.kind === 'almost') return t('almost done');
  return parts.kind === 'seconds'
    ? tp(parts.count, {one: 'about {count} second left', other: 'about {count} seconds left'})
    : tp(parts.count, {one: 'about {count} minute left', other: 'about {count} minutes left'});
}
/** How far a running step is, from the typical time of the same step: 0–0.97 while it runs
 * (it never shows full before it ends), or null when there is no earlier run to go by. */
export function progressFraction(typical: number | undefined, startedUtc: string | undefined, now: number): number | null {
  if (!typical || !startedUtc) return null;
  const elapsed = (now - Date.parse(startedUtc)) / 1000;
  if (!Number.isFinite(elapsed)) return null;
  return Math.min(0.97, Math.max(0, elapsed / typical));
}
type Page = 'project' | 'appearance' | 'physics' | 'motion' | 'extras' | 'timings' | 'logs' | 'settings' | 'build' | 'prepare' | 'setup' | 'onboarding';
/** The five steps of making a mod, in order; then optional extras and tools. */
const navigation: {id: Page; label: string; icon: 'project' | 'model' | 'time' | 'log' | 'settings'; step?: number}[] = [
  {id:'onboarding',label:m('New-user setup'),icon:'settings'},
  {id:'project',label:m('Character & model'),icon:'project',step:1},
  {id:'prepare',label:m('Prepare model'),icon:'model',step:2},
  {id:'appearance',label:m('Appearance & weapons'),icon:'model',step:3},
  {id:'physics',label:m('Physics & preview'),icon:'model',step:4},
  {id:'build',label:m('Build & install'),icon:'project',step:5},
  {id:'motion',label:m('Motion'),icon:'model'}, {id:'extras',label:m('Extras'),icon:'project'},
  {id:'timings',label:m('Step timings'),icon:'time'}, {id:'logs',label:m('Logs'),icon:'log'},
  {id:'settings',label:m('Settings'),icon:'settings'},
];
/** Ctrl+1 to Ctrl+5 open the five steps. */
const STEP_PAGES: Page[] = ['project', 'prepare', 'appearance', 'physics', 'build'];
const NAV_GROUPS: {label: string; pages: Page[]}[] = [
  // The guide stays here until Done on its last step, so a new user who follows
  // one of its buttons into a step can come back to it.
  {label:m('Getting started'),pages:['onboarding']},
  {label:m('Make your mod'),pages:['project','prepare','appearance','physics','build']},
  {label:m('More customization'),pages:['motion','extras']},
  {label:m('Tools'),pages:['timings','logs','settings']},
];
/** Steps that are done, shown with a check mark in the sidebar. */
function stepsDone(project: Project | undefined, built: boolean): Set<Page> {
  const done = new Set<Page>();
  if (!project) return done;
  if (project.original_model && (project.dmx || project.preparation_source)) done.add('project');
  if (project.dmx) done.add('prepare');
  if (project.dmx && Object.keys(project.materials).length) done.add('appearance');
  if (project.customization.secondary_motion?.enabled || project.customization.physics?.enabled) done.add('physics');
  if (built) done.add('build');
  return done;
}
export function App() {
  const [page, setPage] = useState<Page>('project');
  const [document, setDocument] = useState<ProjectDocument | null>(null);
  const documentRef=useRef(document);documentRef.current=document;
  const [settings, setSettings] = useState<Settings | null>(null);
  const [projectDirty, setProjectDirty] = useState(false);
  const [savedSettings, setSavedSettings] = useState<Settings | null>(null);
  const savedRef=useRef(savedSettings);savedRef.current=savedSettings;
  const settingsDirty = settings !== null && (savedSettings === null || Object.keys(settings).some(key => settings[key as keyof Settings] !== savedSettings[key as keyof Settings]));
  const [setupStatus,setSetupStatus]=useState<SetupStatus|null>(null);
  /** What this build includes (null until the backend answers; every choice is shown meanwhile). */
  const [capabilities,setCapabilities]=useState<Capabilities|null>(null);
  const [catalog,setCatalog]=useState<ComponentCatalog|null>(null);
  useEffect(()=>setCatalog(null),[document?.project.id,document?.project.original_model,settings?.pak]);
  // The preview's textures are kept across page changes; free them when the project closes or another opens.
  // The preview's remembered view choices (look, Still or Movement, the last motion packet) go with them.
  useEffect(()=>()=>{textureStore.clear();viewChoices.clear();},[document?.project.id]);
  // `built`: the project a successful build checked after the analysis (that build ran its own analysis).
  // `refused`: a build since refused because something still needs a choice (cleared by the next Analyze).
  const [inspection,setInspection]=useState<{report:ProjectInspection;project:string;built?:string;refused?:boolean}|null>(null);
  // Whether the last Analyze saw the project as it is now (serialized once per change, not per render).
  const fresh=useMemo(()=>Boolean(document&&inspection?.project===JSON.stringify(document.project)),[document,inspection]);
  // Build & install keeps the last analysis as a working list after project edits (readiness keptAnalysis).
  const analyzedProject=useMemo(()=>inspection?JSON.parse(inspection.project) as Project:null,[inspection]);
  const previousAnalysis=useMemo(()=>analyzedProject&&document&&!fresh?{analyzed:analyzedProject,verified:Boolean(inspection?.built)&&inspection?.built===JSON.stringify(document.project),refused:Boolean(inspection?.refused)}:null,[analyzedProject,document,inspection,fresh]);
  const [buildResult,setBuildResult]=useState<BuildResult|null>(null);
  const [installed,setInstalled]=useState<string|null>(null);
  const [prepared,setPrepared]=useState<PreparedModel|null>(null);
  const [originalValidation,setOriginalValidation]=useState<OriginalValidation|null>(null);
  const [preview, setPreview] = useState<PreviewScene|null>(null);
  /** The prepared model (`project.dmx`) when the preview was loaded: a later one makes it stale for Load 3D previews automatically. */
  const [previewModel, setPreviewModel] = useState('');
  const [summary, setSummary] = useState<ModelSummary | null>(null);
  const [report, setReport] = useState<TimingReport | null>(null);
  const [backendRunning,setBackendRunning]=useState(false);
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [cancellable, setCancellable] = useState(false);
  // Undo and Redo of the project's choices (one history; fences start it fresh after operations).
  const writeProject = (project: Project) => { const current = documentRef.current; if (!current) return false; const next = {...current, project}; documentRef.current = next; setDocument(next); setProjectDirty(true); return true; };
  const projectHistory = useProjectHistory({read: () => documentRef.current?.project, write: writeProject, notify: message => setToast(message), blocked: Boolean(busy) || backendRunning});
  const history = projectHistory.controls;
  useUndoKeys(history);
  /** The failure at the top of the page: one line, or the whole error (`full`) when asked for. */
  const [error, setError] = useState<{record: ErrorRecord; logged: boolean; full: boolean} | null>(null);
  // Notifications: this session's events (newest first) and the failure to open in the list.
  const [events, setEvents] = useState<AppEvent[]>([]);
  const eventId = useRef(0);
  const [focusEvent, setFocusEvent] = useState<string | null>(null);
  const pushEvent = useCallback((event: NewEvent) => { const id = `event-${++eventId.current}`; setEvents(current => addEvent(current, {...event, id, at: Date.now()})); return id; }, []);
  const [toast, setToast] = useState('');
  const [guideStep, setGuideStep] = useState<number|undefined>(undefined);
  // Done on the guide's last step takes it out of the sidebar; Settings brings it back. Kept on this PC.
  const [guideDone, setGuideDoneState] = useState(() => { try { return localStorage.getItem(GUIDE_DONE) === '1'; } catch { return false; } });
  const setGuideDone = (value: boolean) => { setGuideDoneState(value); try { localStorage.setItem(GUIDE_DONE, value ? '1' : '0'); } catch { /* storage unavailable */ } };
  // The VPK slot scan, refreshed at every Analyze and from Build & install.
  const [slots, setSlots] = useState<VpkSlots|null>(null);
  /** Only the latest slot scan may set `slots` (scans run in parallel and can finish out of order). */
  const slotScan = useRef(0);
  // The look (Settings → Appearance): colors by data-theme on <html>, the sidebar mark, the window icon.
  const theme = settings?.theme || 'original';
  const customThemes = settings?.custom_themes ?? [];
  useEffect(() => { applyTheme(theme, customThemes); }, [theme, customThemes]);
  async function chooseTheme(next: string) {
    if (!settings || next === theme) return;
    try {
      if (desktop) await api.setTheme(next, themeById(next, customThemes).icon);
      setSettings(current => current ? {...current, theme: next} : current);
      setSavedSettings(current => current ? {...current, theme: next} : current);
    } catch (cause) { fail(m('The theme could not be saved'), cause); }
  }
  async function saveCustomThemes(presets: ThemePreset[]) {
    if (!settings) return;
    try {
      if (desktop) await api.saveThemePresets(presets);
      // The same array in both, so the tool-path "unsaved changes" check stays quiet.
      setSettings(current => current ? {...current, custom_themes: presets} : current);
      setSavedSettings(current => current ? {...current, custom_themes: presets} : current);
    } catch (cause) { fail(m('The theme could not be saved'), cause); }
  }
  // Simple (the default) hides expert pages and tools; Expert shows everything. Kept on this PC.
  const [expert, setExpertState] = useState(() => { try { return localStorage.getItem(UI_MODE) === 'expert'; } catch { return false; } });
  const setExpert = (value: boolean) => { setExpertState(value); try { localStorage.setItem(UI_MODE, value ? 'expert' : 'simple'); } catch { /* storage unavailable */ } };
  const [durations, setDurations] = useState<Record<string,number>>({});
  const [now, setNow] = useState(() => Date.now());
  /** The file name of a DMX dragged over the window (its sentence is made where it is drawn). */
  const [drag, setDrag] = useState<string | null>(null);
  const active = useRef(false);
  const nativeActive=useRef(false);nativeActive.current=backendRunning;
  const alive = useRef(true);
  const desktop = isDesktop();
  /** A failure: one line at the top and (when logged) an error notification with its own copy. */
  /** A failure was shown since the current run began (the status bar's result when the run started no backend operation). */
  const failedDuringRun = useRef(false);
  /** `own`: the cause is a sentence of the app's own (drawn translated). */
  const showFailure = useCallback((title: string, cause: unknown, log: boolean, own = false) => {
    failedDuringRun.current = true;
    const record = errorRecord(title, cause, own && typeof cause === 'string' ? cause : undefined);
    setError({record, logged: log, full: false});
    if (log) setFocusEvent(pushEvent({kind: 'error', title: FAILURE_TITLES[title] ?? title, error: record}));
  }, [pushEvent]);
  const fail = useCallback((title: string, cause: unknown) => showFailure(title, cause, !isBusy(cause) && !isCancelled(cause)), [showFailure]);
  const notify = useCallback((message: string) => setToast(message), []);
  /** `keepError`: an automatic preview load keeps the failure line of the operation before it. */
  const run = useCallback(async <T,>(label: string, operation: () => Promise<T>, canCancel = false, refreshTimings=true, keepError=false): Promise<T | undefined> => {
    if (active.current) return undefined;
    active.current = true;
    setBusy(label); setCancellable(canCancel); if (!keepError) setError(null);
    try { return await operation(); }
    catch (cause) {
      if (isCancelled(cause)) notify(t('Operation cancelled'));
      else fail(label, cause);
    } finally { try{if(refreshTimings){const latest=await api.latestTimings();if(alive.current){setBackendRunning(latest?.status==='running');if(latest)setReport(latest);}}}catch{/* Preserve the original operation error if its receipt cannot be read. */} active.current = false; if (alive.current) { setBusy(null); setCancellable(false); } }
    return undefined;
  }, [fail, notify]);
  // Live checks of the draft settings (files only; cheap). The one caller: Settings,
  // the guide and the Notifications use these.
  const [fieldChecks, setFieldChecks] = useState<FieldCheck[] | null>(null);
  useEffect(() => {
    if (!settings || !desktop) return;
    let current = true;
    const timer = window.setTimeout(() => { void Promise.resolve().then(() => api.checkSettingFields(settings)).then(result => { if (current) setFieldChecks(result ?? null); }).catch(() => { if (current) setFieldChecks(null); }); }, 300);
    return () => { current = false; window.clearTimeout(timer); };
  }, [settings, desktop]);
  // Notifications: states cleared for this session
  // (until their condition goes away), and states ignored for good (kept on this PC).
  const [clearedStates, setClearedStates] = useState<Set<StateNotification>>(() => new Set());
  const [ignoredStates, setIgnoredStates] = useState<StateNotification[]>(readIgnored);
  const setIgnored = (kinds: StateNotification[]) => { setIgnoredStates(kinds); writeIgnored(kinds); };
  const [csdkFound, setCsdkFound] = useState<string | null>(null);
  const csdk12 = usesCsdk12(fieldChecks);
  useEffect(() => { if (!csdk12) { setClearedStates(current => { if (!current.has('csdk12')) return current; const next = new Set(current); next.delete('csdk12'); return next; }); setCsdkFound(null); } }, [csdk12]);
  useEffect(() => {
    alive.current = true;
    if (!desktop) return;
    let disposed = false;
    let releaseLogs: UnlistenFn | undefined; let releaseTimings: UnlistenFn | undefined;
    let timingPending=false, timingReading=false, timingRunning=false;
    const queued: LogEntry[] = [];
    const merge = (incoming: LogEntry[]) => setLogs(current => [...new Map([...current, ...incoming].map(entry => [entry.sequence, entry])).values()].sort((a,b) => a.sequence-b.sequence).slice(-20000));
    const timer = window.setInterval(() => { if (queued.length) merge(queued.splice(0)); }, 50);
    const timingTimer=window.setInterval(()=>{if((timingPending||timingRunning)&&!timingReading){timingPending=false;timingReading=true;void api.latestTimings().then(value=>{if(!disposed&&value){timingRunning=value.status==='running';setBackendRunning(timingRunning);setReport(value);}}).catch(cause=>{if(!disposed)showFailure(m('Could not refresh step timings'),cause,false);}).finally(()=>{timingReading=false;});}},200);
    void (async () => {
      try {
        const unlisten = await listen<LogEntry>('build-log', event => queued.push(event.payload));
        if (disposed) { unlisten(); return; }
        releaseLogs = unlisten;
        const unlistenTimings=await listen<TimingEvent>('timing-event',()=>{timingPending=true;});
        if(disposed){unlistenTimings();return;}releaseTimings=unlistenTimings;
        // Fire and forget: an older or faked backend without the command leaves every choice shown.
        Promise.resolve().then(() => api.capabilities()).then(found => { if (!disposed) setCapabilities(found); }).catch(() => undefined);
        const [loadedSettings, loadedProject, retainedLogs, latestReport] = await Promise.all([api.settings(), api.newProject(), api.logs(), api.latestTimings()]);
        if (!disposed) { setSettings(loadedSettings); setSavedSettings(loadedSettings); setDocument(loadedProject); projectHistory.controls.fence(); merge(retainedLogs); setReport(latestReport);setBackendRunning(latestReport?.status==='running');timingRunning=latestReport?.status==='running';
        // After a crash, offer the newest autosave.
        const recovery = await Promise.resolve().then(() => api.sessionRecovery()).catch(() => null);
        if (!disposed && recovery?.unclean) {
          const latest = recovery.autosaves[0];
          if (latest) {
            let restore = false;
            try { restore = await ask(t('Deadlock Mod Builder did not close normally. Restore the unsaved changes to “{name}” from {time}?', {name: latest.name || t('Untitled project'), time: new Date(latest.saved_ms).toLocaleString()}), {title:t('Restore unsaved changes'), kind:'warning', okLabel:t('Restore'), cancelLabel:t('Not now')}); } catch { restore = false; }
            if (restore && !disposed) {
              try { const restored = await api.restoreAutosave(latest.id, latest.file); documentRef.current = restored; setDocument(restored); setProjectDirty(true); projectHistory.controls.fence(); notify(t('Unsaved changes restored. Save the project to keep them.')); }
              catch (cause) { fail(m('The autosave could not be restored'), cause); }
            }
          }
          void Promise.resolve().then(() => api.dismissRecovery()).catch(() => undefined);
        } }
        // The setup guide opens by itself on first start, and once per session
        // when a required tool path is wrong.
        if (!disposed) {
          let seen = true; try { seen = localStorage.getItem(GUIDE_SEEN) === '1'; if (!seen) localStorage.setItem(GUIDE_SEEN, '1'); } catch { /* storage unavailable */ }
          if (!seen) { setGuideStep(0); setPage('onboarding'); }
          else {
            const fields = await Promise.resolve().then(() => api.checkSettingFields(loadedSettings)).catch(() => null);
            let shown = false; try { shown = sessionStorage.getItem(GUIDE_SHOWN) === '1'; } catch { /* storage unavailable */ }
            if (!disposed && fields?.some(check => check.state === 'problem' && REQUIRED_FIELDS.includes(check.field)) && !shown) {
              try { sessionStorage.setItem(GUIDE_SHOWN, '1'); } catch { /* storage unavailable */ }
              setGuideStep(1); setPage('onboarding'); notify(t('Some tool paths need fixing. The setup guide shows which.'));
            }
          }
        }
      } catch (cause) { if (!disposed) fail(m('Could not initialize the workspace'), cause); }
    })();
    return () => { disposed = true; alive.current = false; releaseLogs?.();releaseTimings?.(); window.clearInterval(timer);window.clearInterval(timingTimer); };
  }, [desktop, fail, showFailure]);
  // Autosave every few minutes while the project has unsaved changes.
  const dirtyRef = useRef(projectDirty); dirtyRef.current = projectDirty;
  useEffect(() => {
    if (!desktop) return;
    const timer = window.setInterval(() => {
      const current = documentRef.current;
      if (current && dirtyRef.current) void Promise.resolve().then(() => api.autosaveProject(current)).catch(() => undefined);
    }, AUTOSAVE_MS);
    return () => window.clearInterval(timer);
  }, [desktop]);
  useEffect(() => { if (!toast) return; const timer = window.setTimeout(() => setToast(''), 4000); return () => window.clearTimeout(timer); }, [toast]);

  const discardProject = useCallback(async () => !projectDirty || await ask(t('This project has unsaved changes. Discard them and continue?'), {title:t('Unsaved project'),kind:'warning',okLabel:t('Discard changes'),cancelLabel:t('Keep editing')}), [projectDirty]);
  const saveProject = useCallback(async (saveAs = false) => {
    if (!document || active.current) return;
    await run(m('Could not save the project'), async () => {
      const path = !saveAs && document.path ? document.path : await saveFor('project',{title:t('Save project'),defaultPath:document.path ?? `${document.project.name}.dmb.json`,filters:[{name:t('Deadlock Mod Builder project'),extensions:['json']}]});
      if (!path) return;
      const saved = await api.saveProject(path, document.project);
      setDocument(saved); setProjectDirty(false); forgetAutosaves(saved.project.id); notify(t('Project saved'));
    });
  }, [document, run, notify]);
  /** Save inside an operation that is already running (no second busy state); resolves to the saved path. */
  const saveDuringOperation = useCallback(async () => {
    const current = documentRef.current;
    if (!current) return undefined;
    const path = current.path ?? await saveFor('project',{title:t('Save project'),defaultPath:`${current.project.name}.dmb.json`,filters:[{name:t('Deadlock Mod Builder project'),extensions:['json']}]});
    if (!path) return undefined;
    const saved = await api.saveProject(path, current.project);
    setDocument(saved); setProjectDirty(false); forgetAutosaves(saved.project.id);
    return saved.path ?? path;
  }, []);
  /** Open… (a file window), or a known project file such as one Installed mods names. */
  const openProject = useCallback(async (chosen?: string) => {
    if (active.current) return;
    await run(m('Could not open the project'), async () => {
      if (!await discardProject()) return;
      const path = chosen ?? await openFor('project',{title:t('Open project'),multiple:false,filters:[{name:t('Deadlock Mod Builder project'),extensions:['json']}]});
      if (typeof path !== 'string') return;
      const loaded = await api.openProject(path);
      setDocument(loaded); history.fence(); setProjectDirty(false); setSummary(null);setPreview(null);setInspection(null);setPrepared(null);setOriginalValidation(null);setBuildResult(null);setInstalled(null); setPage('project'); notify(t('Project opened'));
    });
  }, [run, discardProject, notify]);
  const newProject = useCallback(async () => {
    if (active.current) return;
    await run(m('Could not create a project'), async () => {
      if (!await discardProject()) return;
      setDocument(await api.newProject()); history.fence(); setProjectDirty(false); setSummary(null);setPreview(null);setInspection(null);setPrepared(null);setOriginalValidation(null);setBuildResult(null);setInstalled(null); setPage('project');
    });
  }, [run, discardProject]);
  const analyze = useCallback(async (path: string) => {
    if (!document || active.current) return;
    await run(m('Model analysis did not complete'), async () => {
      try {
        const result = await api.analyze(path);
        void scanSlots();
        setSummary(result.summary);setPreview(null);setInspection(null); setReport(result.timings);
        setDocument(current => current ? {...current,project:{...current.project,dmx:result.summary.path}} : current); history.fence();
        if (document.project.dmx !== result.summary.path) setProjectDirty(true);
        setPage('appearance'); notify(t('Model analysis complete'));
      } finally { try { const latest = await api.latestTimings(); if (latest) setReport(latest); } catch (cause) { fail(m('Could not retrieve the timing report'), cause); } }
    }, true);
  }, [document, run, notify, fail]);
  /** Automatic preparation of a Blender, FBX, glTF or MMD model. A gun in a separate file is asked
   * for first: the fields are locked while preparation runs. */
  const startPreparation = useCallback(async (path: string) => {
    const current = documentRef.current;
    if (current && !current.project.preparation_options.weapon_source) {
      let wants = false;
      try { wants = await ask(t('Is your gun in a separate file? Choose it now and it is prepared together with your character. You can also add it later on the Gun tab.'), {title:t('Gun file'), kind:'info', okLabel:t('Choose gun file…'), cancelLabel:t('No, prepare now')}); } catch { wants = false; }
      if (wants) {
        try {
          const gun = await openFor('gun-file',{title:t('Choose the gun file'), multiple:false, filters:[{name:t('Gun file'), extensions:[...GUN_EXTENSIONS]}]});
          if (typeof gun === 'string') {
            const checked = await api.validateAssetPath(gun, 'weapon');
            const latest = documentRef.current;
            if (latest) {
              const next = {...latest, project:{...latest.project, preparation_options:JSON.parse(JSON.stringify({...latest.project.preparation_options, weapon_source:checked, weapon:undefined, reviewed:false}))}};
              documentRef.current = next; setDocument(next); setProjectDirty(true); projectHistory.record(latest.project, next.project);
            }
          }
        } catch (cause) { fail(m('The gun file could not be used'), cause); }
      }
    }
    setPage('prepare');
    window.dispatchEvent(new CustomEvent('dmb-model-source', {detail:{path, auto:true}}));
  }, [fail]);
  const chooseModel = useCallback(async () => {
    if (active.current || !document) return;
    try {
      const path = await openFor('model-source',{title:t('Select replacement model'),multiple:false,filters:[{name:t('Models'),extensions:['dmx',...SOURCE_EXTENSIONS]}]});
      // Blender, FBX, glTF and MMD models are inspected and prepared automatically (step 2).
      if (typeof path === 'string') {if(/\.dmx$/i.test(path))await analyze(path);else await startPreparation(path);}
    } catch (cause) { fail(m('Could not choose a model'), cause); }
  }, [analyze, document, fail, startPreparation]);
  /** Prepare the current model again from the start (for example with a new gun file). */
  const prepareAgain = useCallback(() => {
    setPage('prepare');
    const path = documentRef.current?.project.preparation_source;
    if (path) window.dispatchEvent(new CustomEvent('dmb-model-source', {detail:{path,auto:true}}));
  }, []);
  const startPreparationRef = useRef(startPreparation);
  useEffect(() => { startPreparationRef.current = startPreparation; }, [startPreparation]);
  const analyzeRef = useRef(analyze);
  useEffect(() => { analyzeRef.current = analyze; }, [analyze]);
  useEffect(() => {
    if (!desktop) return;
    let disposed = false; let release: UnlistenFn | undefined;
    void getCurrentWindow().onDragDropEvent(event => {
      const payload = event.payload;
      const target='position' in payload?window.document.elementFromPoint(payload.position.x/window.devicePixelRatio,payload.position.y/window.devicePixelRatio)?.closest('[data-native-drop]'):null;
      if (payload.type === 'enter') {
        // Enter usually happens outside the intended drop zone. Navigating here
        // unmounts that zone before the user can reach it (including Extras).
        // Keep the current page; an accepted model drop handles navigation.
        setDrag(!target && payload.paths.length === 1 && /\.dmx$/i.test(payload.paths[0]) ? basename(payload.paths[0]) : null);
      } else if (payload.type === 'leave') setDrag(null);
      else if (payload.type === 'drop') {
        setDrag(null);
        if(target){target.dispatchEvent(new CustomEvent('dmb-file-drop',{detail:payload.paths}));return;}
        if(payload.paths.length===1&&isSourceModel(payload.paths[0])&&!active.current){void startPreparationRef.current(payload.paths[0]);}
        else if (payload.paths.length !== 1 || !/\.dmx$/i.test(payload.paths[0])) showFailure(m('Could not import the dropped file'), m('Drop one local file with the .dmx extension, or a Blender, GLB, glTF, FBX or MMD (.pmx, .pmd) source.'), true, true);
        else if (active.current) showFailure(m('An operation is already running'), m('Wait for the current operation to finish or cancel it, then drop the model again.'), false, true);
        else void analyzeRef.current(payload.paths[0]);
      }
    }).then(unlisten => { if (disposed) unlisten(); else release = unlisten; }).catch(cause => fail(m('Drag and drop could not be enabled'), cause));
    return () => { disposed = true; release?.(); };
  }, [desktop, fail, showFailure]);
  useEffect(() => {
    if (!desktop) return;
    let disposed = false; let release: UnlistenFn | undefined;
    void getCurrentWindow().onCloseRequested(async event => {
      if (active.current||nativeActive.current) { event.preventDefault(); notify(t('Finish or cancel the current operation before closing.')); return; }
      if (projectDirty || settingsDirty) {
        event.preventDefault();
        if (await ask(t('There are unsaved changes. Close without saving?'),{title:t('Unsaved changes'),kind:'warning',okLabel:t('Close without saving'),cancelLabel:t('Keep editing')})) await getCurrentWindow().destroy();
      }
    }).then(unlisten => { if (disposed) unlisten(); else release = unlisten; }).catch(cause => fail(m('Could not register close protection'), cause));
    return () => { disposed = true; release?.(); };
  }, [desktop, projectDirty, settingsDirty, notify, fail]);
  useEffect(() => {
    const keydown = (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey) || !desktop) return;
      if (event.key.toLowerCase() === 's') { event.preventDefault(); void saveProject(event.shiftKey); }
      if (event.key.toLowerCase() === 'o') { event.preventDefault(); void openProject(); }
      if (event.key.toLowerCase() === 'n') { event.preventDefault(); void newProject(); }
      if (event.key.toLowerCase() === 'b' && !event.shiftKey) { event.preventDefault(); setPage('build'); window.setTimeout(() => window.dispatchEvent(new CustomEvent('dmb-build-shortcut')), 0); }
      const step = ['1','2','3','4','5'].indexOf(event.key);
      if (step >= 0 && !event.shiftKey && !event.altKey) { event.preventDefault(); setPage(STEP_PAGES[step]); }
    };
    window.addEventListener('keydown', keydown);
    return () => window.removeEventListener('keydown', keydown);
  }, [desktop, saveProject, openProject, newProject]);

  async function discoverComponents(extrasOnly=false){if(!document)return;const requested=document.project;await run(m('Character asset discovery did not complete'),async()=>{const next=await (extrasOnly?api.discoverExtras(requested):api.discoverComponents(requested));if(documentRef.current?.project.id===requested.id&&documentRef.current.project.original_model===requested.original_model)setCatalog(next);notify(t('Character assets loaded.'));},true);}
  async function saveOutfit(){if(!document)return;await run(m('Outfit could not be saved'),async()=>{const path=await saveFor('outfit-preset',{title:t('Save outfit preset'),defaultPath:`${document.project.name}.outfit.json`,filters:[{name:t('Outfit preset'),extensions:['json']}]});if(path){await api.outfitSave(document.project,path,document.project.name);notify(t('Outfit preset saved.'));}});}
  async function loadOutfit(){if(!document)return;await run(m('Outfit could not be loaded'),async()=>{const path=await openFor('outfit-preset',{title:t('Load outfit preset'),multiple:false,filters:[{name:t('Outfit preset'),extensions:['json']}]});if(typeof path==='string'){updateProject(await api.outfitLoad(document.project,path));setSummary(null);setPreview(null);notify(t('Outfit preset loaded. Review its choices.'));}});}
  // The slot status (which Build checks) is read again for the chosen slot: choosing the slot it already had (pak09
  // deleted from the addons folder, Find free slot picks 9 again) changes nothing else that would rescan.
  async function chooseFreeSlot(){if(!document)return;await run(m('Could not choose a free VPK slot'),async()=>{const value=await api.chooseFreeSlot(document.project);const project={...document.project,vpk_number:value.slot};updateProject(project);await scanSlots(project);notify(t('Selected available slot: {file}', {file: value.filename}));});}
  async function buildProject(overwriteOutput:boolean,overwriteInstall:boolean){if(!document||active.current)return;
    // Common causes of failed builds, before a long build starts; the user may build anyway.
    let warnings:string[]=[];try{warnings=(await api.prebuildWarnings())??[];}catch{warnings=[];}
    // The warnings come from the backend as they are; only the question is the app's own.
    if(warnings.length&&!await ask(`${warnings.map(w=>`• ${w}`).join('\n\n')}\n\n${t('Build anyway?')}`,{title:t('Before building'),kind:'warning',okLabel:t('Build anyway'),cancelLabel:t('Cancel')}))return;
    const name=document.project.name;
    await run(m('Build did not complete'),async()=>{setBuildResult(null);
      // The build's own check refused what still needs a choice (readiness BUILD_REFUSALS): the kept list no longer allows Build until Analyze.
      const value=await api.buildProject(document.project,overwriteOutput,overwriteInstall,document.path).catch(cause=>{if(buildRefused(cause))setInspection(prev=>prev?{...prev,refused:true,built:undefined}:prev);throw cause;});const analyzed=JSON.stringify(document.project);
      // The build saves the project before and after building: into the user's own file when it has one, else into
      // the workspace. Only the user's own file makes the project saved; a project never saved by the user stays unsaved.
      const ownFile=document.path!==null;
      setDocument({project:value.project,path:ownFile?value.path:null});history.fence();
      if(ownFile)setProjectDirty(false);else if(analyzed!==JSON.stringify(value.project))setProjectDirty(true);
      // The build checked exactly the analyzed inputs; what it records in the project is not a change to analyze again.
      // From a kept analysis (Build & install) the build's own analysis passed: record what it checked.
      setInspection(prev=>prev&&prev.project===analyzed?{...prev,project:JSON.stringify(value.project)}:prev?{...prev,built:JSON.stringify(value.project)}:prev);if(ownFile)forgetAutosaves(value.project.id);setBuildResult(value.result);if(value.result.compiled||value.result.vpk)saveBuildRecord(value.project);notify(value.result.installed?t('Verified VPK installed.'):value.result.vpk?t('Verified VPK published.'):value.result.compiled?t('Compilation completed; no VPK was requested.'):t('Source preparation completed; no compilation was requested.'));
      pushEvent({kind:'success',operation:'build',title:m('Build completed'),project:value.project.name||name,lines:[builtMessage(value.result)],paths:value.result.vpk?[{label:m('Output'),path:value.result.vpk}]:value.result.prepared?[{label:m('Files'),path:value.result.prepared}]:[],open:{label:m('Open Build & install'),page:'build'}});},true);}
  async function installProject(selected:string|null,overwrite:boolean){if(!document)return;await run(m('Installation did not complete'),async()=>{const path=selected??await openFor('vpk',{title:t('Select VPK to validate and install'),multiple:false,filters:[{name:t('VPK archive'),extensions:['vpk']}]});if(typeof path!=='string')return;setInstalled(null);const value=await api.installProject(document.project,path,overwrite);setInstalled(value);notify(t('VPK validated and installed.'));
    pushEvent({kind:'success',operation:'install',title:m('Mod installed'),project:document.project.name,lines:[{text:m('The VPK was validated and installed as {file}.'),values:{file:basename(value)}}],paths:[{label:m('Installed'),path:value}],open:{label:m('Open Build & install'),page:'build'}});},true);}
  /** The game's addons folder and the output folder, read again (Analyze, Build & install). */
  async function scanSlots(project=documentRef.current?.project){if(!project)return;const scan=++slotScan.current;try{const value=await api.vpkSlots(project);if(scan===slotScan.current)setSlots(value);}catch{if(scan===slotScan.current)setSlots(null);}}
  async function inspectProject(){if(!document)return;await run(m('Project analysis did not complete'),async()=>{
    setInspection(null);const value=await api.inspectProject(document.project);updateProject(value.project);setInspection({report:value.report,project:JSON.stringify(value.project)});if('dmx' in value.report)setSummary({...value.report.dmx,path:value.project.dmx});await scanSlots();const review='needs_review' in value.report&&value.report.needs_review;notify(review?t('Review the analysis before building.'):t('Project analysis complete.'));
    // A report that waits for the user's choice is a warning (the same flag the Build page reads).
    pushEvent({kind:review?'warning':'success',operation:'analyze',title:m('Analysis finished'),project:value.project.name,lines:[review?m('Some items need your choice before building. Review them on Build & install.'):m('Project analysis complete.')],open:{label:m('Open Build & install'),page:'build'}});
  },true);}
  async function prepareProject(offline:boolean){if(!document)return;await run(m('Source preparation did not complete'),async()=>{const value=await api.prepareProject(document.project,offline);updateProject(value.project);setPrepared(value.prepared);notify(t('Compiler source files prepared.'));},true);}
  async function validateOriginal(compile:boolean){if(!document)return;await run(m('Original model test did not complete'),async()=>{setOriginalValidation(await api.validateOriginal(document.project,compile));notify(compile?t('Original test completed. Review the native receipt.'):t('Original compiler plan inspected.'));},true);}
  async function importSourcePackage(){
    if(!document)return;
    await run(m('Source package import did not complete'),async()=>{
      const path=await openFor('source-package',{title:t('Import source package'),multiple:false,filters:[{name:t('Verified source package'),extensions:['zip']}]});
      if(typeof path!=='string')return;
      const project=await api.importSourcePackage(path,document.project);
      updateProject(project);setSummary(null);setPreview(null);notify(t('Source package imported and verified'));
    },true);
  }
  async function analyzeMaterials(){
    if(!document)return;
    await run(m('Material analysis did not complete'),async()=>{updateProject(await api.analyzeMaterials(document.project));notify(t('Material analysis complete. Review assignments before building.'));},true);
  }
  async function detectSettings(extraRoot=false){
    if(!settings)return;
    await run(m('Tool detection did not complete'),async()=>{
      const roots:string[]=[];
      if(extraRoot){const selected=await openFor('tools-folder',{title:t('Search for installed tools'),directory:true,multiple:false});if(typeof selected!=='string')return;roots.push(selected);}
      const detected=await api.detectSettings(settings,roots);setSettings(detected);setSetupStatus(await api.setupStatus(detected));notify(t('Detection complete. Review and save settings.'));
    },true);
  }
  async function checkSetup(){if(settings)await run(m('Could not check tool paths'),async()=>{setSetupStatus(null);setSetupStatus(await api.setupStatus(settings));});}
  async function verifySetup(){if(settings)await run(m('Could not save and check tools'),async()=>{setSetupStatus(null);await api.saveSettings(settings);setSavedSettings(settings);setInspection(null);setSetupStatus(await api.setupStatus(settings));notify(t('Settings saved and tool locations checked.'));});}
  /** Test my setup with the paths on screen (saved or not). */
  async function testSetup(){
    if(!settings)return undefined;
    const value=await run(m('Test my setup'),()=>api.testSetup(settings));
    if(value?.passed)pushEvent({kind:'success',operation:'setup-test',title:m('Setup test passed'),lines:[m('Every check of Test my setup passed.')],open:{label:m('Open Settings'),page:'settings'}});
    else if(value){
      // The panel shows each failed step in place; the notification names them and keeps their details.
      const failed=value.steps.filter(step=>!step.passed);
      const names=failed.map(step=>step.name).join(', ');
      const own={title:names?{text:m('Failed: {steps}.'),values:{steps:names}}:{text:m('Failed: a check.')}};
      const record:ErrorRecord={own,title:`Failed: ${names||'a check'}.`,details:failed.map(step=>[`${step.name}: ${step.detail}`,step.error?errorText(step.error):''].filter(Boolean).join('\n')).join('\n\n'),help:failed.map(step=>errorHelp(step.error)).find(Boolean)??null};
      setFocusEvent(pushEvent({kind:'error',title:m('Setup test failed'),error:record}));
    }
    return value;
  }
  /** Loads the 3D preview (the prepared model, or another mesh to inspect). Resolves to whether it
   * started (false while another operation runs). `quiet`: loaded by Load 3D previews
   * automatically, so no "Preview loaded" toast. */
  async function loadPreview(path=document?.project.dmx,stay=false,quiet=false){if(!path)return false;let started=false,loaded=false;const model=document?.project.dmx??'';await run(m('Preview could not be loaded'),async()=>{started=true;const buffer=await api.preview(path);setPreview(decodePreview(buffer));setPreviewModel(model);loaded=true;if(!stay)setPage('appearance');if(!quiet)notify(t('Preview loaded'));},true,true,quiet);if(started&&!loaded&&quiet)autoFailed.current.add(`${document?.project.id}:${path}`);return started;}
  // Load 3D previews automatically (Settings → Appearance): the pages load their view by themselves.
  // A prepared model whose automatic load failed is not loaded by itself again this session, on
  // either page (Appearance & weapons and Physics & preview share the one preview).
  const autoFailed=useRef(new Set<string>());
  const autoPreviews=Boolean(savedSettings?.auto_previews);
  const autoLoadPreview=autoPreviews?async()=>autoFailed.current.has(`${document?.project.id}:${document?.project.dmx}`)?true:loadPreview(undefined,true,true):undefined;
  const previewCurrent=!preview||previewModel===(document?.project.dmx??'');
  /** Saves one Appearance preference at once (not with the tool paths), as the theme is. Resolves to whether it was saved. */
  async function savePreference(values:Pick<Settings,'default_fit_mode'|'auto_previews'|'language'>){
    if(!savedRef.current||!settings)return false;
    // The saved settings plus this choice (never the unsaved tool paths); the ref keeps two quick changes.
    // Shown at once; set back when it could not be saved.
    const before={default_fit_mode:savedRef.current.default_fit_mode,auto_previews:savedRef.current.auto_previews,language:savedRef.current.language};
    const back=Object.fromEntries(Object.keys(values).map(key=>[key,before[key as keyof typeof before]]));
    const next={...savedRef.current,...values};savedRef.current=next;
    setSettings(current=>current?{...current,...values}:current);
    setSavedSettings(current=>current?{...current,...values}:current);
    try{if(desktop)await api.saveSettings(next);return true;}
    catch(cause){
      savedRef.current={...savedRef.current,...back};
      setSettings(current=>current?{...current,...back}:current);
      setSavedSettings(current=>current?{...current,...back}:current);
      fail(m('The setting could not be saved'),cause);
      return false;
    }
  }
  // The interface language (Settings → Appearance → Language, the guide's first card, the offer):
  // applied at once and saved like the other Appearance preferences; set back when it could not be saved.
  useLanguage();
  // The game's own hero and item names in the interface language (shown only; asked when the
  // language is not English, again when it or the saved game archive changes).
  useGameNamesSource(savedSettings!==null,savedSettings?.pak??'');
  useGameNames();
  const [languageOffer,setLanguageOffer]=useState(false);
  async function chooseLanguage(next:Language){
    const before=getLanguage();
    setLanguageOffer(false);
    await setLanguage(next);
    if(!await savePreference({language:next}))await setLanguage(before);
  }
  // Once the settings are read: the saved language wins; without one, a first start follows
  // Windows (and saves it), and an existing user on Chinese Windows is offered Chinese once.
  const [launch]=useState(launchFacts);
  const languageSettled=useRef(false);
  useEffect(()=>{
    if(!savedSettings||languageSettled.current)return;
    languageSettled.current=true;
    const decision=decideLanguage(savedSettings.language,isFirstStart(launch,savedSettings),windowsLanguage());
    if(decision.language!==getLanguage())void setLanguage(decision.language);
    if(decision.persist)void savePreference({language:decision.language});
    if(decision.offer&&!offerSeen()){markOfferSeen();setLanguageOffer(true);}
  },[savedSettings]);
  /** A change of the project. A hand edit is a step of the undo history; a change an operation
   * makes (while `run` is active: preparing, Analyze, a loaded outfit, the hero …) is a fence. */
  function updateProject(project:Project){const current=documentRef.current;if(!current)return;if(current.project!==document?.project){notify(t('Project changed while the selection was being checked. Please select it again.'));return;}const next={...current,project};documentRef.current=next;setDocument(next);setProjectDirty(true);if(active.current)history.fence();else projectHistory.record(current.project,project);}
  // Make a copy for another hero: what it carried (shown above Prepare model), and the source
  // to check once the copy's Prepare model page has mounted (it is keyed by the project id).
  // `model` is the hero the copy was made for: the notice shows only while the project still has that hero.
  const [copyNotice,setCopyNotice]=useState<(CopyNotice&{id:string;model:string})|null>(null);
  const [pendingSource,setPendingSource]=useState<{id:string;path:string}|null>(null);
  useEffect(()=>{if(!pendingSource||document?.project.id!==pendingSource.id)return;setPendingSource(null);window.dispatchEvent(new CustomEvent('dmb-model-source',{detail:{path:pendingSource.path,auto:true}}));},[pendingSource,document?.project.id]);
  useEffect(()=>{if(copyNotice&&document?.project.id!==copyNotice.id)setCopyNotice(null);},[copyNotice,document?.project.id]);
  /** A new, unsaved project for `hero` with this character's choices; this project is not changed. */
  async function copyForHero(hero:HeroRecord){if(!document||active.current)return;const original=document.project;await run(m('The copy for another hero could not be made'),async()=>{
    if(projectDirty&&!await ask(t('This project has unsaved changes. The copy includes them, but the project file keeps only what was last saved. Make the copy anyway?'),{title:t('Unsaved project'),kind:'warning',okLabel:t('Make the copy'),cancelLabel:t('Keep editing')}))return;
    const value=await api.copyProjectForHero(original,hero.path);
    let project=value.project;
    // A free VPK slot for the copy (the original keeps its own); Build & install checks it again.
    try{const slot=await api.chooseFreeSlot(project);if(slot?.slot)project={...project,vpk_number:slot.slot};}catch{/* the default slot stays */}
    const next={path:null,project};documentRef.current=next;setDocument(next);history.fence();setProjectDirty(true);
    setSummary(null);setPreview(null);setInspection(null);setPrepared(null);setOriginalValidation(null);setBuildResult(null);setInstalled(null);setSlots(null);
    const notice:CopyNotice={...value.report,to:project.hero_name};setCopyNotice({...notice,id:project.id,model:project.original_model});
    pushEvent({kind:'success',operation:'hero-copy',title:m('Copy made for {hero}'),titleValues:{hero:project.hero_name},project:project.name,lines:copyMessages(notice),open:{label:m('Open Prepare model'),page:'prepare'}});
    setPage('prepare');
    if(project.preparation_source)setPendingSource({id:project.id,path:project.preparation_source});
    notify(t('Copy made for {hero}. Save it as a new project.', {hero: shownHero(project.hero_name)}));
  });}
  async function selectHero(hero:HeroRecord){if(!document)return;await run(m('Could not select the character'),async()=>{const project=await api.selectHero(document.project,hero.path);updateProject(project);setSummary(null);setPreview(null);notify(t('Character selected'));});}
  async function openReport() {
    await run(m('Could not open the timing report'), async () => {
      const path = await openFor('timing-report',{title:t('Open step timing report'),multiple:false,filters:[{name:t('Timing report'),extensions:['json']}]});
      if (typeof path === 'string') { setReport(await api.timings(path)); notify(t('Timing report opened')); }
    },false,false);
  }
  async function copyLog() { await run(m('Could not copy the log'), async () => { await navigator.clipboard.writeText(await api.logText()); notify(t('Session log copied')); }); }
  async function saveLog() { await run(m('Could not save the log'), async () => { const path = await saveFor('log',{title:t('Save complete log'),defaultPath:'deadlock-build.log'}); if (path) { await api.saveLog(path); notify(t('Full log saved')); } }); }
  async function persistSettings() { if (settings) await run(m('Could not save settings'), async () => { await api.saveSettings(settings); setSavedSettings(settings); setInspection(null); notify(t('Settings saved')); }); }
  async function cancel() { try { await api.cancel(); notify(t('Cancellation requested')); } catch (cause) { fail(m('Could not cancel the operation'), cause); } }
  const backendBusy=backendRunning;
  const running=Boolean(busy)||backendBusy;
  useEffect(()=>{if(!running||!desktop)return;let current=true;Promise.resolve().then(()=>api.operationDurations()).then(value=>{if(current&&value)setDurations(value);}).catch(()=>undefined);const timer=window.setInterval(()=>setNow(Date.now()),1000);return()=>{current=false;window.clearInterval(timer);};},[running,desktop]);
  const remaining=running&&report?.status==='running'?timeLeftParts(durations[report.operation],report.started_utc,now):null;
  const progress=running&&report?.status==='running'?progressFraction(durations[report.operation],report.started_utc,now):null;
  // A finished operation fills the bar for a moment: green when it worked.
  const [finished,setFinished]=useState<'succeeded'|'failed'|null>(null);
  const wasRunning=useRef(false);
  // The report that was current when the run began, unless that one was still running (then it is
  // this run's own): a run that starts no backend operation must not show an earlier one's result.
  const reportAtStart=useRef<string|null>(null);
  const reportKey=(value:TimingReport|null)=>value?`${value.operation}\u0000${value.run_id}\u0000${value.started_utc}`:null;
  useEffect(()=>{
    if(running){if(!wasRunning.current){wasRunning.current=true;failedDuringRun.current=false;reportAtStart.current=report?.status==='running'?null:reportKey(report);}setFinished(null);return;}
    if(!wasRunning.current)return;
    wasRunning.current=false;
    const own=report!==null&&reportKey(report)!==reportAtStart.current;
    setFinished(own?(report?.status==='succeeded'?'succeeded':report?.status==='failed'?'failed':null):failedDuringRun.current?'failed':'succeeded');
    const timer=window.setTimeout(()=>setFinished(null),2500);
    return()=>window.clearTimeout(timer);
  },[running,report?.status]);
  const runningStep=backendBusy?[...(report?.steps??[])].reverse().find(step=>step.status==='running')?.name:undefined;
  function navigate(page:Page, section?:string){setPage(page);if(section)requestAnimationFrame(()=>revealSection(section));}
  const unavailable = Boolean(busy) || backendBusy || !desktop;
  const done = stepsDone(document?.project, Boolean(buildResult?.vpk || installed));
  const goPage = (target: string) => setPage(target as Page);
  const clearState = (kind: StateNotification) => setClearedStates(current => new Set(current).add(kind));
  const stateItems: NotificationItem[] = settings && csdk12 && !clearedStates.has('csdk12') && !ignoredStates.includes('csdk12') ? [{id:'csdk12', kind:'warning', title:t('Reduced CSDK 12 in use'), dismiss:()=>clearState('csdk12'),
    body:<MoveToCsdk13 settings={settings} busy={unavailable} update={value=>{setSettings(value);setSetupStatus(null);}} go={goPage} notify={notify} fail={fail} found={csdkFound} setFound={setCsdkFound} ignore={()=>setIgnored([...ignoredStates.filter(kind=>kind!=='csdk12'),'csdk12'])}/>}] : [];
  const showDetails = (event: AppEvent) => { if (!event.error) return; setError({record: event.error, logged: true, full: true}); window.document.querySelector('.workspace')?.scrollTo?.({top: 0}); };
  const notifications = [...stateItems, ...eventItems(events, {go: goPage, showDetails, dismiss: id => setEvents(current => current.filter(event => event.id !== id))})];
  const clearNotifications = () => { setEvents([]); setClearedStates(current => { const next = new Set(current); for (const item of stateItems) next.add(item.id as StateNotification); return next; }); };
  return <HistoryContext.Provider value={history}><div className="app-shell">
    <aside className="sidebar"><div className="brand"><span className="brand-mark" aria-hidden="true"><BrandMark theme={theme}/></span><div>Deadlock<span>Mod Builder</span></div></div><nav aria-label={t('Main navigation')}>{NAV_GROUPS.map(group=>({group,items:navigation.filter(item=>group.pages.includes(item.id)&&(expert||!EXPERT_PAGES.includes(item.id))&&(item.id!=='onboarding'||!guideDone))})).filter(({items})=>items.length>0).map(({group,items})=><div className="nav-group" key={group.label}><div className="nav-group-label">{t(group.label)}</div>{items.map(item=>{const current=page===item.id||(page==='setup'&&item.id==='settings')||(page==='onboarding'&&item.id==='settings'&&guideDone);const finished=done.has(item.id);return <button key={item.id} className={`nav-item ${item.step?'nav-step':''} ${finished?'nav-done':''} ${current?'selected':''}`} aria-current={current?'page':undefined} onClick={()=>setPage(item.id)}>{item.step?<span className="nav-step-number" aria-hidden="true">{finished?'✓':item.step}</span>:<NavIcon type={item.icon}/>}{t(item.label)}</button>;})}</div>)}</nav><Notifications items={notifications} clear={clearNotifications} focus={focusEvent}/><div className="sidebar-bottom"><span className="version">{APP_VERSION}</span><button className="tertiary about-link" onClick={()=>{setPage('settings');requestAnimationFrame(()=>{const details=window.document.getElementById('app-about');if(details instanceof HTMLDetailsElement)details.open=true;details?.scrollIntoView?.({block:'start'});});}}>{t('About this build')}</button></div></aside>
    <div className="workspace-shell"><header className="topbar"><div className="project-identity"><span>{document?.project.name ?? t('Project workspace')}</span>{projectDirty && <span className="dirty-indicator" aria-label={t('Unsaved project changes')}>●</span>}{document && <HistoryButtons controls={history}/>}<span className="muted">/ {page==='setup'?t('Model tool setup'):page==='onboarding'?t('New-user setup'):t(navigation.find(item => item.id === page)?.label ?? '')}</span></div><div className="row project-controls"><div className="segmented topbar-mode" role="radiogroup" aria-label={t('Interface mode')}>{([[false,m('Simple')],[true,m('Expert')]] as const).map(([value,label])=><button key={label} type="button" role="radio" aria-checked={expert===value} title={value?t('Every page, table and tool'):t('The five steps with recommended settings')} onClick={()=>setExpert(value)}>{t(label)}</button>)}</div><button className="topbar-button" disabled={unavailable} onClick={() => void newProject()}>{t('New')}</button><button className="topbar-button" disabled={unavailable} onClick={() => void openProject()}>{t('Open…')}</button><button className="topbar-button" disabled={unavailable || !document} onClick={() => void saveProject()}>{t('Save')}</button><button className="topbar-button" disabled={unavailable || !document} onClick={() => void saveProject(true)}>{t('Save as…')}</button></div></header>
      <main className={`workspace page-${page} ${['project','appearance','physics','motion','build','timings','logs','settings'].includes(page)?'studio-workspace':''}`}>
        {languageOffer && <LanguageOffer choose={next=>void chooseLanguage(next)} disabled={unavailable}/>}
        {!desktop && <div className="capability-notice"><strong>{t('Desktop connection required')}</strong><p>{t('This browser cannot access the Rust backend. Run the Tauri desktop application to open projects and analyze models.')}</p></div>}
        {settingsDirty && !['settings','onboarding','setup'].includes(page) && <div className="capability-notice"><strong>{t('Tool settings have unsaved changes')}</strong><p>{t('Save settings to use the tool paths you selected.')}</p><button disabled={unavailable} onClick={()=>void persistSettings()}>{t('Save settings')}</button></div>}
        {error && (error.full
          ? <ErrorNotice {...error.record} shownTitle={shownTitle(error.record)} shownDetails={shownDetails(error.record)} dismiss={() => setError(null)} go={target => { setError(null); setPage(target as Page); }} />
          : <div className="error-line" role="alert"><Badge kind="error"/><strong>{shownTitle(error.record)}</strong>{error.logged && <span className="error-line-hint">{t('Details in Notifications.')}</span>}<button className="tertiary" onClick={() => setError({...error, full: true})}>{t('Show details')}</button><button className="tertiary error-line-close" aria-label={t('Close')} title={t('Close')} onClick={() => setError(null)}>×</button></div>)}
        {copyNotice && document?.project.original_model===copyNotice.model && (page==='prepare'||page==='project') && <HeroCopyNotice notice={copyNotice} dismiss={()=>setCopyNotice(null)}/>}
        {page === 'project' && (document ? <ProjectOverview document={document} report={inspection?.report??null} fresh={fresh} disabled={unavailable} update={updateProject} selectHero={hero=>void selectHero(hero)} copyForHero={hero=>void copyForHero(hero)} fail={fail} go={navigate} loadOutfit={()=>void loadOutfit()} saveOutfit={()=>void saveOutfit()} chooseModel={()=>void chooseModel()} prepareAgain={prepareAgain}/> : <><PageTitle title={t('Character & model')} subtitle={t('Choose the hero your mod replaces and the model you want to use.')}/><div className="empty-state"><h2>{desktop?t('Loading workspace…'):t('Open the desktop application')}</h2><p>{t('Project data comes from the Rust backend. No sample project is loaded.')}</p></div></>)}
        {page === 'appearance' && <Appearance expert={expert} next={()=>navigate('physics')} openColors={()=>{setPage('extras');requestAnimationFrame(()=>window.document.getElementById('extras-tab-colors')?.click());}} autoLoad={autoLoadPreview} previewCurrent={previewCurrent} weapons={document?<WeaponsSection project={document.project} update={updateProject} busy={unavailable} operate={run} refreshPreview={()=>void loadPreview(undefined,true)} openPreparation={prepareAgain} separate={{projectPath:document.path,saveProject:saveDuringOperation}}/>:undefined} report={inspection?.report??null} fresh={inspection?.project===JSON.stringify(document?.project)} openPreparation={()=>setPage('prepare')} document={document} summary={summary} choose={() => void chooseModel()} analyze={path => void analyze(path)} busy={unavailable} drag={drag ? t('Release to analyze {file}', {file: drag}) : null} update={updateProject} fail={fail} preview={preview} loadPreview={()=>void loadPreview()} importPackage={()=>void importSourcePackage()} analyzeMaterials={()=>void analyzeMaterials()} />}
        {document&&<div hidden={page!=='physics'}><PhysicsPage expert={expert} tangtangPresets={capabilities?.tangtang_presets??true} vindictaReference={capabilities?.vindicta_reference??true} preview={page==='physics'?<CharacterPreview project={document.project} preview={preview} load={()=>loadPreview(undefined,true)} autoLoad={autoLoadPreview} previewCurrent={previewCurrent} busy={unavailable} report={inspection?.report??null} fail={fail} analyze={()=>inspectProject()} analyzing={busy==='Project analysis did not complete'}/>:null} openPreparation={()=>setPage('prepare')} next={()=>navigate('build')} report={inspection?.report??null} fresh={fresh} key={document.project.id} project={document.project} update={updateProject} disabled={unavailable} fail={fail} summary={summary}><ClothAuthoringPanel project={document.project} update={updateProject} busy={unavailable} operate={run} onPreview={path=>{void loadPreview(path);}}/></PhysicsPage></div>}
        {page==='motion'&&document&&<MotionPage canDiscover={Boolean(settings?.pak)&&!settingsDirty} expert={expert} operate={run} openPreparation={()=>setPage('prepare')} preview={preview} load={()=>void loadPreview(undefined,true)} autoLoad={autoLoadPreview} previewCurrent={previewCurrent} next={()=>navigate('build')} key={document.project.id} project={document.project} update={updateProject} disabled={unavailable} fail={fail}/>}
        {page==='extras'&&document&&<ExtrasPage catalog={catalog} discover={()=>discoverComponents(true)} discovering={busy==='Character asset discovery did not complete'} canDiscover={Boolean(settings?.pak)&&!settingsDirty} openSetup={()=>setPage('settings')} openPreparation={()=>setPage('prepare')} reviewBuild={()=>navigate('build')} operate={run} autoPreviews={autoPreviews} key={`${document.project.id}:${document.project.original_model}`} project={document.project} update={updateProject} disabled={unavailable} fail={fail}/>}
        {document&&<div hidden={page!=='prepare'}><PreparationPage expert={expert} defaultFit={defaultFit(savedSettings)} active={page==='prepare'} autoPreviews={autoPreviews} key={document.project.id} project={document.project} update={updateProject} busy={unavailable} operate={run} projectPath={document.path} saveProject={saveDuringOperation} next={()=>{pushEvent({kind:'success',operation:'prepare',title:m('Model prepared'),project:documentRef.current?.project.name,lines:[m('The model was prepared. Check the gun fit and the materials on Appearance & weapons.')],open:{label:m('Open Appearance & weapons'),page:'appearance'}});setPage('appearance');}}/></div>}
        {page==='build'&&document&&<BuildPage expert={expert} analyzing={busy==='Project analysis did not complete'} go={navigate} project={document.project} update={updateProject} report={inspection?.report??null} fresh={fresh} previous={previousAnalysis} slots={slots} scanSlots={()=>void scanSlots()} prepared={prepared} original={originalValidation} busy={unavailable} inspect={()=>void inspectProject()} prepare={offline=>void prepareProject(offline)} validateOriginal={compile=>void validateOriginal(compile)} build={(output,install)=>void buildProject(output,install)} result={buildResult} install={(path,overwrite)=>void installProject(path,overwrite)} installed={installed} chooseFreeSlot={()=>void chooseFreeSlot()} openOutput={()=>void run(m('Output folder could not be opened'),async()=>{await api.openOutput(document.project);})} openProjectFile={path=>void openProject(path)}/>}
        {page === 'timings' && <Timings report={report} openReport={() => void openReport()} busy={unavailable} />}
        {page === 'logs' && <><LogView entries={logs} copy={() => void copyLog()} save={() => void saveLog()} busy={unavailable} /><ToolLogs busy={!desktop} fail={fail}/></>}
        {page === 'settings' && <><SettingsPage fields={fieldChecks} savePreference={values=>void savePreference(values)} chooseLanguage={next=>void chooseLanguage(next)} ignoredNotifications={ignoredStates.length} showIgnoredNotifications={()=>setIgnored([])} expert={expert} theme={theme} chooseTheme={next=>void chooseTheme(next)} customThemes={customThemes} saveCustomThemes={presets=>void saveCustomThemes(presets)} guideShown={!guideDone} setGuideShown={shown=>setGuideDone(!shown)} go={page=>setPage(page as Page)} testSetup={testSetup} onboarding={()=>{setGuideDone(false);setPage('onboarding');}} modelSetup={()=>setPage('setup')} settings={settings} update={value => {setSettings(value);setSetupStatus(null);}} persist={() => void persistSettings()} busy={unavailable} dirty={settingsDirty} fail={fail} detect={extra=>void detectSettings(extra)} status={setupStatus} check={()=>void checkSetup()} />{settings&&desktop&&<StorageSection settings={settings} update={value=>setSettings(value)} project={document?.project??null} busy={unavailable} dirty={settingsDirty} operate={run}/>}{settings&&expert&&<Disclosure title={t('Tool diagnostics')} hint={t('Compiler candidates, configuration audits and Blender validation')}><ToolDiagnostics settings={settings} blenderSaved={path=>{setSettings(current=>current?{...current,blender:path}:current);setSavedSettings(current=>current?{...current,blender:path}:current);setSetupStatus(null);setInspection(null);}} disabled={unavailable} operate={run}/></Disclosure>}{expert&&<AboutBuild version={APP_VERSION}/>}</>}
        {settings&&document&&<div hidden={page!=='onboarding'}><OnboardingPage fields={fieldChecks} chooseLanguage={next=>void chooseLanguage(next)} expert={expert} startStep={guideStep} active={page==='onboarding'} project={document.project} settings={settings} busy={unavailable} dirty={settingsDirty} status={setupStatus} updateSettings={value=>{setSettings(value);setSetupStatus(null);}} saveSettings={()=>void persistSettings()} testSetup={testSetup} verify={()=>void verifySetup()} detect={extra=>void detectSettings(extra)} check={()=>void checkSetup()} fail={fail} go={setPage} openProject={()=>void openProject()} chooseModel={()=>void chooseModel()} analyze={()=>{setPage('build');void inspectProject();}} finish={()=>{setGuideDone(true);setPage('project');}}/></div>}
        {settings&&document&&<div hidden={page!=='setup'}><button className="tertiary" disabled={unavailable} onClick={()=>setPage('settings')}>{t('← Back to settings')}</button><SetupPage settings={settings} project={document.project} updateSettings={value=>{setSettings(value);setSetupStatus(null);}} saveSettings={async next=>{await api.saveSettings(next);setSettings(next);setSavedSettings(next);setSetupStatus(null);setInspection(null);}} busy={unavailable} operate={run}/></div>}
      </main><footer className={`statusbar ${busy||backendBusy||finished?'statusbar-working':'statusbar-idle'}`}>{busy||backendBusy||finished?<div className={`statusbar-progress ${progress===null&&!finished?'indeterminate':''} ${finished??''}`} role="progressbar" aria-label={t('Progress')} aria-valuemin={0} aria-valuemax={100} aria-valuenow={finished?100:progress===null?undefined:Math.round(progress*100)}><div className="statusbar-progress-fill" style={finished?{width:'100%'}:progress===null?undefined:{width:`${Math.max(2,progress*100)}%`}}/></div>:null}<span className="statusbar-text" aria-live="polite">{finished?(finished==='succeeded'?t('Done'):t('Stopped with an error')):busy||backendBusy?`${progress===null?'':`${Math.round(progress*100)}% · `}${runningStep??t(runningText(busy))}${remaining?` · ${timeLeftText(remaining)}`:''}`:desktop?t('Ready'):t('Desktop backend unavailable')}</span><div className="row">{settingsDirty && <span className="muted">{t('Settings unsaved')}</span>}{((busy&&cancellable)||(!busy&&backendBusy)) && <button onClick={() => void cancel()}>{t('Cancel')}</button>}</div></footer>
    </div>{toast && <div className="toast" role="status">{toast}</div>}<SteamQrDialog/>
  </div></HistoryContext.Provider>;
}
