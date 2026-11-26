// i18n-area: settings
import { useState } from 'react';
import { openUrl } from '@tauri-apps/plugin-opener';
import { toolLinks, type ToolLinkId } from './toolLinks';
import { useT } from '../i18n';
import { T } from '../i18n/T';

export function ToolLink({ id, disabled = false }: { id: ToolLinkId; disabled?: boolean }) {
  const t = useT(), tool = toolLinks[id];
  const [error, setError] = useState(false), [copied, setCopied] = useState(false);
  async function launch() {
    if (disabled) return;
    setError(false);
    try { await openUrl(tool.url); } catch { setError(true); }
  }
  return <span className="tool-link">
    <a href={tool.url} target="_blank" rel="noopener noreferrer" title={t('{action} — opens in your browser', {action: t(tool.action)})} aria-disabled={disabled || undefined} tabIndex={disabled ? -1 : undefined} onClick={event => { event.preventDefault(); void launch(); }}>{t(tool.action)}<span aria-hidden="true"> ↗</span></a>
    {error && <span className="tool-link-error" role="alert"><T text="Couldn’t open your browser. Open this address manually: <code>{url}</code>" tags={{code: chunk => <code className="path">{chunk}</code>}} values={{url: tool.url}}/><button className="tertiary" onClick={() => { void navigator.clipboard.writeText(tool.url).then(() => setCopied(true)).catch(() => setCopied(false)); }}>{copied ? t('Copied') : t('Copy link')}</button></span>}
  </span>;
}

/** The four programs a new user installs, in the order the guide lists them. */
export const MAIN_TOOLS = ['deadlock', 'blender', 'source2viewer', 'csdk'] as const;

export function ToolDownloads({ busy, expanded = false }: { busy: boolean; expanded?: boolean }) {
  const t = useT();
  const row = (id: ToolLinkId) => <div className="tool-download-row" key={id}><div><div className="tool-download-title"><strong>{toolLinks[id].name}</strong><span className="tool-requirement">{t(toolLinks[id].requirement)}</span></div><p>{t(toolLinks[id].help)}</p></div><ToolLink id={id} disabled={busy}/></div>;
  return <details className="disclosure tool-downloads" open={expanded || undefined}>
    <summary>{t('Get programs and download links')}</summary>
    <div className="disclosure-content">
      <p className="hint">{t('Links open the publisher’s site or the CSDK community guide in your browser. Install or extract each tool, then select its location below.')}</p>
      {MAIN_TOOLS.map(row)}
      <details className="technical"><summary>{t('Windows runtimes (only if a tool will not start)')}</summary>{(['vcredist', 'webview2'] as const).map(row)}</details>
      <p className="hint">{t('VPK packaging and the independent cloth reader are included with the app. No separate Python or .NET installation is needed for them, and no separate model compiler or other Steam download is needed besides Deadlock.')}</p>
    </div>
  </details>;
}
