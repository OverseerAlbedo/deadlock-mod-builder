import {useEffect,useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import {open} from '@tauri-apps/plugin-dialog';
import type {Project,Settings} from '../models';
import {PageTitle,Section,Field} from '../components/Primitives';
import {SelectField} from '../components/CustomizationPages';
import {CompactPathField} from '../components/Paths';
import {PrepIcon} from '../preparation/PreparationChrome';
import {ToolLink} from './ToolDownloads';
import {ManualGuide} from './ManualGuide';
import type {AcquireAction,AcquireAuth,AcquisitionResult,IntegrityReceipt,ManualPreview,SetupDefaults,SetupOptions,ValidationReport} from './models';
import '../preparation/preparation.css';

type Props={settings:Settings;project:Project;updateSettings:(settings:Settings)=>void;busy:boolean;operate:(label:string,fn:()=>Promise<void>,cancel?:boolean)=>Promise<unknown>;
  /** Saves settings right away (the toolset is useless until saved). */
  saveSettings?:(settings:Settings)=>Promise<void>};
const bytes=(n:number)=>n<1024?`${n} B`:`${(n/1024**(Math.min(4,Math.floor(Math.log(n)/Math.log(1024))))).toFixed(2)} ${['B','KiB','MiB','GiB','TiB'][Math.min(4,Math.floor(Math.log(n)/Math.log(1024)))]}`;
function joined(root:string,child:string){return `${root.replace(/[\\/]+$/,'')}${root.includes('\\')?'\\':'/'}${child}`;}
function SetupPath({label,value,onChange,directory=true,busy,operate,example}: {label:string;value:string;onChange:(v:string)=>void;directory?:boolean;busy:boolean;operate:Props['operate'];example?:string}){
  return <CompactPathField fullPath label={label} value={value} update={onChange} disabled={busy} placeholder={example?`Example: ${example}`:undefined} browse={async()=>{await operate(`Choose ${label.toLowerCase()}`,async()=>{const path=await open({title:label,directory,multiple:false});if(typeof path==='string')onChange(path);});}}/>;
}
/** What is wrong with a DepotDownloader choice, before running anything. */
export function depotDownloaderHint(path:string):string|null{
  if(!path.trim())return 'Select DepotDownloader.exe from the extracted DepotDownloader-windows-x64 folder.';
  if(/\.zip$/i.test(path))return 'This is the ZIP. Right-click it → Extract All, then select DepotDownloader.exe inside the extracted folder.';
  if(!/\.exe$/i.test(path))return 'A folder works if DepotDownloader.exe is inside it; the app finds it.';
  if(!/depotdownloader\.exe$/i.test(path))return 'This is not DepotDownloader.exe.';
  return null;
}
/** The four stages of the page; all stay visible, the header scrolls to them. */
const STAGES:[string,string,string][]=[['setup-acquire','Get the files','Download with your Steam account'],['setup-existing','Or use existing files','A toolset you already have'],['setup-test','Test with your hero','Compile the original once'],['setup-use','Use it','Select the toolset in Settings']];
const jump=(id:string)=>window.document.getElementById(id)?.scrollIntoView({behavior:'smooth',block:'start'});

export function SetupPage({settings,project,updateSettings,busy,operate,saveSettings}:Props){
  const [defaults,setDefaults]=useState<SetupDefaults|null>(null),[loadError,setLoadError]=useState('');
  const [options,setOptions]=useState<SetupOptions>({dd:'',toolroot:'',manifests:''});
  const [root,setRoot]=useState(''),[dd,setDd]=useState(''),[exportRoot,setExportRoot]=useState('');
  const [authMode,setAuthMode]=useState<'qr'|'username'>('qr'),[username,setUsername]=useState(''),[signedIn,setSignedIn]=useState('');
  const [acquisition,setAcquisition]=useState<AcquisitionResult|null>(null),[preview,setPreview]=useState<ManualPreview|null>(null);
  const [integrity,setIntegrity]=useState<IntegrityReceipt|null>(null),[validation,setValidation]=useState<ValidationReport|null>(null),[note,setNote]=useState('');
  useEffect(()=>{let active=true;void invoke<SetupDefaults>('setup_defaults').then(value=>{if(active){setDefaults(value);setOptions(value.options);setRoot(value.automatic_root);setDd(value.options.dd);setExportRoot(value.export_root);}}).catch(error=>{if(active)setLoadError(typeof error==='object'&&error&&'message'in error?String(error.message):String(error));});return()=>{active=false;};},[]);
  useEffect(()=>setValidation(null),[project.original_model,settings.deadlock,settings.pak,settings.workspace,settings.source2viewer]);
  const disabled=busy||!defaults;
  const resetAutomatic=(change:()=>void)=>{change();setAcquisition(null);setNote('');};
  const editOptions=(change:Partial<SetupOptions>)=>{setOptions({...options,...change});setPreview(null);setIntegrity(null);setValidation(null);setNote('');};
  const auth:AcquireAuth=authMode==='qr'?{mode:'qr'}:{mode:'username',username};
  /** Write the toolset into Settings and save, so builds use it at once. */
  const apply=async(toolroot:string,automatic:boolean)=>{
    const next=await invoke<Settings>('setup_use_toolset',{settings,toolroot});updateSettings(next);
    if(saveSettings){await saveSettings(next);setNote(automatic?'Toolset verified, selected and saved. Builds now use this model compiler; you can test it with your hero below.':'Model compiler selected and saved. Builds now use this toolset.');}
    else setNote('Model tool paths selected. Save settings before running a build.');
  };
  const acquire=(action:AcquireAction)=>operate(action==='manifests'?'Read pinned tool manifests':action==='download'?'Download selected model tools':'Verify downloaded tool files',async()=>{
    setNote('');setIntegrity(null);setValidation(null);setAcquisition(previous=>action==='manifests'?null:previous?{...previous,receipt:null}:null);const result=await invoke<AcquisitionResult>('setup_acquire',{action,root,dd:dd||null,auth,approvedPlanDigest:action==='download'?acquisition?.plan_digest??null:null});
    const toolroot=result.plan.destination??joined(root,'toolset');
    setAcquisition(result);setOptions({dd,toolroot,manifests:joined(root,'manifests')});setPreview(null);setIntegrity(result.receipt);setValidation(null);
    // The step people miss: a verified download is applied right away.
    if(action!=='manifests'&&result.receipt?.manifest_integrity_passed)await apply(toolroot,true);
  },true);
  const select=()=>operate('Select model toolset',async()=>{await apply(options.toolroot,false);});
  const ddHint=depotDownloaderHint(dd);
  const trim=(path:string)=>path.replace(/[\\/]+$/,'').toLowerCase();
  const selectedHere=Boolean(options.toolroot)&&trim(settings.cswin)===trim(options.toolroot);
  const pathProps={busy:disabled,operate};
  // Which stage the user has reached, for the header.
  const stage=validation?.compatible||note?3:integrity?.manifest_integrity_passed||acquisition?.downloaded?2:acquisition||preview||options.toolroot?1:0;
  const toolsetKnown=Boolean(options.toolroot);
  return <div className="prep-workspace setup-workspace">
    <PageTitle eyebrow="Model compiler" title="Model tool setup" subtitle="The model compiler is a pinned set of Source 2 tool files. Get them, check them, test them on the original hero once, then use them in Settings."/>
    <nav className="prep-steps" aria-label="Model tool setup stages">
      {STAGES.map(([id,label,hint],index)=><span key={id} className="prep-step-slot">{index>0&&<i/>}<button type="button" aria-label={`${index+1}. ${label}`} aria-current={stage===index?'step':undefined} onClick={()=>jump(id)}><b>{stage>index?<PrepIcon name="check" size={14}/>:index+1}</b><span>{label}<small>{hint}</small></span></button></span>)}
    </nav>
    {loadError&&<p className="prep-card" role="alert">Setup defaults could not be loaded: {loadError}</p>}
    {!defaults&&!loadError&&<p className="quiet-empty">Loading the installed setup recipe…</p>}
    {note&&<div className="prep-card prep-done" role="status"><PrepIcon name="check"/><div><strong>{note}</strong></div></div>}
    {!note&&integrity?.manifest_integrity_passed&&!selectedHere&&<div className="prep-card setup-last-step" role="alert"><div><strong>Last step: use this toolset</strong><p>The files are verified, but Settings still uses {settings.cswin?'another model compiler':'no model compiler'}. Builds fail with a schema mismatch until you select it.</p></div><button className="prep-primary" disabled={disabled} onClick={()=>void select()}>Use this toolset in settings</button></div>}
    <Section title="Get the files automatically" className="prep-card" id="setup-acquire">
      <p className="prep-section-intro">Uses DepotDownloader with your own Steam account. The first action only reads the pinned manifests and shows the exact file plan; nothing is downloaded until you approve it.</p>
      <p className="hint"><ToolLink id="depotdownloader" disabled={busy}/> — extract the Windows x64 ZIP and select its executable below. Your Steam account needs access to Counter-Strike 2; a full installation is optional. <ToolLink id="cs2" disabled={busy}/></p>
      <div className="settings-fields"><SetupPath {...pathProps} label="DepotDownloader executable" directory={false} example={String.raw`C:\Tools\DepotDownloader\DepotDownloader.exe`} value={dd} onChange={value=>resetAutomatic(()=>setDd(value))}/>{ddHint&&<p className="field-check field-check-warning">{ddHint}</p>}<SetupPath {...pathProps} label="Acquisition folder" example={String.raw`C:\DeadlockModdingTools\DMBTools-1`} value={root} onChange={value=>resetAutomatic(()=>setRoot(value))}/></div>
      <ol className="setup-list"><li>Click <strong>Fetch manifests and plan</strong>. A QR code appears on screen: scan it with the Steam app on your phone.</li><li>Check the plan, then click <strong>Download</strong> (scan one more QR code).</li><li>When the files are verified, the app selects and saves the toolset for you.</li></ol>
      <SelectField label="Steam authentication" value={authMode} values={[["qr","Steam mobile QR sign-in"],["username","Locally authenticated Steam account"]]} onChange={value=>resetAutomatic(()=>{setAuthMode(value as 'qr'|'username');setSignedIn('');})} disabled={disabled}/>
      {authMode==='username'?<><Field label="Steam username" value={username} onChange={value=>resetAutomatic(()=>{setUsername(value);setSignedIn('');})} disabled={disabled} hint="The account name you sign in to Steam with, not your profile name."/>
        <p className="hint">DepotDownloader runs in the background without a window, so it needs a saved sign-in first. <strong>Sign in to Steam…</strong> opens it once in its own window: type your password and Steam Guard code there. The builder never sees them.</p>
        <div className="row"><button className="prep-secondary" disabled={disabled||!dd||!username.trim()} onClick={()=>void operate('Sign in to Steam',async()=>{setSignedIn('');await invoke('setup_steam_sign_in',{dd:dd||null,username:username.trim()});setSignedIn(username.trim());},true)}>Sign in to Steam…</button></div>
        {signedIn&&<p role="status" className="prep-badge-line"><span className="prep-badge"><PrepIcon name="check" size={14}/>Signed in</span> DepotDownloader saved the sign-in for {signedIn}. Click Fetch manifests and plan.</p>}</>
        :<p className="hint">When Steam asks you to sign in, the QR code appears on screen. Scan it with the Steam app on your phone (Steam Guard → Scan a QR code) and approve.</p>}
      <div className="row"><button className="prep-primary" disabled={disabled||!root||!dd||(authMode==='username'&&!username.trim())} onClick={()=>void acquire('manifests')}>Fetch manifests and plan</button><button className="prep-secondary" disabled={disabled||!root} onClick={()=>void acquire('verify')}>Verify existing acquisition</button></div>
      {acquisition&&<div className="editor-fields setup-plan"><h3>Build {acquisition.plan.steam_build}</h3><dl className="summary-list"><dt>Final selected files</dt><dd>{acquisition.plan.files.length.toLocaleString()}</dd><dt>Installed size</dt><dd>{bytes(acquisition.plan.installed_bytes)}</dd><dt>Uncompressed transfer</dt><dd>{bytes(acquisition.plan.transfer_uncompressed_bytes)}</dd><dt>Destination</dt><dd className="path">{acquisition.plan.destination??joined(root,'toolset')}</dd></dl>
        <p className="hint">{acquisition.plan.download_estimate_note??'Network transfer can differ from uncompressed file sizes.'}</p>
        <table><thead><tr><th>Depot</th><th>Files</th><th>Selected size</th></tr></thead><tbody>{acquisition.plan.depots.map(depot=><tr key={depot.depot}><td>{depot.depot}</td><td>{depot.selected_files.toLocaleString()}</td><td>{bytes(depot.selected_bytes)}</td></tr>)}</tbody></table>
        <details className="technical"><summary>Selected files and plan identity</summary><code className="path">{acquisition.plan_digest}</code><p className="hint">{acquisition.plan.scope}</p><pre>{acquisition.plan.files.slice(0,200).map(file=>`${file.path}  ${bytes(file.bytes)}  ${file.sha1}`).join('\n')}</pre>{acquisition.plan.files.length>200&&<p className="hint">Showing the first 200 entries. Export the manual plan below for the complete selected-file list and hashes.</p>}</details>
        <button className="prep-primary" disabled={disabled||!dd||acquisition.downloaded} onClick={()=>void acquire('download')}>Download these {acquisition.plan.files.length.toLocaleString()} files</button>
        {acquisition.receipt?.manifest_integrity_passed&&<p role="status" className="prep-badge-line"><span className="prep-badge"><PrepIcon name="check" size={14}/>Files verified</span> Selected file integrity passed. Compiler compatibility is checked in stage 3.</p>}
      </div>}
    </Section>
    <Section title="Or use files you already have" className="prep-card" id="setup-existing">
      <p className="prep-section-intro">Already have the toolset files, or want to download them yourself in PowerShell? Click Show manual download steps for a step-by-step guide. Nothing here downloads anything.</p>
      <div className="settings-fields"><SetupPath {...pathProps} label="Manual DepotDownloader executable" directory={false} value={options.dd} onChange={dd=>editOptions({dd})}/><SetupPath {...pathProps} label="Existing toolset folder" value={options.toolroot} onChange={toolroot=>editOptions({toolroot})}/><SetupPath {...pathProps} label="Manifest listings folder" value={options.manifests} onChange={manifests=>editOptions({manifests})}/><SetupPath {...pathProps} label="Manual plan export folder" value={exportRoot} onChange={value=>{setExportRoot(value);setPreview(null);}}/></div>
      <div className="row"><button className="prep-primary" disabled={disabled||!options.toolroot||!options.manifests} onClick={()=>void operate('Verify selected tool files',async()=>{setIntegrity(null);setValidation(null);setIntegrity(await invoke<IntegrityReceipt>('setup_verify_files',{options}));},true)}>Verify file integrity</button><button className="prep-secondary" disabled={disabled||!options.dd||!options.manifests||!options.toolroot} onClick={()=>void operate('Preview manual setup',async()=>{setPreview(await invoke<ManualPreview>('setup_manual_preview',{options}));})}>Show manual download steps</button><button className="prep-secondary" disabled={disabled||!exportRoot||!options.manifests||!options.toolroot} onClick={()=>void operate('Export exact manual setup plan',async()=>{setPreview(await invoke<ManualPreview>('setup_manual_export',{options,exportRoot}));},true)}>Export exact file plan</button></div>
      {preview&&<div className="editor-fields"><ManualGuide preview={preview}/>{preview.expected_files&&<details><summary>Expected files and hashes</summary><pre>{preview.expected_files.split('\n').slice(0,201).join('\n')}</pre><p className="hint">The complete list is in expected-files.tsv in the exported plan.</p></details>}</div>}
      {integrity&&<p role="status" className="prep-badge-line">{integrity.manifest_integrity_passed&&<span className="prep-badge"><PrepIcon name="check" size={14}/>Verified</span>}<span>{`${integrity.manifest_integrity_passed?'Integrity passed':'Integrity not established'}: ${integrity.files_verified.toLocaleString()} files, ${bytes(integrity.bytes_verified)}. This checks local manifest hashes; compiler compatibility is checked in stage 3.`}</span></p>}
    </Section>
    <Section title="Test the tools with your hero" className="prep-card" id="setup-test">
      <p className="prep-section-intro">Compiles the original hero once with the selected toolset in a private copy and checks ModelDoc 41, the gameplay and UI animation graphs and the skeleton. Uses the saved game, workspace and Source 2 Viewer settings; allow disk space for the copy.</p>
      <dl className="summary-list"><dt>Selected original</dt><dd><code className="path">{project.original_model||'Choose a hero in Character & model first.'}</code></dd><dt>Toolset</dt><dd><code className="path">{options.toolroot||'None selected yet'}</code></dd></dl>
      <div className="row"><button className="prep-primary" disabled={disabled||!project.original_model||!toolsetKnown||!options.manifests} onClick={()=>void operate('Test original hero with existing tools',async()=>{setValidation(null);setValidation(await invoke<ValidationReport>('setup_validate_existing',{project,options}));},true)}>Test original hero compatibility</button></div>
      {validation&&<div className="editor-fields"><p role="status" className="prep-badge-line">{validation.compatible?<span className="prep-badge"><PrepIcon name="check" size={14}/>Original hero compatibility passed</span>:<strong>Original hero compatibility not established</strong>}</p>{validation.checks&&<dl className="summary-list">{Object.entries(validation.checks).map(([name,passed])=><div key={name}><dt>{({native_modeldoc41:'Native ModelDoc 41',original_compiled:'Original hero compile',gameplay_animgraph2:'Gameplay animation graph',ui_animgraph2:'UI animation graph',nm_skeleton:'Skeleton reference'} as Record<string,string>)[name]}</dt><dd>{passed?'Passed':'Not established'}</dd></div>)}</dl>}<p className="hint">{validation.scope}</p><p className="hint">Private validation copy {validation.validation_copy_removed?'removed':'retained; review cleanup details'}. Report: <code className="path">{validation.report_directory}</code></p>{(validation.error||validation.cleanup_error)&&<pre>{[validation.error,validation.cleanup_error].filter(Boolean).join('\n')}</pre>}</div>}
    </Section>
    <Section title="Use this toolset" className="prep-card" id="setup-use">
      <p className="prep-section-intro">Writes the toolset's paths into Settings (model compiler toolset, executable and game folder){saveSettings?' and saves them':'; save settings afterwards'}. Every build still validates the tools it runs.</p>
      {selectedHere&&<p className="prep-badge-line"><span className="prep-badge"><PrepIcon name="check" size={14}/>In use</span> Settings use this toolset.</p>}
      <div className="row"><button className="prep-primary" disabled={disabled||!toolsetKnown} onClick={()=>void select()}>Use this toolset in settings</button></div>
    </Section>
  </div>;
}
