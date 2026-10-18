import {useEffect,useState} from 'react';
import {listen,type UnlistenFn} from '@tauri-apps/api/event';

/** A Steam sign-in QR code from DepotDownloader: one string per row, "1" dark, without the quiet zone. */
export type SteamQrCode={rows:string[]};
const QUIET=4,MODULE_PX=7;

/** The SVG path of the dark modules, offset by the quiet zone. */
export function qrPath(rows:string[]):string{
  let path='';
  rows.forEach((row,y)=>{for(let x=0;x<row.length;x++)if(row[x]==='1')path+=`M${x+QUIET} ${y+QUIET}h1v1h-1z`;});
  return path;
}

/**
 * Shows DepotDownloader's Steam QR code on screen while it waits for a scan
 * (the backend sends `steam-qr` with the code, then null when the sign-in is
 * over). DepotDownloader runs without a console, so this is the only place
 * the code can be read.
 */
export function SteamQrDialog(){
  const [code,setCode]=useState<SteamQrCode|null>(null),[hidden,setHidden]=useState(false);
  useEffect(()=>{
    let disposed=false,release:UnlistenFn|undefined;
    const receive=(next:SteamQrCode|null)=>{setCode(next);if(!next)setHidden(false);};
    try{void listen<SteamQrCode|null>('steam-qr',event=>receive(event.payload??null)).then(unlisten=>{if(disposed)unlisten();else release=unlisten;}).catch(()=>{});}
    catch{/* No desktop bridge (browser preview). */}
    return()=>{disposed=true;release?.();};
  },[]);
  if(!code||hidden||!code.rows.length)return null;
  const total=code.rows.length+QUIET*2;
  return <div className="qr-backdrop"><div className="qr-dialog" role="dialog" aria-modal="true" aria-labelledby="steam-qr-title">
    <h2 id="steam-qr-title">Scan to sign in to Steam</h2>
    <p>On your phone, open the Steam app, tap the Steam Guard shield and choose <strong>Scan a QR code</strong>. Approve the sign-in; the download tool continues by itself.</p>
    <svg role="img" aria-label="Steam sign-in QR code" viewBox={`0 0 ${total} ${total}`} width={total*MODULE_PX} height={total*MODULE_PX} shapeRendering="crispEdges"><rect width={total} height={total} fill="#fff"/><path d={qrPath(code.rows)} fill="#000"/></svg>
    <p className="hint">Steam renews the code every few seconds; this one is always the newest. Steam asks once to fetch the manifests and once more to download.</p>
    <div className="row"><button onClick={()=>setHidden(true)}>Hide until the next sign-in</button></div>
  </div></div>;
}
