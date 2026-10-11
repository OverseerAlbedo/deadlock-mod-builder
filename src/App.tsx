import { version as APP_VERSION } from "../package.json";
import { useCallback, useEffect, useRef, useState } from 'react';
import { ask, open, save } from '@tauri-apps/plugin-dialog';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { api, basename, errorText, isDesktop } from './api';
import type { HeroRecord, LogEntry, ModelSummary, Project, ProjectDocument, Settings, SetupStatus, TimingEvent, TimingReport } from './models';
import { ProjectOverview } from './components/ProjectOverview';
import { ExtrasPage, MotionPage, PhysicsPage } from './components/CustomizationPages';
import { decodePreview, type PreviewScene } from './preview/packet';
import { ToolDiagnostics } from './setup/ToolDiagnostics';
import { ClothAuthoringPanel } from './cloth/ClothAuthoringPanel';
import { SetupPage } from './setup/SetupPage';
import { OnboardingPage } from './setup/OnboardingPage';
import { PreparationPage } from './preparation/PreparationPage';
import { BuildPage } from './build/BuildPage';
import type { BuildResult, ProjectInspection, OriginalValidation, PreparedModel } from './build/models';
import { Appearance } from './components/Appearance';
import type { ComponentCatalog } from './components/AssetCatalog';
import { ToolLogs } from './components/ToolLogs';
import { LogView } from './components/LogView';
import { Disclosure, ErrorNotice, NavIcon, PageTitle } from './components/Primitives';
import { SettingsPage } from './components/SettingsPage';
import { StorageSection } from './components/StorageSection';
import { Timings } from './components/Timings';
import { revealSection } from './components/workflowNavigation';

type Page = 'project' | 'appearance' | 'physics' | 'motion' | 'extras' | 'timings' | 'logs' | 'settings' | 'build' | 'prepare' | 'setup' | 'onboarding';
const navigation: {id: Page; label: string; icon: 'project' | 'model' | 'time' | 'log' | 'settings'}[] = [
  {id:'project',label:'Overview',icon:'project'}, {id:'appearance',label:'Appearance',icon:'model'},
  {id:'physics',label:'Physics',icon:'model'}, {id:'motion',label:'Motion',icon:'model'}, {id:'extras',label:'Extras',icon:'project'},
  {id:'prepare',label:'Model preparation',icon:'model'},
  {id:'build',label:'Review & build',icon:'project'},
  {id:'timings',label:'Step timings',icon:'time'}, {id:'logs',label:'Logs',icon:'log'},
  {id:'settings',label:'Settings',icon:'settings'},
];
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
  const [error, setError] = useState<{title: string; details: string} | null>(null);
  const [toast, setToast] = useState('');
  const [drag, setDrag] = useState<string | null>(null);
  const active = useRef(false);
  const nativeActive=useRef(false);nativeActive.current=backendRunning;
  const alive = useRef(true);
  const desktop = isDesktop();
  const fail = useCallback((title: string, cause: unknown) => {
    const message=typeof cause==='object'&&cause!==null&&'message' in cause&&typeof cause.message==='string'?cause.message:title;
    setError({title:message, details:errorText(cause)});
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
        if (!disposed) { setSettings(loadedSettings); setSavedSettings(loadedSettings); setDocument(loadedProject); merge(retainedLogs); setReport(latestReport);setBackendRunning(latestReport?.status==='running');timingRunning=latestReport?.status==='running'; }
      } catch (cause) { if (!disposed) fail('Could not initialize the workspace', cause); }
    })();
    return () => { disposed = true; alive.current = false; releaseLogs?.();releaseTimings?.(); window.clearInterval(timer);window.clearInterval(timingTimer); };
  }, [desktop, fail]);
  useEffect(() => { if (!toast) return; const timer = window.setTimeout(() => setToast(''), 4000); return () => window.clearTimeout(timer); }, [toast]);

  const discardProject = useCallback(async () => !projectDirty || await ask('This project has unsaved changes. Discard them and continue?', {title:'Unsaved project',kind:'warning',okLabel:'Discard changes',cancelLabel:'Keep editing'}), [projectDirty]);
  const saveProject = useCallback(async (saveAs = false) => {
    if (!document || active.current) return;
    await run('Could not save the project', async () => {
      const path = !saveAs && document.path ? document.path : await save({title:'Save project',defaultPath:document.path ?? `${document.project.name}.dmb.json`,filters:[{name:'Deadlock Mod Builder project',extensions:['json']}]});
      if (!path) return;
      const saved = await api.saveProject(path, document.project);
      setDocument(saved); setProjectDirty(false); notify('Project saved');
    });
  }, [document, run, notify]);
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
        setSummary(result.summary);setPreview(null);setInspection(null); setReport(result.timings);
        setDocument(current => current ? {...current,project:{...current.project,dmx:result.summary.path}} : current);
        if (document.project.dmx !== result.summary.path) setProjectDirty(true);
        setPage('appearance'); notify('Model analysis complete');
      } finally { try { const latest = await api.latestTimings(); if (latest) setReport(latest); } catch (cause) { fail('Could not retrieve the timing report', cause); } }
    }, true);
  }, [document, run, notify, fail]);
  const chooseModel = useCallback(async () => {
    if (active.current || !document) return;
    try {
      const path = await open({title:'Select replacement model',multiple:false,filters:[{name:'Models',extensions:['dmx','blend','glb','gltf','fbx']}]});
      if (typeof path === 'string') {if(/\.dmx$/i.test(path))await analyze(path);else{setPage('prepare');window.dispatchEvent(new CustomEvent('dmb-model-source',{detail:path}));}}
    } catch (cause) { fail('Could not choose a model', cause); }
  }, [analyze, document, fail]);
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
        if(payload.paths.length===1&&/\.(blend|glb|gltf|fbx)$/i.test(payload.paths[0])&&!active.current){setPage('prepare');window.dispatchEvent(new CustomEvent('dmb-model-source',{detail:payload.paths[0]}));}
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
    };
    window.addEventListener('keydown', keydown);
    return () => window.removeEventListener('keydown', keydown);
  }, [desktop, saveProject, openProject, newProject]);

  async function discoverComponents(extrasOnly=false){if(!document)return;const requested=document.project;await run('Character asset discovery did not complete',async()=>{const next=await (extrasOnly?api.discoverExtras(requested):api.discoverComponents(requested));if(documentRef.current?.project.id===requested.id&&documentRef.current.project.original_model===requested.original_model)setCatalog(next);notify('Character assets loaded.');},true);}
  async function saveOutfit(){if(!document)return;await run('Outfit could not be saved',async()=>{const path=await save({title:'Save outfit preset',defaultPath:`${document.project.name}.outfit.json`,filters:[{name:'Outfit preset',extensions:['json']}]});if(path){await api.outfitSave(document.project,path,document.project.name);notify('Outfit preset saved.');}});}
  async function loadOutfit(){if(!document)return;await run('Outfit could not be loaded',async()=>{const path=await open({title:'Load outfit preset',multiple:false,filters:[{name:'Outfit preset',extensions:['json']}]});if(typeof path==='string'){updateProject(await api.outfitLoad(document.project,path));setSummary(null);setPreview(null);notify('Outfit preset loaded. Review its choices.');}});}
  async function chooseFreeSlot(){if(!document)return;await run('Could not choose a free VPK slot',async()=>{const value=await api.chooseFreeSlot(document.project);updateProject({...document.project,vpk_number:value.slot});notify(`Selected available slot: ${value.filename}`);});}
  async function buildProject(overwriteOutput:boolean,overwriteInstall:boolean){if(!document)return;await run('Build did not complete',async()=>{setBuildResult(null);const value=await api.buildProject(document.project,overwriteOutput,overwriteInstall,document.path);setDocument({project:value.project,path:value.path});setProjectDirty(false);setBuildResult(value.result);notify(value.result.installed?'Verified VPK installed.':value.result.vpk?'Verified VPK published.':value.result.compiled?'Compilation completed; no VPK was requested.':'Source preparation completed; no compilation was requested.');},true);}
  async function installProject(selected:string|null,overwrite:boolean){if(!document)return;await run('Installation did not complete',async()=>{const path=selected??await open({title:'Select VPK to validate and install',multiple:false,filters:[{name:'VPK archive',extensions:['vpk']}]});if(typeof path!=='string')return;setInstalled(null);setInstalled(await api.installProject(document.project,path,overwrite));notify('VPK validated and installed.');},true);}
  async function inspectProject(){if(!document)return;await run('Project analysis did not complete',async()=>{
    setInspection(null);const value=await api.inspectProject(document.project);updateProject(value.project);setInspection({report:value.report,project:JSON.stringify(value.project)});if('dmx' in value.report)setSummary({...value.report.dmx,path:value.project.dmx});notify('needs_review' in value.report&&value.report.needs_review?'Review the analysis before building.':'Project analysis complete.');
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
  async function loadPreview(path=document?.project.dmx){if(!path)return;await run('Preview could not be loaded',async()=>{const buffer=await api.preview(path);setPreview(decodePreview(buffer));setPage('appearance');notify('Preview loaded');},true);}
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
  const runningStep=backendBusy?[...(report?.steps??[])].reverse().find(step=>step.status==='running')?.name:undefined;
  function navigate(page:Page, section?:string){setPage(page);if(section)requestAnimationFrame(()=>revealSection(section));}
  const unavailable = Boolean(busy) || backendBusy || !desktop;
  return <div className="app-shell">
    <aside className="sidebar"><div className="brand"><span className="brand-mark" aria-hidden="true">D</span><div>Deadlock<span>Mod Builder</span></div></div><nav aria-label="Main navigation">{[{label:'Project',pages:['project','appearance','physics','motion','extras','prepare']},{label:'Build',pages:['build','timings','logs']},{label:'System',pages:['settings']}].map(group=><div className="nav-group" key={group.label}><div className="nav-group-label">{group.label}</div>{navigation.filter(item=>group.pages.includes(item.id)).map(item=><button key={item.id} className={`nav-item ${item.id==='prepare'?'nav-utility':''} ${(page===item.id||((page==='setup'||page==='onboarding')&&item.id==='settings'))?'selected':''}`} aria-current={(page===item.id||((page==='setup'||page==='onboarding')&&item.id==='settings'))?'page':undefined} onClick={()=>setPage(item.id)}><NavIcon type={item.icon}/>{item.label}</button>)}</div>)}</nav><div className="sidebar-bottom"><span className="version">{APP_VERSION}</span><button className="tertiary about-link" onClick={()=>{setPage('settings');requestAnimationFrame(()=>{const details=window.document.getElementById('app-about');if(details instanceof HTMLDetailsElement)details.open=true;details?.scrollIntoView?.({block:'start'});});}}>About this build</button></div></aside>
    <div className="workspace-shell"><header className="topbar"><div className="project-identity"><span>{document?.project.name ?? 'Project workspace'}</span>{projectDirty && <span className="dirty-indicator" aria-label="Unsaved project changes">●</span>}<span className="muted">/ {page==='setup'?'Model tool setup':page==='onboarding'?'New-user setup':navigation.find(item => item.id === page)?.label}</span></div><div className="row project-controls"><button className="tertiary" disabled={unavailable} onClick={() => void newProject()}>New</button><button className="tertiary" disabled={unavailable} onClick={() => void openProject()}>Open…</button><button disabled={unavailable || !document} onClick={() => void saveProject()}>Save</button><button className="tertiary" disabled={unavailable || !document} onClick={() => void saveProject(true)}>Save as…</button></div></header>
      <main className={`workspace page-${page} ${['project','appearance','physics','motion','build','timings','logs','settings'].includes(page)?'studio-workspace':''}`}>
        {!desktop && <div className="capability-notice"><strong>Desktop connection required</strong><p>This browser cannot access the Rust backend. Run the Tauri desktop application to open projects and analyze models.</p></div>}
        {settingsDirty && !['settings','onboarding','setup'].includes(page) && <div className="capability-notice"><strong>Tool settings have unsaved changes</strong><p>Save settings to use the tool paths you selected.</p><button disabled={unavailable} onClick={()=>void persistSettings()}>Save settings</button></div>}
        {error && <ErrorNotice {...error} dismiss={() => setError(null)} />}
        {page === 'project' && (document ? <ProjectOverview document={document} report={inspection?.report??null} fresh={inspection?.project===JSON.stringify(document.project)} disabled={unavailable} update={updateProject} selectHero={hero=>void selectHero(hero)} fail={fail} go={navigate} loadOutfit={()=>void loadOutfit()} saveOutfit={()=>void saveOutfit()}/> : <><PageTitle title="Project overview" subtitle="Your character, configured choices, and the next step."/><div className="empty-state"><h2>{desktop?'Loading workspace…':'Open the desktop application'}</h2><p>Project data comes from the Rust backend. No sample project is loaded.</p></div></>)}
        {page === 'appearance' && <Appearance report={inspection?.report??null} fresh={inspection?.project===JSON.stringify(document?.project)} openPreparation={()=>setPage('prepare')} document={document} summary={summary} choose={() => void chooseModel()} analyze={path => void analyze(path)} busy={unavailable} drag={drag} update={updateProject} fail={fail} preview={preview} loadPreview={()=>void loadPreview()} importPackage={()=>void importSourcePackage()} analyzeMaterials={()=>void analyzeMaterials()} />}
        {document&&<div hidden={page!=='physics'}><PhysicsPage report={inspection?.report??null} fresh={inspection?.project===JSON.stringify(document.project)} key={document.project.id} project={document.project} update={updateProject} disabled={unavailable} fail={fail} summary={summary}><ClothAuthoringPanel project={document.project} update={updateProject} busy={unavailable} operate={run} onPreview={loadPreview}/></PhysicsPage></div>}
        {page==='motion'&&document&&<MotionPage catalog={catalog?.includes_animations===false?null:catalog} discover={()=>void discoverComponents()} key={document.project.id} project={document.project} update={updateProject} disabled={unavailable} fail={fail}/>}
        {page==='extras'&&document&&<ExtrasPage catalog={catalog} discover={()=>discoverComponents(true)} discovering={busy==='Character asset discovery did not complete'} canDiscover={Boolean(settings?.pak)&&!settingsDirty} openSetup={()=>setPage('settings')} reviewBuild={()=>navigate('build')} key={`${document.project.id}:${document.project.original_model}`} project={document.project} update={updateProject} disabled={unavailable} fail={fail}/>}
        {document&&<div hidden={page!=='prepare'}><PreparationPage key={document.project.id} project={document.project} update={updateProject} busy={unavailable} operate={run}/></div>}
        {page==='build'&&document&&<BuildPage analyzing={busy==='Project analysis did not complete'} go={navigate} project={document.project} update={updateProject} report={inspection?.report??null} fresh={inspection?.project===JSON.stringify(document.project)} prepared={prepared} original={originalValidation} busy={unavailable} inspect={()=>void inspectProject()} prepare={offline=>void prepareProject(offline)} validateOriginal={compile=>void validateOriginal(compile)} build={(output,install)=>void buildProject(output,install)} result={buildResult} install={(path,overwrite)=>void installProject(path,overwrite)} installed={installed} chooseFreeSlot={()=>void chooseFreeSlot()} openOutput={()=>void run('Output folder could not be opened',async()=>{await api.openOutput(document.project);})}/>}
        {page === 'timings' && <Timings report={report} openReport={() => void openReport()} busy={unavailable} />}
        {page === 'logs' && <><LogView entries={logs} copy={() => void copyLog()} save={() => void saveLog()} busy={unavailable} /><ToolLogs busy={!desktop} fail={fail}/></>}
        {page === 'settings' && <><SettingsPage onboarding={()=>setPage('onboarding')} modelSetup={()=>setPage('setup')} settings={settings} update={value => {setSettings(value);setSetupStatus(null);}} persist={() => void persistSettings()} busy={unavailable} dirty={settingsDirty} fail={fail} detect={extra=>void detectSettings(extra)} status={setupStatus} check={()=>void checkSetup()} />{settings&&desktop&&<StorageSection settings={settings} update={value=>setSettings(value)} project={document?.project??null} busy={unavailable} dirty={settingsDirty} operate={run}/>}{settings&&<Disclosure title="Tool diagnostics" hint="Compiler candidates, configuration audits and Blender validation"><ToolDiagnostics settings={settings} blenderSaved={path=>{setSettings(current=>current?{...current,blender:path}:current);setSavedSettings(current=>current?{...current,blender:path}:current);setSetupStatus(null);setInspection(null);}} disabled={unavailable} operate={run}/></Disclosure>}<Disclosure title="About this build" hint="Version and qualification status" id="app-about"><p>Deadlock Mod Builder Rust · {APP_VERSION}</p><p className="hint">The Vindicta R12 project has been reported working in game. This update recognizes MMD physics helpers during source import and retains the accepted cloth reference. Other projects and toolchains require their own validation.</p></Disclosure></>}
        {settings&&document&&<div hidden={page!=='onboarding'}><OnboardingPage active={page==='onboarding'} project={document.project} settings={settings} busy={unavailable} dirty={settingsDirty} status={setupStatus} updateSettings={value=>{setSettings(value);setSetupStatus(null);}} saveSettings={()=>void persistSettings()} verify={()=>void verifySetup()} detect={extra=>void detectSettings(extra)} check={()=>void checkSetup()} fail={fail} go={setPage} openProject={()=>void openProject()} chooseModel={()=>void chooseModel()} analyze={()=>{setPage('build');void inspectProject();}}/></div>}
        {settings&&document&&<div hidden={page!=='setup'}><button className="tertiary" disabled={unavailable} onClick={()=>setPage('settings')}>← Back to settings</button><SetupPage settings={settings} project={document.project} updateSettings={value=>{setSettings(value);setSetupStatus(null);}} busy={unavailable} operate={run}/></div>}
      </main><footer className={`statusbar ${busy||backendBusy?'':'statusbar-idle'}`}><span aria-live="polite">{busy||backendBusy?`● ${runningStep??'Working…'}`:desktop?'Ready':'Desktop backend unavailable'}</span><div className="row">{settingsDirty && <span className="muted">Settings unsaved</span>}{((busy&&cancellable)||(!busy&&backendBusy)) && <button onClick={() => void cancel()}>Cancel</button>}</div></footer>
    </div>{toast && <div className="toast" role="status">{toast}</div>}
  </div>;
}
