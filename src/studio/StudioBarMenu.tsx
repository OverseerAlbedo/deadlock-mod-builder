import { useCallback, useState, type KeyboardEvent, type ReactNode } from 'react';
import type { StudioTone } from './Studio';
import './studioBarMenu.css';

/** Top-bar menus whose panel opens UNDER the bar, across its whole width (a disclosure, not a
 * popover that closes on an outside click). One menu is open at a time. A panel closes only
 * on its button, its Close button or Escape, never on a click elsewhere or a lost focus, so a form
 * in it stays usable while a file dialog is open or a re-check runs. A closed panel is not
 * rendered: keep its state in the page. Put the buttons in `StudioTopBar`'s tools and the open
 * panel in its `below` slot. */
export function useStudioBarMenus<T extends string>(base: string) {
  const [open, setOpen] = useState<T | null>(null);
  const toggle = useCallback((id: T) => setOpen(current => current === id ? null : id), []);
  const close = useCallback((id: T) => {
    setOpen(current => current === id ? null : current);
    document.getElementById(barMenuButtonId(base, id))?.focus();
  }, [base]);
  return {open, setOpen, toggle, close, base};
}
export const barMenuButtonId = (base: string, id: string) => `${base}-menu-${id}`;
export const barMenuPanelId = (base: string, id: string) => `${base}-menu-panel-${id}`;

/** A menu button: its name, an optional short detail (the current choice or a count) and a mark
 * when something in it needs a look (`tone` attention). */
export function StudioBarMenuButton({base, id, open, onToggle, label, hiddenLabel, detail, tone, disabled}: {base: string; id: string; open: boolean; onToggle: () => void; label: string; /** Read before the label by screen readers only (when the label is a value, say what it is). */hiddenLabel?: string; detail?: ReactNode; tone?: StudioTone; disabled?: boolean}) {
  return <button type="button" id={barMenuButtonId(base, id)} className={`studio-barmenu-button ${tone ?? ''}`} aria-expanded={open} aria-controls={open ? barMenuPanelId(base, id) : undefined} disabled={disabled} onClick={onToggle}>
    {tone === 'attention' && <span className="studio-barmenu-mark" aria-hidden="true"/>}
    {hiddenLabel && <span className="sr-only">{hiddenLabel}: </span>}<span>{label}</span>{detail !== undefined && detail !== null && detail !== '' && <small>{detail}</small>}
    {tone === 'attention' && <span className="sr-only"> (needs a look)</span>}
    <span className="studio-barmenu-caret" aria-hidden="true">▾</span>
  </button>;
}

/** A menu's panel, under the bar. Escape closes it and gives the focus back to its button.
 * Without `onClose` it is shown open with no Close button (a page that has no menu button for it
 * yet, for example before a model is checked); render it in the same slot either way, so a form in
 * it keeps its state when it turns into a menu or back. */
export function StudioBarMenuPanel({base, id, title, onClose, children, className = ''}: {base: string; id: string; title: string; onClose?: () => void; children: ReactNode; className?: string}) {
  function keys(event: KeyboardEvent<HTMLElement>) {
    if (!onClose || event.key !== 'Escape' || event.defaultPrevented) return;
    // A native control that uses Escape itself (an open select) keeps it.
    if ((event.target as HTMLElement).tagName === 'SELECT') return;
    event.preventDefault(); onClose();
  }
  return <section id={barMenuPanelId(base, id)} className={`studio-barmenu-panel ${className}`} role="region" aria-label={title} onKeyDown={keys}>
    <div className="studio-barmenu-panel-head"><h3>{title}</h3>{onClose && <button type="button" className="tertiary studio-barmenu-close" aria-label={`Close ${title}`} onClick={onClose}>Close</button>}</div>
    {children}
  </section>;
}
