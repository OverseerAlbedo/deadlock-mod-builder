// i18n-area: motion
import { useEffect, useRef, useState, type ReactNode } from 'react';
import { m, mc, t, tc, tp, useT } from '../i18n';
import { T } from '../i18n/T';
import { sentences } from '../build/words';
import type { ResourceReplacement } from '../models';
import { openFor } from '../dialogs';
import { api } from '../api';
import { extensions } from '../components/FilePathInput';
import { Check } from '../components/CustomizationPages';
import type { FitReport } from './motionTypes';
import { seconds, shortPath, STATE_WORDS, type ClipItem, type ClipState } from './motionCatalog';

/** A fit check's status (English; shown with `tc('fit status', word)`). */
const STATUS_WORDS = {pass: mc('fit status', 'checked'), fail: mc('fit status', 'does not fit'), not_checked: mc('fit status', 'not checked'), after_compile: mc('fit status', 'checked at build')} as const;
const MARKS = {pass: '✓', fail: '!', not_checked: '–', after_compile: '…'} as const;
/** The quiet line for a clip compiled from the user's own animation (a .vnmclip with its DMX), the one
 * kind of replacement not yet tried in game; replaced compiled clips play in game as expected. */
export const AUTHORED_UNTRIED = m('Your animation is compiled into this clip at build: check it in a match.');
/** A saved replacement that names an animation document (.vnmclip), compiled at build. */
export const authoredSource = (source: string | undefined) => /\.vnmclip$/i.test(source?.trim() ?? '');

/** Taking a clip file: a drop on `node` or the file dialog, checked by the app's intake. */
function useClipFile(disabled: boolean, choose: (path: string) => void, fail: (title: string, error: unknown) => void) {
  const node = useRef<HTMLDivElement>(null);
  const [checking, setChecking] = useState(false);
  const latest = useRef({disabled, choose, fail, checking});
  latest.current = {disabled, choose, fail, checking};
  async function commit(path: string) {
    if (latest.current.disabled || latest.current.checking) return;
    setChecking(true);
    try { latest.current.choose(await api.validateAssetPath(path.trim(), 'animation')); }
    catch (error) { latest.current.fail(m('Could not use this file as a clip'), error); }
    finally { setChecking(false); }
  }
  const commitRef = useRef(commit); commitRef.current = commit;
  useEffect(() => {
    const element = node.current;
    const drop = (event: Event) => { const paths = (event as CustomEvent<string[]>).detail; if (paths.length === 1) void commitRef.current(paths[0]); else latest.current.fail(m('Choose one clip'), 'Drop one clip file here.'); };
    element?.addEventListener('dmb-file-drop', drop);
    return () => element?.removeEventListener('dmb-file-drop', drop);
  }, []);
  async function browse() {
    try {
      const path = await openFor('animation-clip', {title: t('Replacement clip'), multiple: false, filters: [{name: t('Animation clip'), extensions: extensions.animation}]});
      if (typeof path === 'string') await commit(path);
    } catch (error) { latest.current.fail(m('Could not choose a clip'), error); }
  }
  return {node, checking, commit: (path: string) => commitRef.current(path), browse};
}

/** The drop target and file button for a clip that is not replaced yet. */
function ReplaceDrop({disabled, choose, fail}: {disabled: boolean; choose: (path: string) => void; fail: (title: string, error: unknown) => void}) {
  const t = useT();
  const file = useClipFile(disabled, choose, fail);
  return <div className="clip-drop" ref={file.node} data-native-drop={disabled ? undefined : 'animation'}>
    <p>{t('Drop a compiled clip (.vnmclip_c) here, or a .vnmclip that names your animation DMX')}</p>
    <button type="button" disabled={disabled || file.checking} onClick={() => void file.browse()}>{file.checking ? t('Checking…') : t('Choose a file…')}</button>
  </div>;
}

/** A file name cut in the middle, the extension and the last letters kept. */
export function fileNameParts(path: string): [string, string] {
  const name = path.split(/[\\/]/).pop() ?? path;
  const ext = /\.[^.]+$/.exec(name)?.[0] ?? '';
  const stem = name.slice(0, name.length - ext.length), cut = Math.max(0, stem.length - 4);
  return [stem.slice(0, cut), stem.slice(cut) + ext];
}

/** The saved replacement file: its name over the card's width (full path as the tooltip), then Edit
 * path, Replace… and Remove. A file dropped on it replaces it. */
function ReplacementFile({path, label, disabled, choose, remove, fail}: {path: string; label: string; disabled: boolean; choose: (path: string) => void; remove: () => void; fail: (title: string, error: unknown) => void}) {
  const t = useT();
  const file = useClipFile(disabled, choose, fail);
  const [editing, setEditing] = useState(false), [draft, setDraft] = useState(path);
  useEffect(() => { setDraft(path); }, [path]);
  const [start, end] = fileNameParts(path);
  const commit = () => { setEditing(false); if (draft.trim() && draft.trim() !== path) void file.commit(draft); else setDraft(path); };
  return <div className="clip-file" ref={file.node} data-native-drop={disabled ? undefined : 'animation'} role="group" aria-label={label}>
    {editing
      ? <input className="clip-file-input mono" aria-label={label} value={draft} autoFocus onChange={event => setDraft(event.target.value)} onBlur={commit} onKeyDown={event => { if (event.key === 'Enter') { event.preventDefault(); commit(); } else if (event.key === 'Escape') { setDraft(path); setEditing(false); } }}/>
      : <span className="clip-file-name mono" title={path}><span className="clip-file-start">{start}</span><span className="clip-file-end">{end}</span></span>}
    <span className="clip-file-actions">
      <button type="button" disabled={disabled || file.checking} onClick={() => setEditing(!editing)}>{editing ? t('Done') : t('Edit path')}</button>
      <button type="button" disabled={disabled || file.checking} onClick={() => void file.browse()}>{file.checking ? t('Checking…') : t('Replace…')}</button>
      <button type="button" className="tertiary" disabled={disabled} onClick={remove}>{t('Remove')}</button>
    </span>
  </div>;
}

function FitList({fit}: {fit: FitReport | 'checking' | Error | undefined}) {
  const t = useT();
  if (fit === undefined || fit === 'checking') return <p className="hint" role="status">{t('Checking the file against the game’s clip…')}</p>;
  if (fit instanceof Error) return <p className="hint" role="status">{t('The fit could not be checked here: {reason} The build checks it again.', {reason: fit.message})}</p>;
  return <>
    <ul className="clip-checks" aria-label={t('Fit checks')}>
      {fit.checks.map(check => <li key={check.id} className={check.status}>
        <span className="clip-check-mark" aria-hidden="true">{MARKS[check.status]}</span>
        <span className="clip-check-label">{check.label}</span>
        <span className="clip-check-status">{tc('fit status', STATUS_WORDS[check.status])}</span>
      </li>)}
    </ul>
    {fit.fits
      ? fit.kind === 'authored'
        ? <div className="clip-verdict quiet" role="status"><T text="<strong>Checked after compiling, at build.</strong> The steps before compiling pass; the build compiles your clip and refuses it if it does not fit."/></div>
        : <div className="clip-verdict ok" role="status"><T text="<strong>Fits:</strong> only the poses differ from the game’s clip."/></div>
      : <div className="clip-verdict attention" role="status"><T text="<strong>The build stops on a clip that does not fit.</strong> {reason} The game reads sounds, effects and movement from these values, so only the poses may differ. Make the clip again, or switch it off below." values={{reason: fit.reason}}/></div>}
  </>;
}

/** The selected clip's card: replace it, its fit checks, Include in build, Remove, and what the
 * clip is. Writes only through `choose`, `remove`, `include` and `share` (the saved row format). */
/** How the replacement is made: a compiled clip file, made from the user's animation, or taken from another hero's animation. */
export type ReplacementRoute = 'file' | 'make' | 'taken';
const ROUTES: readonly [ReplacementRoute, string][] = [['file', m('A compiled clip file')], ['make', m('Make it from my animation')], ['taken', m('Another hero’s animation')]];

export function ClipCard({item, state, fit, nothingReplaced, disabled, fail, choose, remove, include, share, next, route = 'file', setRoute, makePanel, takenPanel}: {
  item: ClipItem | null; state: ClipState; fit: FitReport | 'checking' | Error | undefined; nothingReplaced: boolean; disabled: boolean;
  fail: (title: string, error: unknown) => void; choose: (path: string) => void; remove: () => void; include: (on: boolean) => void; share: (on: boolean) => void;
  next?: () => void;
  /** The shown route; flipping it writes nothing. Without `setRoute` the switch is not shown. */
  route?: ReplacementRoute; setRoute?: (route: ReplacementRoute) => void;
  /** "Make it from my animation", shown on the make route. */
  makePanel?: ReactNode;
  /** "Another hero’s animation", shown on the taken route. */
  takenPanel?: ReactNode;
}) {
  const t = useT();
  if (!item) return <section className="studio-card clip-card" aria-label={t('Clip')}><p className="hint">{t('Choose a clip from the list to see it, replace it or check a replacement.')}</p></section>;
  const {entry, row} = item;
  const made = Boolean(row?.made), taken = Boolean(row?.taken);
  const shownRoute: ReplacementRoute = setRoute ? route : 'file';
  const elsewhere = t('An animation in a Blender, FBX or glTF file can be made into this clip instead (Make it from my animation), or the move taken from another hero (Another hero’s animation).');
  const tone = state === 'replaced' ? 'changed' : state === 'does_not_fit' ? 'attention' : state === 'off' ? 'quiet' : 'ok';
  const switcher = setRoute && <span className="studio-view-group clip-route" role="group" aria-label={t('How the replacement is made')}>
    {ROUTES.map(([id, label]) => <button key={id} type="button" aria-pressed={shownRoute === id} onClick={() => setRoute(id)}>{t(label)}</button>)}
  </span>;
  const heading = row ? t('Your replacement') : t('Replace this clip');
  return <section className="studio-card clip-card" aria-label={t('Clip {name}', {name: entry.name})}>
    <div className="clip-card-head">
      <div><h3>{entry.name}</h3><p className="mono clip-path" title={entry.target}>{shortPath(entry.target)}</p></div>
      <span className={`studio-badge ${tone}`}>{tc('clip state', STATE_WORDS[state])}</span>
    </div>
    {!row && nothingReplaced && <div className="clip-verdict ok"><T text="<strong>Original movement, ready to keep.</strong> Your character uses the hero’s own animations; nothing needs to be done on this page."/></div>}
    {shownRoute === 'taken' ? <>
      <h4>{heading}</h4>
      {switcher}
      {takenPanel}
      {row && <ClipInclude row={row} shared={entry.shared} disabled={disabled} include={include} share={share} next={next}/>}
    </> : shownRoute === 'make' ? <>
      <h4>{heading}</h4>
      {switcher}
      {makePanel}
      {row && <ClipInclude row={row} shared={entry.shared} disabled={disabled} include={include} share={share} next={next}/>}
    </> : row && !made && !taken ? <>
      <h4>{t('Your replacement')}</h4>
      {switcher}
      <ReplacementFile path={row.source ?? ''} label={t('Replacement clip for {name}', {name: entry.name})} disabled={disabled} choose={choose} remove={remove} fail={fail}/>
      <p className="hint">{sentences(t('A compiled clip (.vnmclip_c), or a .vnmclip that names your animation DMX, made for this hero’s skeleton.'), setRoute ? elsewhere : '')}</p>
      <h4>{t('Does it fit the original?')}</h4>
      <FitList fit={fit}/>
      {authoredSource(row.source) && <p className="hint clip-untried">{t(AUTHORED_UNTRIED)}</p>}
      <ClipInclude row={row} shared={entry.shared} disabled={disabled} include={include} share={share} next={next}/>
    </> : <>
      <h4>{heading}</h4>
      {switcher}
      {made && <p className="hint">{t('This clip is made from your animation now. A compiled clip file chosen here replaces that.')}</p>}
      {taken && <p className="hint">{t('This clip is taken from another hero’s animation now. A compiled clip file chosen here replaces that.')}</p>}
      <ReplaceDrop disabled={disabled} choose={choose} fail={fail}/>
      <p className="hint">{sentences(t('The clip must be made for this hero’s skeleton, with the same length, events and root motion as the original: only the poses may differ.'), setRoute ? elsewhere : '')}</p>
      {row && <ClipInclude row={row} shared={entry.shared} disabled={disabled} include={include} share={share} next={next}/>}
    </>}
    {item.derived && <p className="hint">{t('The game also keeps a full-pose copy of this clip that its compiler derived from it; that copy stays the game’s.')}</p>}
    {item.unnamed && <p className="hint">{t('No animation graph of this hero names this clip.')}</p>}
    <h4>{t('About this clip')}</h4>
    <dl className="clip-about">
      <dt>{t('Length')}</dt><dd>{seconds(entry.duration)} · {tp(entry.frames, {one: '{count} frame', other: '{count} frames'})}</dd>
      <dt>{t('Bone tracks')}</dt><dd>{entry.tracks}</dd>
      <dt>{t('Events')}</dt><dd>{entry.events}</dd>
      {entry.additive && <><dt>{tc('clip about', 'Kind')}</dt><dd>{t('An additive layer (the game plays it over another pose)')}</dd></>}
      {entry.ends_where_starts && <><dt>{t('Ends')}</dt><dd>{t('Ends where it starts')}</dd></>}
      <dt>{t('Clip name')}</dt><dd className="mono">{entry.label}</dd>
    </dl>
  </section>;
}

/** Include in build, the shared choice and the way to Build & install, for any saved row. */
function ClipInclude({row, shared, disabled, include, share, next}: {row: ResourceReplacement; shared?: boolean; disabled: boolean; include: (on: boolean) => void; share: (on: boolean) => void; next?: () => void}) {
  const t = useT();
  return <>
      <div className="clip-include">
        <Check label={t('Include in build')} value={row.enabled ?? true} onChange={include} disabled={disabled}/>
        {shared && <><Check label={t('Allow this replacement to affect other characters sharing the clip')} value={row.allow_shared ?? false} onChange={share} disabled={disabled}/><p className="hint">{t('Other heroes also use this clip.')}</p></>}
      </div>
      {next && <p className="clip-next hint">{t('Used by the next build.')} <button type="button" onClick={next}>{t('Open Build & install')} <span aria-hidden="true">→</span></button></p>}
  </>;
}
