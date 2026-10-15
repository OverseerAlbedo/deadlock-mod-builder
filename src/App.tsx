import { version as APP_VERSION } from "../package.json";
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { ask, open, save } from '@tauri-apps/plugin-dialog';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { api, basename, errorHelp, errorText, isDesktop, type VpkSlots } from './api';
import { BrandMark } from './components/BrandMark';
import { applyTheme, themeById } from './theme/themes';
import type { ThemePreset } from './models';
import { saveBuildRecord } from './build/changes';
import type { ErrorHelp, HeroRecord, LogEntry, ModelSummary, Project, ProjectDocument, Settings, SetupStatus, TimingEvent, TimingReport } from './models';
import { ProjectOverview } from './components/ProjectOverview';
import { ExtrasPage, MotionPage, PhysicsPage } from './components/CustomizationPages';
import { decodePreview, type PreviewScene } from './preview/packet';
import { ToolDiagnostics } from './setup/ToolDiagnostics';
import { ClothAuthoringPanel } from './cloth/ClothAuthoringPanel';
import { SetupPage } from './setup/SetupPage';
import { OnboardingPage } from './setup/OnboardingPage';
import { PreparationPage } from './preparation/PreparationPage';
import { WeaponsSection } from './preparation/WeaponsSection';
import { BuildPage } from './build/BuildPage';
import type { BuildResult, ProjectInspection, OriginalValidation, PreparedModel } from './build/models';
import { Appearance } from './components/Appearance';
import type { ComponentCatalog } from './components/AssetCatalog';
import { ToolLogs } from './components/ToolLogs';
import { LogView } from './components/LogView';
import { Disclosure, ErrorNotice, NavIcon, PageTitle } from './components/Primitives';
import { SettingsPage } from './components/SettingsPage';
import { StorageSection } from './components/StorageSection';
import { CharacterPreview } from './components/CharacterPreview';
import { Timings } from './components/Timings';
import { revealSection } from './components/workflowNavigation';

/** How often a project with unsaved changes is autosaved. */
export const AUTOSAVE_MS = 3 * 60 * 1000;
/** The saved project file is current again: its autosaves are not needed. */
const forgetAutosaves = (id: string) => { void Promise.resolve().then(() => api.clearAutosaves(id)).catch(() => undefined); };
const GUIDE_SEEN = 'dmb.setupGuideSeen', GUIDE_SHOWN = 'dmb.setupGuideShownThisSession', GUIDE_DONE = 'dmb.setupGuideDone', UI_MODE = 'dmb.uiMode';
/** Pages only Expert mode lists in the sidebar. */
const EXPERT_PAGES: Page[] = ['motion', 'extras', 'timings'];
/** Paths a build cannot do without; a problem in one of them opens the setup guide. */
const REQUIRED_FIELDS = ['workspace', 'deadlock', 'pak', 'source2viewer', 'resourcecompiler', 'csdk', 'cswin'];
/** "about 2 minutes left" from earlier runs of the same operation. */
export function timeLeft(typical: number | undefined, startedUtc: string | undefined, now: number): string | null {
  if (!typical || !startedUtc) return null;
  const elapsed = (now - Date.parse(startedUtc)) / 1000;
  if (!Number.isFinite(elapsed)) return null;
  const left = typical - elapsed;
  if (left < -Math.max(30, typical * 0.25)) return 'taking longer than usual';
  if (left <= 15) return 'almost done';
  return left < 90 ? `about ${Math.round(left / 10) * 10} seconds left` : `about ${Math.round(left / 60)} minutes left`;
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
  {id:'onboarding',label:'New-user setup',icon:'settings'},
  {id:'project',label:'Character & model',icon:'project',step:1},
  {id:'prepare',label:'Prepare model',icon:'model',step:2},
  {id:'appearance',label:'Appearance & weapons',icon:'model',step:3},
  {id:'physics',label:'Physics & preview',icon:'model',step:4},
  {id:'build',label:'Build & install',icon:'project',step:5},
  {id:'motion',label:'Motion',icon:'model'}, {id:'extras',label:'Extras',icon:'project'},
  {id:'timings',label:'Step timings',icon:'time'}, {id:'logs',label:'Logs',icon:'log'},
  {id:'settings',label:'Settings',icon:'settings'},
];
/** Ctrl+1 to Ctrl+5 open the five steps. */
const STEP_PAGES: Page[] = ['project', 'prepare', 'appearance', 'physics', 'build'];
const NAV_GROUPS: {label: string; pages: Page[]}[] = [
  // The guide stays here until Done on its last step, so a new user who follows
  // one of its buttons into a step can come back to it.
  {label:'Getting started',pages:['onboarding']},
  {label:'Make your mod',pages:['project','prepare','appearance','physics','build']},
  {label:'More customization',pages:['motion','extras']},
  {label:'Tools',pages:['timings','logs','settings']},
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
  const settingsDirty = settings !== null && (savedSettings === null || Object.keys(settings).some(key => settings[key as keyof Settings] !== savedSettings[key as keyof Settings]));
  const [setupStatus,setSetupStatus]=useState<SetupStatus|null>(null);
  const [catalog,setCatalog]=useState<ComponentCatalog|null>(null);
  useEffect(()=>setCatalog(null),[document?.project.id,document?.project.original_model,settings?.pak]);
  const [inspection,setInspection]=useState<{report:ProjectInspection;project:string}|null>(null);
  // Whether the last Analyze saw the project as it is now (serialized once per change, not per render).
  const fresh=useMemo(()=>Boolean(document&&inspection?.project===JSON.stringify(document.project)),[document,inspection]);
  const [buildResult,setBuildResult]=useState<BuildResult|null>(null);
  const [installed,setInstalled]=useState<string|null>(null);
  const [prepared,setPrepared]=useState<PreparedModel|null>(null);
  const [originalValidation,setOriginalValidation]=useState<OriginalValidation|null>(null);
  const [preview, setPreview] = useState<PreviewScene|null>(null);
  const [summary, setSummary] = useState<ModelSummary | null>(null);
  const [report, setReport] = useState<TimingReport | null>(null);
  const [backendRunning,setBackendRunning]=useState(false);
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [cancellable, setCancellable] = useState(false);
  const [error, setError] = useState<{title: string; details: string; help: ErrorHelp | null} | null>(null);
  const [toast, setToast] = useState('');
  const [guideStep, setGuideStep] = useState<number|undefined>(undefined);
  // Done on the guide's last step takes it out of the sidebar; Settings brings it back. Kept on this PC.
  const [guideDone, setGuideDoneState] = useState(() => { try { return localStorage.getItem(GUIDE_DONE) === '1'; } catch { return false; } });
  const setGuideDone = (value: boolean) => { setGuideDoneState(value); try { localStorage.setItem(GUIDE_DONE, value ? '1' : '0'); } catch { /* storage unavailable */ } };
  // The VPK slot scan, refreshed at every Analyze and from Build & install.
  const [slots, setSlots] = useState<VpkSlots|null>(null);
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
    } catch (cause) { fail('The theme could not be saved', cause); }
  }
  async function saveCustomThemes(presets: ThemePreset[]) {
    if (!settings) return;
    try {
      if (desktop) await api.saveThemePresets(presets);
      // The same array in both, so the tool-path "unsaved changes" check stays quiet.
      setSettings(current => current ? {...current, custom_themes: presets} : current);
      setSavedSettings(current => current ? {...current, custom_themes: presets} : current);
    } catch (cause) { fail('The theme could not be saved', cause); }
  }
  // Simple (the default) hides expert pages and tools; Expert shows everything. Kept on this PC.
  const [expert, setExpertState] = useState(() => { try { return localStorage.getItem(UI_MODE) === 'expert'; } catch { return false; } });
  const setExpert = (value: boolean) => { setExpertState(value); try { localStorage.setItem(UI_MODE, value ? 'expert' : 'simple'); } catch { /* storage unavailable */ } };
  const [durations, setDurations] = useState<Record<string,number>>({});
  const [now, setNow] = useState(() => Date.now());
  const [drag, setDrag] = useState<string | null>(null);
  const active = useRef(false);
  const nativeActive=useRef(false);nativeActive.current=backendRunning;
  const alive = useRef(true);
  const desktop = isDesktop();
  const fail = useCallback((title: string, cause: unknown) => {
    const message=typeof cause==='object'&&cause!==null&&'message' in cause&&typeof cause.message==='string'?cause.message:title;
    setError({title:message, details:errorText(cause), help:errorHelp(cause)});
  }, []);
  const notify = useCallback((message: string) => setToast(message), []);
  const run = useCallback(async <T,>(label: string, operation: () => Promise<T>, canCancel = false, refreshTimings=true): Promise<T | undefined> => {
    if (active.current) return undefined;
    active.current = true;
    setBusy(label); setCancellable(canCancel); setError(null);
    try { return await operation(); }
    catch (cause) {
      if (typeof cause === 'object' && cause !== null && 'kind' in cause && cause.kind === 'cancelled') notify('Operation cancelled');
      else fail(label, cause);
    } finally { try{if(refreshTimings){const latest=await api.latestTimings();if(alive.current){setBackendRunning(latest?.status==='running');if(latest)setReport(latest);}}}catch{/* Preserve the original operation error if its receipt cannot be read. */} active.current = false; if (alive.current) { setBusy(null); setCancellable(false); } }
    return undefined;
  }, [fail, notify]);
  useEffect(() => {
    alive.current = true;
    if (!desktop) return;
    let disposed = false;
    let releaseLogs: UnlistenFn | undefined; let releaseTimings: UnlistenFn | undefined;
    let timingPending=false, timingReading=false, timingRunning=false;
    const queued: LogEntry[] = [];
    const merge = (incoming: LogEntry[]) => setLogs(current => [...new Map([...current, ...incoming].map(entry => [entry.sequence, entry])).values()].sort((a,b) => a.sequence-b.sequence).slice(-20000));
    const timer = window.setInterval(() => { if (queued.length) merge(queued.splice(0)); }, 50);
    const timingTimer=window.setInterval(()=>{if((timingPending||timingRunning)&&!timingReading){timingPending=false;timingReading=true;void api.latestTimings().then(value=>{if(!disposed&&value){timingRunning=value.status==='running';setBackendRunning(timingRunning);setReport(value);}}).catch(cause=>{if(!disposed)fail('Could not refresh step timings',cause);}).finally(()=>{timingReading=false;});}},200);
    void (async () => {
      try {
        const unlisten = await listen<LogEntry>('build-log', event => queued.push(event.payload));
        if (disposed) { unlisten(); return; }
        releaseLogs = unlisten;
        const unlistenTimings=await listen<TimingEvent>('timing-event',()=>{timingPending=true;});
        if(disposed){unlistenTimings();return;}releaseTimings=unlistenTimings;
        const [loadedSettings, loadedProject, retainedLogs, latestReport] = await Promise.all([api.settings(), api.newProject(), api.logs(), api.latestTimings()]);
        if (!disposed) { setSettings(loadedSettings); setSavedSettings(loadedSettings); setDocument(loadedProject); merge(retainedLogs); setReport(latestReport);setBackendRunning(latestReport?.status==='running');timingRunning=latestReport?.status==='running';
        // After a crash, offer the newest autosave.
        const recovery = await Promise.resolve().then(() => api.sessionRecovery()).catch(() => null);
        if (!disposed && recovery?.unclean) {
          const latest = recovery.autosaves[0];
          if (latest) {
            let restore = false;
            try { restore = await ask(`Deadlock Mod Builder did not close normally. Restore the unsaved changes to “${latest.name || 'Untitled project'}” from ${new Date(latest.saved_ms).toLocaleString()}?`, {title:'Restore unsaved changes', kind:'warning', okLabel:'Restore', cancelLabel:'Not now'}); } catch { restore = false; }
            if (restore && !disposed) {
              try { const restored = await api.restoreAutosave(latest.id, latest.file); documentRef.current = restored; setDocument(restored); setProjectDirty(true); notify('Unsaved changes restored. Save the project to keep them.'); }
              catch (cause) { fail('The autosave could not be restored', cause); }
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
              setGuideStep(1); setPage('onboarding'); notify('Some tool paths need fixing. The setup guide shows which.');
            }
          }
        }
      } catch (cause) { if (!disposed) fail('Could not initialize the workspace', cause); }
    })();
    return () => { disposed = true; alive.current = false; releaseLogs?.();releaseTimings?.(); window.clearInterval(timer);window.clearInterval(timingTimer); };
  }, [desktop, fail]);
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

  const discardProject = useCallback(async () => !projectDirty || await ask('This project has unsaved changes. Discard them and continue?', {title:'Unsaved project',kind:'warning',okLabel:'Discard changes',cancelLabel:'Keep editing'}), [projectDirty]);
  const saveProject = useCallback(async (saveAs = false) => {
    if (!document || active.current) return;
    await run('Could not save the project', async () => {
      const path = !saveAs && document.path ? document.path : await save({title:'Save project',defaultPath:document.path ?? `${document.project.name}.dmb.json`,filters:[{name:'Deadlock Mod Builder project',extensions:['json']}]});
      if (!path) return;
      const saved = await api.saveProject(path, document.project);
      setDocument(saved); setProjectDirty(false); forgetAutosaves(saved.project.id); notify('Project saved');
    });
  }, [document, run, notify]);
  /** Save inside an operation that is already running (no second busy state); resolves to the saved path. */
  const saveDuringOperation = useCallback(async () => {
    const current = documentRef.current;
    if (!current) return undefined;
    const path = current.path ?? await save({title:'Save project',defaultPath:`${current.project.name}.dmb.json`,filters:[{name:'Deadlock Mod Builder project',extensions:['json']}]});
    if (!path) return undefined;
    const saved = await api.saveProject(path, current.project);
    setDocument(saved); setProjectDirty(false); forgetAutosaves(saved.project.id);
    return saved.path ?? path;
  }, []);
  const openProject = useCallback(async () => {
    if (active.current) return;
    await run('Could not open the project', async () => {
      if (!await discardProject()) return;
      const path = await open({title:'Open project',multiple:false,filters:[{name:'Deadlock Mod Builder project',extensions:['json']}]});
      if (typeof path !== 'string') return;
      const loaded = await api.openProject(path);
      setDocument(loaded); setProjectDirty(false); setSummary(null);setPreview(null);setInspection(null);setPrepared(null);setOriginalValidation(null);setBuildResult(null);setInstalled(null); setPage('project'); notify('Project opened');
    });
  }, [run, discardProject, notify]);
  const newProject = useCallback(async () => {
    if (active.current) return;
    await run('Could not create a project', async () => {
      if (!await discardProject()) return;
      setDocument(await api.newProject()); setProjectDirty(false); setSummary(null);setPreview(null);setInspection(null);setPrepared(null);setOriginalValidation(null);setBuildResult(null);setInstalled(null); setPage('project');
    });
  }, [run, discardProject]);
  const analyze = useCallback(async (path: string) => {
    if (!document || active.current) return;
    await run('Model analysis did not complete', async () => {
      try {
        const result = await api.analyze(path);
        void scanSlots();
        setSummary(result.summary);setPreview(null);setInspection(null); setReport(result.timings);
        setDocument(current => current ? {...current,project:{...current.project,dmx:result.summary.path}} : current);
        if (document.project.dmx !== result.summary.path) setProjectDirty(true);
        setPage('appearance'); notify('Model analysis complete');
      } finally { try { const latest = await api.latestTimings(); if (latest) setReport(latest); } catch (cause) { fail('Could not retrieve the timing report', cause); } }
    }, true);
  }, [document, run, notify, fail]);
  /** Automatic preparation of a Blender, FBX or glTF model. A gun in a separate file is asked
   * for first: the fields are locked while preparation runs. */
  const startPreparation = useCallback(async (path: string) => {
    const current = documentRef.current;
    if (current && !current.project.preparation_options.weapon_source) {
      let wants = false;
      try { wants = await ask('Is your gun in a separate file? Choose it now and it is prepared together with your character. You can also add it later on the Weapon tab.', {title:'Gun file', kind:'info', okLabel:'Choose gun file…', cancelLabel:'No, prepare now'}); } catch { wants = false; }
      if (wants) {
        try {
          const gun = await open({title:'Choose the gun file', multiple:false, filters:[{name:'Gun file', extensions:['blend','glb','gltf','fbx']}]});
          if (typeof gun === 'string') {
            const checked = await api.validateAssetPath(gun, 'weapon');
            const latest = documentRef.current;
            if (latest) {
              const next = {...latest, project:{...latest.project, preparation_options:JSON.parse(JSON.stringify({...latest.project.preparation_options, weapon_source:checked, weapon:undefined, reviewed:false}))}};
              documentRef.current = next; setDocument(next); setProjectDirty(true);
            }
          }
        } catch (cause) { fail('The gun file could not be used', cause); }
      }
    }
    setPage('prepare');
    window.dispatchEvent(new CustomEvent('dmb-model-source', {detail:{path, auto:true}}));
  }, [fail]);
  const chooseModel = useCallback(async () => {
    if (active.current || !document) return;
    try {
      const path = await open({title:'Select replacement model',multiple:false,filters:[{name:'Models',extensions:['dmx','blend','glb','gltf','fbx']}]});
      // Blender, FBX and glTF models are inspected and prepared automatically (step 2).
      if (typeof path === 'string') {if(/\.dmx$/i.test(path))await analyze(path);else await startPreparation(path);}
    } catch (cause) { fail('Could not choose a model', cause); }
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
        setDrag(!target && payload.paths.length === 1 && /\.dmx$/i.test(payload.paths[0]) ? `Release to analyze ${basename(payload.paths[0])}` : null);
      } else if (payload.type === 'leave') setDrag(null);
      else if (payload.type === 'drop') {
        setDrag(null);
        if(target){target.dispatchEvent(new CustomEvent('dmb-file-drop',{detail:payload.paths}));return;}
        if(payload.paths.length===1&&/\.(blend|glb|gltf|fbx)$/i.test(payload.paths[0])&&!active.current){void startPreparationRef.current(payload.paths[0]);}
        else if (payload.paths.length !== 1 || !/\.dmx$/i.test(payload.paths[0])) fail('Could not import the dropped file', 'Drop one local file with the .dmx extension, or a Blender, GLB, glTF, or FBX source.');
        else if (active.current) fail('An operation is already running', 'Wait for the current operation to finish or cancel it, then drop the model again.');
        else void analyzeRef.current(payload.paths[0]);
      }
    }).then(unlisten => { if (disposed) unlisten(); else release = unlisten; }).catch(cause => fail('Drag and drop could not be enabled', cause));
    return () => { disposed = true; release?.(); };
  }, [desktop, fail]);
  useEffect(() => {
    if (!desktop) return;
    let disposed = false; let release: UnlistenFn | undefined;
    void getCurrentWindow().onCloseRequested(async event => {
      if (active.current||nativeActive.current) { event.preventDefault(); notify('Finish or cancel the current operation before closing.'); return; }
      if (projectDirty || settingsDirty) {
        event.preventDefault();
        if (await ask('There are unsaved changes. Close without saving?',{title:'Unsaved changes',kind:'warning',okLabel:'Close without saving',cancelLabel:'Keep editing'})) await getCurrentWindow().destroy();
      }
    }).then(unlisten => { if (disposed) unlisten(); else release = unlisten; }).catch(cause => fail('Could not register close protection', cause));
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

  async function discoverComponents(extrasOnly=false){if(!document)return;const requested=document.project;await run('Character asset discovery did not complete',async()=>{const next=await (extrasOnly?api.discoverExtras(requested):api.discoverComponents(requested));if(documentRef.current?.project.id===requested.id&&documentRef.current.project.original_model===requested.original_model)setCatalog(next);notify('Character assets loaded.');},true);}
  async function saveOutfit(){if(!document)return;await run('Outfit could not be saved',async()=>{const path=await save({title:'Save outfit preset',defaultPath:`${document.project.name}.outfit.json`,filters:[{name:'Outfit preset',extensions:['json']}]});if(path){await api.outfitSave(document.project,path,document.project.name);notify('Outfit preset saved.');}});}
  async function loadOutfit(){if(!document)return;await run('Outfit could not be loaded',async()=>{const path=await open({title:'Load outfit preset',multiple:false,filters:[{name:'Outfit preset',extensions:['json']}]});if(typeof path==='string'){updateProject(await api.outfitLoad(document.project,path));setSummary(null);setPreview(null);notify('Outfit preset loaded. Review its choices.');}});}
  async function chooseFreeSlot(){if(!document)return;await run('Could not choose a free VPK slot',async()=>{const value=await api.chooseFreeSlot(document.project);updateProject({...document.project,vpk_number:value.slot});notify(`Selected available slot: ${value.filename}`);});}
  async function buildProject(overwriteOutput:boolean,overwriteInstall:boolean){if(!document||active.current)return;
    // Common causes of failed builds, before a long build starts; the user may build anyway.
    let warnings:string[]=[];try{warnings=(await api.prebuildWarnings())??[];}catch{warnings=[];}
    if(warnings.length&&!await ask(`${warnings.map(w=>`• ${w}`).join('\n\n')}\n\nBuild anyway?`,{title:'Before building',kind:'warning',okLabel:'Build anyway',cancelLabel:'Cancel'}))return;
    await run('Build did not complete',async()=>{setBuildResult(null);const value=await api.buildProject(document.project,overwriteOutput,overwriteInstall,document.path);const analyzed=JSON.stringify(document.project);setDocument({project:value.project,path:value.path});setProjectDirty(false);
      // The build checked exactly the analyzed inputs; what it records in the project is not a change to analyze again.
      setInspection(prev=>prev&&prev.project===analyzed?{...prev,project:JSON.stringify(value.project)}:prev);forgetAutosaves(value.project.id);setBuildResult(value.result);if(value.result.compiled||value.result.vpk)saveBuildRecord(value.project);notify(value.result.installed?'Verified VPK installed.':value.result.vpk?'Verified VPK published.':value.result.compiled?'Compilation completed; no VPK was requested.':'Source preparation completed; no compilation was requested.');},true);}
  async function installProject(selected:string|null,overwrite:boolean){if(!document)return;await run('Installation did not complete',async()=>{const path=selected??await open({title:'Select VPK to validate and install',multiple:false,filters:[{name:'VPK archive',extensions:['vpk']}]});if(typeof path!=='string')return;setInstalled(null);setInstalled(await api.installProject(document.project,path,overwrite));notify('VPK validated and installed.');},true);}
  /** The game's addons folder and the output folder, read again (Analyze, Build & install). */
  async function scanSlots(){if(!document)return;try{setSlots(await api.vpkSlots(document.project));}catch{setSlots(null);}}
  async function inspectProject(){if(!document)return;await run('Project analysis did not complete',async()=>{
    setInspection(null);const value=await api.inspectProject(document.project);updateProject(value.project);setInspection({report:value.report,project:JSON.stringify(value.project)});if('dmx' in value.report)setSummary({...value.report.dmx,path:value.project.dmx});await scanSlots();notify('needs_review' in value.report&&value.report.needs_review?'Review the analysis before building.':'Project analysis complete.');
  },true);}
  async function prepareProject(offline:boolean){if(!document)return;await run('Source preparation did not complete',async()=>{const value=await api.prepareProject(document.project,offline);updateProject(value.project);setPrepared(value.prepared);notify('Compiler source files prepared.');},true);}
  async function validateOriginal(compile:boolean){if(!document)return;await run('Original model test did not complete',async()=>{setOriginalValidation(await api.validateOriginal(document.project,compile));notify(compile?'Original test completed. Review the native receipt.':'Original compiler plan inspected.');},true);}
  async function importSourcePackage(){
    if(!document)return;
    await run('Source package import did not complete',async()=>{
      const path=await open({title:'Import source package',multiple:false,filters:[{name:'Verified source package',extensions:['zip']}]});
      if(typeof path!=='string')return;
      const project=await api.importSourcePackage(path,document.project);
      updateProject(project);setSummary(null);setPreview(null);notify('Source package imported and verified');
    },true);
  }
  async function analyzeMaterials(){
    if(!document)return;
    await run('Material analysis did not complete',async()=>{updateProject(await api.analyzeMaterials(document.project));notify('Material analysis complete. Review assignments before building.');},true);
  }
  async function detectSettings(extraRoot=false){
    if(!settings)return;
    await run('Tool detection did not complete',async()=>{
      const roots:string[]=[];
      if(extraRoot){const selected=await open({title:'Search for installed tools',directory:true,multiple:false});if(typeof selected!=='string')return;roots.push(selected);}
      const detected=await api.detectSettings(settings,roots);setSettings(detected);setSetupStatus(await api.setupStatus(detected));notify('Detection complete. Review and save settings.');
    },true);
  }
  async function checkSetup(){if(settings)await run('Could not check tool paths',async()=>{setSetupStatus(null);setSetupStatus(await api.setupStatus(settings));});}
  async function verifySetup(){if(settings)await run('Could not save and check tools',async()=>{setSetupStatus(null);await api.saveSettings(settings);setSavedSettings(settings);setInspection(null);setSetupStatus(await api.setupStatus(settings));notify('Settings saved and tool locations checked.');});}
  /** Test my setup with the paths on screen (saved or not). */
  async function testSetup(){return settings?run('Test my setup',()=>api.testSetup(settings)):undefined;}
  async function loadPreview(path=document?.project.dmx,stay=false){if(!path)return;await run('Preview could not be loaded',async()=>{const buffer=await api.preview(path);setPreview(decodePreview(buffer));if(!stay)setPage('appearance');notify('Preview loaded');},true);}
  function updateProject(project:Project){const current=documentRef.current;if(!current)return;if(current.project!==document?.project){notify('Project changed while the selection was being checked. Please select it again.');return;}const next={...current,project};documentRef.current=next;setDocument(next);setProjectDirty(true);}
  async function selectHero(hero:HeroRecord){if(!document)return;await run('Could not select the character',async()=>{const project=await api.selectHero(document.project,hero.path);updateProject(project);setSummary(null);setPreview(null);notify('Character selected');});}
  async function openReport() {
    await run('Could not open the timing report', async () => {
      const path = await open({title:'Open step timing report',multiple:false,filters:[{name:'Timing report',extensions:['json']}]});
      if (typeof path === 'string') { setReport(await api.timings(path)); notify('Timing report opened'); }
    },false,false);
  }
  async function copyLog() { await run('Could not copy the log', async () => { await navigator.clipboard.writeText(await api.logText()); notify('Session log copied'); }); }
  async function saveLog() { await run('Could not save the log', async () => { const path = await save({title:'Save complete log',defaultPath:'deadlock-build.log'}); if (path) { await api.saveLog(path); notify('Full log saved'); } }); }
  async function persistSettings() { if (settings) await run('Could not save settings', async () => { await api.saveSettings(settings); setSavedSettings(settings); setInspection(null); notify('Settings saved'); }); }
  async function cancel() { try { await api.cancel(); notify('Cancellation requested'); } catch (cause) { fail('Could not cancel the operation', cause); } }
  const backendBusy=backendRunning;
  const running=Boolean(busy)||backendBusy;
  useEffect(()=>{if(!running||!desktop)return;let current=true;Promise.resolve().then(()=>api.operationDurations()).then(value=>{if(current&&value)setDurations(value);}).catch(()=>undefined);const timer=window.setInterval(()=>setNow(Date.now()),1000);return()=>{current=false;window.clearInterval(timer);};},[running,desktop]);
  const remaining=running&&report?.status==='running'?timeLeft(durations[report.operation],report.started_utc,now):null;
  const progress=running&&report?.status==='running'?progressFraction(durations[report.operation],report.started_utc,now):null;
  // A finished operation fills the bar for a moment: green when it worked.
  const [finished,setFinished]=useState<'succeeded'|'failed'|null>(null);
  const wasRunning=useRef(false);
  useEffect(()=>{
    if(running){wasRunning.current=true;setFinished(null);return;}
    if(!wasRunning.current)return;
    wasRunning.current=false;
    setFinished(report?.status==='succeeded'?'succeeded':report?.status==='failed'?'failed':null);
    const timer=window.setTimeout(()=>setFinished(null),2500);
    return()=>window.clearTimeout(timer);
  },[running,report?.status]);
  const runningStep=backendBusy?[...(report?.steps??[])].reverse().find(step=>step.status==='running')?.name:undefined;
  function navigate(page:Page, section?:string){setPage(page);if(section)requestAnimationFrame(()=>revealSection(section));}
  const unavailable = Boolean(busy) || backendBusy || !desktop;
  const done = stepsDone(document?.project, Boolean(buildResult?.vpk || installed));
  return <div className="app-shell">
    <aside className="sidebar"><div className="brand"><span className="brand-mark" aria-hidden="true"><BrandMark theme={theme}/></span><div>Deadlock<span>Mod Builder</span></div></div><nav aria-label="Main navigation">{NAV_GROUPS.map(group=>({group,items:navigation.filter(item=>group.pages.includes(item.id)&&(expert||!EXPERT_PAGES.includes(item.id))&&(item.id!=='onboarding'||!guideDone))})).filter(({items})=>items.length>0).map(({group,items})=><div className="nav-group" key={group.label}><div className="nav-group-label">{group.label}</div>{items.map(item=>{const current=page===item.id||(page==='setup'&&item.id==='settings')||(page==='onboarding'&&item.id==='settings'&&guideDone);const finished=done.has(item.id);return <button key={item.id} className={`nav-item ${item.step?'nav-step':''} ${finished?'nav-done':''} ${current?'selected':''}`} aria-current={current?'page':undefined} onClick={()=>setPage(item.id)}>{item.step?<span className="nav-step-number" aria-hidden="true">{finished?'✓':item.step}</span>:<NavIcon type={item.icon}/>}{item.label}</button>;})}</div>)}</nav><div className="sidebar-bottom"><span className="version">{APP_VERSION}</span><button className="tertiary about-link" onClick={()=>{setPage('settings');requestAnimationFrame(()=>{const details=window.document.getElementById('app-about');if(details instanceof HTMLDetailsElement)details.open=true;details?.scrollIntoView?.({block:'start'});});}}>About this build</button></div></aside>
    <div className="workspace-shell"><header className="topbar"><div className="project-identity"><span>{document?.project.name ?? 'Project workspace'}</span>{projectDirty && <span className="dirty-indicator" aria-label="Unsaved project changes">●</span>}<span className="muted">/ {page==='setup'?'Model tool setup':page==='onboarding'?'New-user setup':navigation.find(item => item.id === page)?.label}</span></div><div className="row project-controls"><div className="segmented topbar-mode" role="radiogroup" aria-label="Interface mode">{([[false,'Simple'],[true,'Expert']] as const).map(([value,label])=><button key={label} type="button" role="radio" aria-checked={expert===value} title={value?'Every page, table and tool':'The five steps with recommended settings'} onClick={()=>setExpert(value)}>{label}</button>)}</div><button className="topbar-button" disabled={unavailable} onClick={() => void newProject()}>New</button><button className="topbar-button" disabled={unavailable} onClick={() => void openProject()}>Open…</button><button className="topbar-button" disabled={unavailable || !document} onClick={() => void saveProject()}>Save</button><button className="topbar-button" disabled={unavailable || !document} onClick={() => void saveProject(true)}>Save as…</button></div></header>
      <main className={`workspace page-${page} ${['project','appearance','physics','motion','build','timings','logs','settings'].includes(page)?'studio-workspace':''}`}>
        {!desktop && <div className="capability-notice"><strong>Desktop connection required</strong><p>This browser cannot access the Rust backend. Run the Tauri desktop application to open projects and analyze models.</p></div>}
        {settingsDirty && !['settings','onboarding','setup'].includes(page) && <div className="capability-notice"><strong>Tool settings have unsaved changes</strong><p>Save settings to use the tool paths you selected.</p><button disabled={unavailable} onClick={()=>void persistSettings()}>Save settings</button></div>}
        {error && <ErrorNotice {...error} dismiss={() => setError(null)} go={target => { setError(null); setPage(target as Page); }} />}
        {page === 'project' && (document ? <ProjectOverview document={document} report={inspection?.report??null} fresh={fresh} disabled={unavailable} update={updateProject} selectHero={hero=>void selectHero(hero)} fail={fail} go={navigate} loadOutfit={()=>void loadOutfit()} saveOutfit={()=>void saveOutfit()} chooseModel={()=>void chooseModel()} prepareAgain={prepareAgain}/> : <><PageTitle title="Character & model" subtitle="Choose the hero your mod replaces and the model you want to use."/><div className="empty-state"><h2>{desktop?'Loading workspace…':'Open the desktop application'}</h2><p>Project data comes from the Rust backend. No sample project is loaded.</p></div></>)}
        {page === 'appearance' && <Appearance weapons={document?<WeaponsSection project={document.project} update={updateProject} busy={unavailable} operate={run} refreshPreview={()=>void loadPreview(undefined,true)} openPreparation={prepareAgain} separate={{projectPath:document.path,saveProject:saveDuringOperation}}/>:undefined} report={inspection?.report??null} fresh={inspection?.project===JSON.stringify(document?.project)} openPreparation={()=>setPage('prepare')} document={document} summary={summary} choose={() => void chooseModel()} analyze={path => void analyze(path)} busy={unavailable} drag={drag} update={updateProject} fail={fail} preview={preview} loadPreview={()=>void loadPreview()} importPackage={()=>void importSourcePackage()} analyzeMaterials={()=>void analyzeMaterials()} />}
        {document&&<div hidden={page!=='physics'}><PhysicsPage expert={expert} preview={page==='physics'?<CharacterPreview project={document.project} preview={preview} load={()=>void loadPreview(undefined,true)} busy={unavailable} report={inspection?.report??null} fail={fail} analyze={()=>void inspectProject()} analyzing={busy==='Project analysis did not complete'}/>:null} openPreparation={()=>setPage('prepare')} report={inspection?.report??null} fresh={fresh} key={document.project.id} project={document.project} update={updateProject} disabled={unavailable} fail={fail} summary={summary}><ClothAuthoringPanel project={document.project} update={updateProject} busy={unavailable} operate={run} onPreview={loadPreview}/></PhysicsPage></div>}
        {page==='motion'&&document&&<MotionPage catalog={catalog?.includes_animations===false?null:catalog} discover={()=>void discoverComponents()} key={document.project.id} project={document.project} update={updateProject} disabled={unavailable} fail={fail}/>}
        {page==='extras'&&document&&<ExtrasPage catalog={catalog} discover={()=>discoverComponents(true)} discovering={busy==='Character asset discovery did not complete'} canDiscover={Boolean(settings?.pak)&&!settingsDirty} openSetup={()=>setPage('settings')} reviewBuild={()=>navigate('build')} key={`${document.project.id}:${document.project.original_model}`} project={document.project} update={updateProject} disabled={unavailable} fail={fail}/>}
        {document&&<div hidden={page!=='prepare'}><PreparationPage expert={expert} key={document.project.id} project={document.project} update={updateProject} busy={unavailable} operate={run} projectPath={document.path} saveProject={saveDuringOperation} next={()=>setPage('appearance')}/></div>}
        {page==='build'&&document&&<BuildPage expert={expert} analyzing={busy==='Project analysis did not complete'} go={navigate} project={document.project} update={updateProject} report={inspection?.report??null} fresh={fresh} slots={slots} scanSlots={()=>void scanSlots()} prepared={prepared} original={originalValidation} busy={unavailable} inspect={()=>void inspectProject()} prepare={offline=>void prepareProject(offline)} validateOriginal={compile=>void validateOriginal(compile)} build={(output,install)=>void buildProject(output,install)} result={buildResult} install={(path,overwrite)=>void installProject(path,overwrite)} installed={installed} chooseFreeSlot={()=>void chooseFreeSlot()} openOutput={()=>void run('Output folder could not be opened',async()=>{await api.openOutput(document.project);})}/>}
        {page === 'timings' && <Timings report={report} openReport={() => void openReport()} busy={unavailable} />}
        {page === 'logs' && <><LogView entries={logs} copy={() => void copyLog()} save={() => void saveLog()} busy={unavailable} /><ToolLogs busy={!desktop} fail={fail}/></>}
        {page === 'settings' && <><SettingsPage expert={expert} theme={theme} chooseTheme={next=>void chooseTheme(next)} customThemes={customThemes} saveCustomThemes={presets=>void saveCustomThemes(presets)} guideShown={!guideDone} setGuideShown={shown=>setGuideDone(!shown)} go={page=>setPage(page as Page)} testSetup={testSetup} onboarding={()=>{setGuideDone(false);setPage('onboarding');}} modelSetup={()=>setPage('setup')} settings={settings} update={value => {setSettings(value);setSetupStatus(null);}} persist={() => void persistSettings()} busy={unavailable} dirty={settingsDirty} fail={fail} detect={extra=>void detectSettings(extra)} status={setupStatus} check={()=>void checkSetup()} />{settings&&desktop&&<StorageSection settings={settings} update={value=>setSettings(value)} project={document?.project??null} busy={unavailable} dirty={settingsDirty} operate={run}/>}{settings&&expert&&<Disclosure title="Tool diagnostics" hint="Compiler candidates, configuration audits and Blender validation"><ToolDiagnostics settings={settings} blenderSaved={path=>{setSettings(current=>current?{...current,blender:path}:current);setSavedSettings(current=>current?{...current,blender:path}:current);setSetupStatus(null);setInspection(null);}} disabled={unavailable} operate={run}/></Disclosure>}{expert&&<Disclosure title="About this build" hint="Version and qualification status" id="app-about"><p>Deadlock Mod Builder Rust · {APP_VERSION}</p><p className="hint">The Vindicta R12 project has been reported working in game. This update recognizes MMD physics helpers during source import and retains the accepted cloth reference. Other projects and toolchains require their own validation.</p></Disclosure>}</>}
        {settings&&document&&<div hidden={page!=='onboarding'}><OnboardingPage startStep={guideStep} active={page==='onboarding'} project={document.project} settings={settings} busy={unavailable} dirty={settingsDirty} status={setupStatus} updateSettings={value=>{setSettings(value);setSetupStatus(null);}} saveSettings={()=>void persistSettings()} testSetup={testSetup} verify={()=>void verifySetup()} detect={extra=>void detectSettings(extra)} check={()=>void checkSetup()} fail={fail} go={setPage} openProject={()=>void openProject()} chooseModel={()=>void chooseModel()} analyze={()=>{setPage('build');void inspectProject();}} finish={()=>{setGuideDone(true);setPage('project');}}/></div>}
        {settings&&document&&<div hidden={page!=='setup'}><button className="tertiary" disabled={unavailable} onClick={()=>setPage('settings')}>← Back to settings</button><SetupPage settings={settings} project={document.project} updateSettings={value=>{setSettings(value);setSetupStatus(null);}} saveSettings={async next=>{await api.saveSettings(next);setSettings(next);setSavedSettings(next);setSetupStatus(null);setInspection(null);}} busy={unavailable} operate={run}/></div>}
      </main><footer className={`statusbar ${busy||backendBusy||finished?'statusbar-working':'statusbar-idle'}`}>{busy||backendBusy||finished?<div className={`statusbar-progress ${progress===null&&!finished?'indeterminate':''} ${finished??''}`} role="progressbar" aria-label="Progress" aria-valuemin={0} aria-valuemax={100} aria-valuenow={finished?100:progress===null?undefined:Math.round(progress*100)}><div className="statusbar-progress-fill" style={finished?{width:'100%'}:progress===null?undefined:{width:`${Math.max(2,progress*100)}%`}}/></div>:null}<span className="statusbar-text" aria-live="polite">{finished?(finished==='succeeded'?'Done':'Stopped with an error'):busy||backendBusy?`${progress===null?'':`${Math.round(progress*100)}% · `}${runningStep??busy??'Working…'}${remaining?` · ${remaining}`:''}`:desktop?'Ready':'Desktop backend unavailable'}</span><div className="row">{settingsDirty && <span className="muted">Settings unsaved</span>}{((busy&&cancellable)||(!busy&&backendBusy)) && <button onClick={() => void cancel()}>Cancel</button>}</div></footer>
    </div>{toast && <div className="toast" role="status">{toast}</div>}
  </div>;
}
