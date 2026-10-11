import { useState } from 'react';
import { openUrl } from '@tauri-apps/plugin-opener';
import { toolLinks, type ToolLinkId } from './toolLinks';

export function ToolLink({ id, disabled = false }: { id: ToolLinkId; disabled?: boolean }) {
  const tool = toolLinks[id];
  const [error, setError] = useState(false), [copied, setCopied] = useState(false);
  async function launch() {
    if (disabled) return;
    setError(false);
    try { await openUrl(tool.url); } catch { setError(true); }
  }
  return <span className="tool-link">
    <a href={tool.url} target="_blank" rel="noopener noreferrer" title={`${tool.action} — opens in your browser`} aria-disabled={disabled || undefined} tabIndex={disabled ? -1 : undefined} onClick={event => { event.preventDefault(); void launch(); }}>{tool.action}<span aria-hidden="true"> ↗</span></a>
    {error && <span className="tool-link-error" role="alert">Couldn’t open your browser. Open this address manually: <code className="path">{tool.url}</code><button className="tertiary" onClick={() => { void navigator.clipboard.writeText(tool.url).then(() => setCopied(true)).catch(() => setCopied(false)); }}>{copied ? 'Copied' : 'Copy link'}</button></span>}
  </span>;
}

export function ToolDownloads({ busy, expanded = false, modelSetup }: { busy: boolean; expanded?: boolean; modelSetup?: () => void }) {
  const row = (id: ToolLinkId) => <div className="tool-download-row" key={id}><div><div className="tool-download-title"><strong>{toolLinks[id].name}</strong><span className="tool-requirement">{toolLinks[id].requirement}</span></div><p>{toolLinks[id].help}</p></div><ToolLink id={id} disabled={busy}/></div>;
  return <details className="disclosure tool-downloads" open={expanded || undefined}>
    <summary>Get programs and download links</summary>
    <div className="disclosure-content">
      <p className="hint">Links open the publisher’s site or the CSDK community guide in your browser. Install or extract each tool, then select its location below.</p>
      {(['steam', 'deadlock', 'source2viewer', 'csdk'] as const).map(row)}
      <div className="tool-download-row"><div><div className="tool-download-title"><strong>Model compiler toolset</strong><span className="tool-requirement">Required for replacement models</span></div><p>Keep your working toolset, or use the guided setup to acquire and verify the selected tool files.</p></div>{modelSetup ? <button disabled={busy} onClick={modelSetup}>Set up model compiler…</button> : <span className="hint">Available in Settings</span>}</div>
      <details className="technical"><summary>Model-tool downloads, Blender and Windows runtimes</summary>{(['depotdownloader', 'cs2', 'blender', 'vcredist', 'webview2'] as const).map(row)}</details>
      <p className="hint">VPK packaging and the independent cloth reader are included with the app. No separate Python or .NET installation is needed for them.</p>
    </div>
  </details>;
}
