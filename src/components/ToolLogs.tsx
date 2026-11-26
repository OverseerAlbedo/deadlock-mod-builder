import { useState } from 'react';
import { api, basename, type ToolLogFile, type ToolLogPage } from '../api';
import { Section } from './Primitives';
import { m, tp, useT } from '../i18n';
// i18n-area: common
export function ToolLogs({busy,fail}:{busy:boolean;fail:(title:string,error:unknown)=>void}){
  const t=useT();
  const [files,setFiles]=useState<ToolLogFile[]|null>(null),[file,setFile]=useState<ToolLogFile|null>(null),[page,setPage]=useState<ToolLogPage|null>(null),[offset,setOffset]=useState(0),[reading,setReading]=useState(false);
  async function refresh(){setReading(true);try{setFiles(await api.toolLogs());}catch(error){fail(m('Tool log list could not be read'),error);}finally{setReading(false);}}
  async function read(selected:ToolLogFile,next=0){setReading(true);try{const value=await api.toolLog(selected.path,next);setFile(selected);setOffset(next);setPage(value);}catch(error){fail(m('Tool output could not be read'),error);}finally{setReading(false);}}
  return <Section title={t('Complete tool output')} actions={<button disabled={busy||reading} onClick={()=>void refresh()}>{t('List tool logs')}</button>}><p className="hint">{t('Compiler and helper output is retained on disk. Read it in bounded pages here, or save the complete combined log above.')}</p>{files!==null&&(files.length?<div className="tool-log-files">{files.map(entry=><button key={entry.path} disabled={reading} title={entry.path} onClick={()=>void read(entry)}>{entry.kind} · {basename(entry.path)} <span className="muted">{tp(entry.bytes,{other:'{total} bytes'},{total:entry.bytes.toLocaleString()})}</span></button>)}</div>:<p className="quiet-empty">{t('No tool log files have been recorded.')}</p>)}{file&&page&&<><p className="path mono">{file.path}</p><div className="row"><button disabled={reading||offset===0} onClick={()=>void read(file)}>{t('Start')}</button><span className="hint">{t('Bytes {from}–{to}',{from:offset.toLocaleString(),to:page.next_offset.toLocaleString()})}</span><button disabled={reading||page.eof} onClick={()=>void read(file,page.next_offset)}>{t('Next page')}</button><button disabled={reading} onClick={()=>void read(file,offset)}>{t('Refresh page')}</button></div><pre className="raw-tool-log" tabIndex={0}>{page.text||t('No output in this page.')}</pre></>}</Section>;
}
