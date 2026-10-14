import { useEffect, useId, useState, type KeyboardEventHandler } from 'react';
import { basename } from '../api';

export function PathValue({ path, empty = 'Not selected' }: { path?: string | null; empty?: string }) {
  const [copied, setCopied] = useState(false), [copyError, setCopyError] = useState(false);
  useEffect(() => { setCopied(false); setCopyError(false); }, [path]);
  if (!path) return <span className="muted">{empty}</span>;
  async function copy() {
    try { await navigator.clipboard.writeText(path!); setCopied(true); setCopyError(false); }
    catch { setCopyError(true); }
  }
  return <details className="path-value"><summary title={path}>{basename(path)}</summary><div className="path-expanded"><code className="path">{path}</code><button className="tertiary" onClick={() => void copy()}>{copied ? 'Copied' : 'Copy path'}</button>{copyError && <p className="hint" role="status">Copy unavailable. Select the full path above to copy it.</p>}</div></details>;
}

/** Presentation only. Callers retain their existing path validation and commit rules. */
export function CompactPathField({ label, value, committed = value, update, browse, disabled, placeholder, onBlur, onKeyDown, checking = false, folder = false, fullPath = false }: {
  label: string; value: string; committed?: string; update: (value: string) => void;
  browse: () => void | Promise<void>; disabled: boolean; placeholder?: string;
  onBlur?: () => void; onKeyDown?: KeyboardEventHandler<HTMLInputElement>; checking?: boolean; folder?: boolean;
  /** Show the whole path once chosen (Settings: the folder matters, not just the file name). */
  fullPath?: boolean;
}) {
  const id = useId(), [expanded, setExpanded] = useState(false);
  const selected = Boolean(committed);
  async function choose() { await browse(); setExpanded(false); }
  if (folder && !selected) return <div className="path-control folder-path-control">
    <label htmlFor={id}>{label}</label><div className="folder-picker"><div><strong>Drop a folder here</strong><p>No texture folder selected</p></div><button disabled={disabled} aria-label={`Browse ${label}`} onClick={()=>void choose()}>{checking?'Checking…':'Browse…'}</button></div>
    <details className="technical"><summary>Enter folder path</summary><div className="path-editor"><input id={id} value={value} disabled={disabled} placeholder={placeholder} onChange={event=>update(event.target.value)} onBlur={onBlur} onKeyDown={onKeyDown}/></div></details>
  </div>;
  return <div className="path-control">
    <label htmlFor={id} onClick={() => setExpanded(true)}>{label}</label>
    {selected && <div className="selected-file"><span className={fullPath ? "file-name full-path" : "file-name"} title={committed}>{fullPath ? committed : basename(committed)}</span><div className="row"><button className="tertiary" disabled={disabled} aria-expanded={expanded} aria-controls={`${id}-editor`} onClick={() => setExpanded(!expanded)}>{expanded ? 'Hide path' : 'Edit path'}</button><button disabled={disabled} aria-label={`Browse ${label}`} onClick={() => void choose()}>{checking ? 'Checking…' : folder ? 'Change folder' : 'Replace…'}</button></div></div>}
    <div id={`${id}-editor`} className="path-editor" hidden={selected && !expanded}>
      <div className="row"><input id={id} value={value} disabled={disabled} placeholder={placeholder} onFocus={() => setExpanded(true)} onChange={event => update(event.target.value)} onBlur={onBlur} onKeyDown={onKeyDown} />{!selected && <button disabled={disabled} aria-label={`Browse ${label}`} onClick={() => void choose()}>{checking ? 'Checking…' : 'Browse…'}</button>}</div>
      {selected && <p className="hint">Full path — select to copy, or edit to change the source.</p>}
    </div>
  </div>;
}
