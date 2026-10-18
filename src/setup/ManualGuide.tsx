import {useState} from 'react';
import {Field} from '../components/Primitives';
import {PrepIcon} from '../preparation/PreparationChrome';
import type {ManualPreview} from './models';

const gb=(bytes:number)=>`${(bytes/2**30).toFixed(1)} GB`;
const ACCOUNT=/^[A-Za-z0-9_]{2,64}$/;

/** One command to paste, with a Copy button. */
export function CommandBlock({text,label}:{text:string;label:string}){
  const [copied,setCopied]=useState(false);
  const copy=()=>{void navigator.clipboard?.writeText(text).then(()=>setCopied(true)).catch(()=>setCopied(false));};
  // Each word wraps as a whole, so a line never breaks inside a flag such as -osarch.
  return <div className="command-block"><code aria-label={label}>{text.split(' ').flatMap((word,index)=>[index?' ':'',<span key={index}>{word}</span>])}</code><button type="button" className="tertiary" onClick={copy}>{copied?'Copied':'Copy'}</button></div>;
}

/**
 * The manual toolset download as steps on the page: the user picks a sign-in,
 * copies each command, and sees why the download commands only appear after
 * Export exact file plan. The plain-text version (manual_setup.rs `guide`,
 * saved as instructions.txt) has the same steps; keep the two in step.
 */
export function ManualGuide({preview}:{preview:ManualPreview}){
  const [mode,setMode]=useState<'qr'|'username'>('qr'),[account,setAccount]=useState('');
  const commands=preview.argv[mode];
  const name=ACCOUNT.test(account.trim())?account.trim():'';
  const fill=(text:string)=>name?text.split('YOUR_STEAM_USERNAME').join(name):text;
  const downloads=commands?.download_commands??[];
  const depots=(commands?.downloads??[]).map(argv=>{const rest=argv.slice(argv.indexOf('-depot')+1);const end=rest.findIndex(word=>word.startsWith('-'));return (end<0?rest:rest.slice(0,end)).join(', ');});
  const partLabel=(index:number)=>downloads.length===1?`One command for depots ${depots[0]}`:`Part ${index+1} of ${downloads.length} · depot ${depots[index]}`;
  const signIn=mode==='qr'
    ?<>A QR code appears in the PowerShell window. Scan it with the Steam app on your phone (Steam Guard → <strong>Scan a QR code</strong>). Every command shows a new code.</>
    :<>DepotDownloader asks for your password and Steam Guard code in the PowerShell window. Type them there; the app never sees them, and the sign-in is kept for the next commands.</>;
  return <div className="manual-guide">
    <p className="hint">Run the download yourself in PowerShell, where you see DepotDownloader’s own sign-in. Nothing has run yet.</p>
    <div className="manual-signin"><span className="segmented" role="radiogroup" aria-label="Steam sign-in">{([['qr','QR code'],['username','Steam username']] as const).map(([value,label])=><button key={value} type="button" role="radio" aria-checked={mode===value} onClick={()=>setMode(value)}>{label}</button>)}</span>
      {mode==='username'&&<Field label="Steam account name" value={account} onChange={setAccount} hint="Optional: fills it into the commands below. It is not saved."/>}</div>
    <ol className="manual-steps">
      <li><h4>Open PowerShell</h4><p>Start menu → type <strong>PowerShell</strong> → Enter.</p></li>
      <li><h4>Get the file lists</h4><p>Paste this command into PowerShell and press Enter. It only downloads lists of file names (under 1 MB), not the tools.</p>
        {commands&&<CommandBlock label="Command to get the file lists" text={fill(commands.metadata_command)}/>}<p className="hint">{signIn}</p></li>
      <li><h4>Make your download plan</h4><p>Above, set <strong>Manifest listings folder</strong> to the folder after <code>-dir</code> in step 2, <strong>Existing toolset folder</strong> to where the tools should go (it can be empty) and <strong>Manual plan export folder</strong> to any empty folder. Then click <strong>Export exact file plan</strong>.</p>
        {preview.export_dir&&<p className="prep-badge-line" role="status"><span className="prep-badge"><PrepIcon name="check" size={14}/>Plan made</span> Saved in <code className="path">{preview.export_dir}</code></p>}</li>
      <li><h4>Download the tools</h4>{downloads.length
        ?<><p>{downloads.length===1?'Run this command and let it finish':'Run these one after another and let each one finish'}{preview.selected_bytes!=null?` (${gb(preview.selected_bytes)} in total)`:''}. If a command stops with an error, run it again; it continues where it stopped. Never remove <code>-filelist</code>: without it the whole depot downloads.</p>
          {downloads.map((text,index)=><div key={index} className="manual-part"><span>{partLabel(index)}</span><CommandBlock label={`Download command ${index+1}`} text={fill(text)}/></div>)}</>
        :<div className="manual-later"><strong>Your download commands appear here after step 3.</strong><p>Each one needs a file list that picks only the files the model compiler needs: a few GB instead of all {gb(preview.whole_bytes)} of the three Counter-Strike 2 depots. The app makes those lists from the files you get in step 2, so it cannot write these commands before that.</p></div>}</li>
      <li><h4>Check and use the toolset</h4><p>Click <strong>Verify file integrity</strong> above, then <strong>Use this toolset in settings</strong> at the bottom of the page.</p></li>
    </ol>
    <details className="technical"><summary>If something goes wrong</summary><ul className="manual-problems">
      <li><strong>“Failed to authenticate with Steam” or “InitializeSteam failed”:</strong> the QR code was not scanned in time, or Steam refused the sign-in. Run the command again and scan the newest code. The account needs Counter-Strike 2 in its library (free).</li>
      <li><strong>“Access token was rejected”:</strong> the saved sign-in expired. Run the step 2 command with your username again.</li>
      <li><strong>“Unable to locate manifest”:</strong> this Steam account has no Counter-Strike 2. Add it to the library; do not change the manifest numbers.</li>
      <li><strong>Verify file integrity finds missing files:</strong> a download command did not finish. Run it again, then verify again.</li>
    </ul></details>
    <details className="technical"><summary>Plain-text version{preview.export_dir?' (saved as instructions.txt in the plan folder)':''}</summary><pre>{preview.guide}</pre></details>
  </div>;
}
