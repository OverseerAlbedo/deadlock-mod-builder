// i18n-area: motion
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { m, mc, tc, tp, useT } from '../i18n';
import { shownHero } from '../i18n/gameNames';
import { T } from '../i18n/T';
import { sentences } from '../build/words';
import { api } from '../api';
import type { Project, ResourceReplacement } from '../models';
import type { ClipGroup, ComponentCatalog } from '../components/AssetCatalog';
import type { PreviewScene } from '../preview/packet';
import { AssetRows } from '../components/CustomizationPages';
import { changeCustomization } from '../components/customization';
import { StudioFrame, StudioTopBar } from '../studio/Studio';
import { StudioBarMenuButton, StudioBarMenuPanel, useStudioBarMenus } from '../studio/StudioBarMenu';
import { StudioOptionCards } from '../studio/StudioChoices';
import { ClipList, type ListFilter } from './ClipList';
import { ClipCard, type ReplacementRoute } from './ClipCard';
import { ClipPlayer, type MadePreview } from './ClipPlayer';
import { MakeClip, type Operate } from './MakeClip';
import { TakeClip } from './TakeClip';
import { takenOf, withCompiledFile, withoutTaken } from './takenRows';
import { isAnimationFile, madeOf, withoutMade } from './madeRows';
import { NewestQueue } from './madeQueue';
import { clipState, fitKey, motionList } from './motionCatalog';
import type { FitReport } from './motionTypes';
import './motion.css';

export interface MotionPageProps {
  project: Project; update: (project: Project) => void; disabled: boolean; fail: (title: string, error: unknown) => void;
  /** A game archive is set in Settings (the list can be read). */canDiscover?: boolean;
  /** The prepared character's preview, and how to load it (Show my character). */
  preview?: PreviewScene | null; load?: () => void; autoLoad?: () => Promise<boolean>; previewCurrent?: boolean;
  /** Continue to Build & install. */next?: () => void;
  expert?: boolean;
  /** The app's operation runner (Blender work of Make it from my animation); a local stand-in without it. */
  operate?: Operate;
  /** Open Prepare model (a model prepared before clips could be made from an animation). */
  openPreparation?: () => void;
}

// The options' words stay English here and are translated where they are drawn (`shownOptions`).
const POLICIES = [
  {value: 'preserve', title: m('Preserve original animations'), badge: mc('option badge', 'default'), badgeTone: 'ok' as const, help: m('The hero’s animation references stay as the game has them.')},
  {value: 'disable_legacy', title: m('Disable legacy animation references'), help: m('For models whose build stops on old-style animation references.')},
];
const GRAPH_MODES = [
  {value: 'auto', title: mc('graph mode', 'Automatic'), badge: mc('option badge', 'default'), badgeTone: 'ok' as const, help: m('The app picks how the animation graph is compiled.')},
  {value: 'preserve', title: mc('graph mode', 'Preserve'), help: m('Keep the game’s compiled graph.')},
  {value: 'cswin', title: mc('graph mode', 'Legacy compiler'), help: m('Compile the graph with the older model toolset.')},
  {value: 'adapter', title: mc('graph mode', 'Configured adapter'), help: m('Use the adapter set up in Settings.')},
];
type Fit = FitReport | 'checking' | Error;

/** The Motion page: the hero's animations as a list beside the selected clip played on the prepared
 * character and the clip's card (replace it, its fit checks, what it is). The saved format is
 * `customization.animations` rows as before; opening the page or selecting a clip writes nothing.
 * Only the list's own read (`api.motionCatalog`: lease-free, kept per game archive and hero) starts
 * when the page opens, once per project and hero, also while another operation runs; a fit check runs for the selected clip's saved file, a clip packet only once the
 * character is shown. The player card always stays in the same place, so it never remounts. */
/** A row edited in Other saved rows whose file is no longer an animation file (Blender, FBX, glTF) is no longer made from
 * an animation (the saved format refuses `made` with another source), and a row taken from another hero that was given a
 * file is no longer taken (the saved format refuses `taken` beside a file). */
function withMadeSources(project: Project): Project {
  const rows = project.customization.animations;
  const filed = (row: ResourceReplacement) => Boolean(row.taken && (row.source ?? '').trim());
  const stale = (row: ResourceReplacement) => Boolean(row.made && !isAnimationFile(row.source)) || filed(row);
  if (!rows?.some(stale)) return project;
  return {...project, customization: {...project.customization, animations: rows.map(row => !stale(row) ? row : filed(row) ? withoutTaken(row) : withoutMade(row))}};
}

export function MotionPage(props: MotionPageProps) {
  const t = useT();
  /** The options with their words in the interface language (`graph`: the graph modes, whose titles have their own context). */
  const shownOptions = <O extends {title: string; help: string; badge?: string}>(options: O[], graph = false): O[] =>
    options.map(option => ({...option, title: graph ? tc('graph mode', option.title) : t(option.title), help: t(option.help), badge: option.badge && tc('option badge', option.badge)}));
  const {project, update, disabled, fail, canDiscover = true, preview = null, load, autoLoad, previewCurrent = true, next, operate, openPreparation} = props;
  const rows = useMemo(() => project.customization.animations ?? [], [project.customization.animations]);
  const projectRef = useRef(project); projectRef.current = project;
  const mounted = useRef(true);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  // The list's own read: once per project and hero when the page is opened. It takes no operation
  // lease, so it also runs while a build, Analyze or asset load is going on.
  const [listed, setListed] = useState<{key: string; catalog: ComponentCatalog | null; error: string | null; loading: boolean}>({key: '', catalog: null, error: null, loading: false});
  const listKey = `${project.id}:${project.original_model}`;
  const attempted = useRef('');
  const loadList = useCallback((reload = false) => {
    const key = listKey, asked = project;
    attempted.current = key;
    setListed({key, catalog: null, error: null, loading: true});
    Promise.resolve().then(() => api.motionCatalog(asked, reload)).then(
      result => { if (mounted.current && attempted.current === key) setListed({key, loading: false, error: null, catalog: {hero_path: result.hero_path, animations: result.animations, icons: [], effects: [], audio: [], variants: [], animation_unavailable: null, warnings: [], includes_animations: true}}); },
      error => { if (mounted.current && attempted.current === key) setListed({key, catalog: null, loading: false, error: error && typeof error === 'object' && 'message' in error ? String(error.message) : String(error)}); });
    // eslint-disable-next-line react-hooks/exhaustive-deps -- the key names the project and hero
  }, [listKey]);
  useEffect(() => { if (canDiscover && attempted.current !== listKey) loadList(); }, [canDiscover, listKey, loadList]);
  const current = listed.key === listKey ? listed : null;
  const catalog = current?.catalog && current.catalog.hero_path === project.original_model ? current.catalog : null;
  const discovering = Boolean(current?.loading);
  const discover = () => loadList(true);
  const list = useMemo(() => motionList(catalog, rows), [catalog, rows]);
  const [group, setGroup] = useState<ClipGroup>('movement');
  const [filter, setFilter] = useState<ListFilter>('all');
  const [query, setQuery] = useState('');
  const [chosen, setChosen] = useState('');
  // Opening on the page's own choice (the first replaced clip): show its group.
  const opened = useRef('');
  const item = list.items.find(i => i.entry.target === chosen) ?? list.folded.find(i => i.entry.target === chosen)
    ?? list.items.find(i => i.row) ?? list.items.find(i => i.entry.group === group) ?? list.items[0] ?? null;
  useEffect(() => {
    if (!catalog || chosen || !item || opened.current === listKey) return;
    opened.current = listKey;
    setGroup(item.entry.group);
  }, [catalog, chosen, item, listKey]);
  // A group or clip the user picks is the page's opening too: the effect above, when its run comes
  // only after that click (its render's effects still pending), must not put the old group back.
  const pickGroup = (next: ClipGroup) => { opened.current = listKey; setGroup(next); };
  const pickClip = (target: string) => { opened.current = listKey; setChosen(target); };
  // Fit checks, by clip and file (read-only, no lease): for the selected clip's saved file.
  const [fits, setFits] = useState<Map<string, Fit>>(() => new Map());
  useEffect(() => { setFits(new Map()); }, [project.original_model]);
  const source = item?.row?.source ?? '';
  // A clip made from an animation has no file to check (its source is the .blend): the engine's report says what it makes.
  const madeRow = madeOf(item?.row) ? item!.row : null;
  const key = item && source && !madeRow ? fitKey(item.entry.target, source) : '';
  useEffect(() => {
    if (!key || !item || fits.has(key)) return;
    setFits(current => new Map(current).set(key, 'checking'));
    const asked = project;
    api.clipFit(asked, item.entry.target, source)
      .then(report => { if (mounted.current) setFits(current => new Map(current).set(key, report)); })
      .catch(error => { if (mounted.current) setFits(current => new Map(current).set(key, new Error(error && typeof error === 'object' && 'message' in error ? String(error.message) : String(error)))); });
    // eslint-disable-next-line react-hooks/exhaustive-deps -- one check per clip and file
  }, [key]);
  const menus = useStudioBarMenus<'graph'>('motion');
  const [confirming, setConfirming] = useState(false);
  const change = (next: ResourceReplacement[], activate = true) => update(changeCustomization(project, {animations: next}, activate));
  // Replace: the row the old destination picker wrote (target from the list, not shared). A
  // compiled file chosen for a clip made from an animation takes the place of that.
  function choose(path: string) {
    if (!item) return;
    if (item.row && item.index >= 0) change(rows.map((row, i) => i === item.index ? withCompiledFile(row, path) : row));
    else if (rows.length < 128) change([...rows, {enabled: true, source: path, target: item.entry.target, allow_shared: false}]);
    else fail(m('Too many clip replacements'), 'A project holds at most 128 clip replacements.');
  }
  const edit = (values: Partial<ResourceReplacement>) => { if (item?.row) change(rows.map((row, i) => i === item.index ? {...row, ...values} : row)); };
  const remove = () => { if (item?.row) change(rows.filter((_, i) => i !== item.index)); };
  // Make it from my animation. The route is page state only (flipping it writes nothing): a clip
  // made from an animation opens on it, every other clip on the compiled file.
  const [routes, setRoutes] = useState<Map<string, ReplacementRoute>>(() => new Map());
  const routeKey = item ? item.entry.target.toLowerCase() : '';
  const takenRow = takenOf(item?.row) ? item!.row : null;
  const route: ReplacementRoute = routes.get(routeKey) ?? (madeRow ? 'make' : takenRow ? 'taken' : 'file');
  const setRoute = (value: ReplacementRoute) => setRoutes(current => new Map(current).set(routeKey, value));
  /** Bumped after every make, so the report and the player ask again for the same row. */
  const [revision, setRevision] = useState(0);
  /** The clip's row written as ONE step (replaced in place, or added); returns the project written. */
  function writeRow(row: ResourceReplacement): Project | null {
    const now = projectRef.current, current = now.customization.animations ?? [];
    const at = item ? current.findIndex((r, i) => i === item.index && (r.target ?? '').toLowerCase() === (item.entry.target ?? '').toLowerCase()) : -1;
    if (at < 0 && current.length >= 128) { fail(m('Too many clip replacements'), 'A project holds at most 128 clip replacements.'); return null; }
    const nextRows = at >= 0 ? current.map((r, i) => i === at ? row : r) : [...current, row];
    const written = changeCustomization(now, {animations: nextRows}, true);
    projectRef.current = written;
    update(written);
    return written;
  }
  // The runner for Blender work: the app's (busy state, Cancel, the error line), or a local stand-in.
  const [localBusy, setLocalBusy] = useState(false);
  const localRunning = useRef(false);
  const run: Operate = operate ?? (async (label, operation) => {
    if (localRunning.current) return undefined;
    localRunning.current = true; setLocalBusy(true);
    try { return await operation(); } catch (error) { fail(label, error); return undefined; }
    finally { localRunning.current = false; if (mounted.current) setLocalBusy(false); }
  });
  // The made clip's packet for the player: one request at a time, the newest waiting one wins.
  const previews = useRef(new NewestQueue());
  const madePreview: MadePreview | null = useMemo(() => {
    if (!madeRow || !item) return null;
    const target = item.entry.target, row = madeRow;
    return {key: `${JSON.stringify(row)}|${revision}`, fetch: () => previews.current.ask(() => api.madeClipPreview(projectRef.current, target, row))};
    // eslint-disable-next-line react-hooks/exhaustive-deps -- the row's text and the revision name the packet
  }, [madeRow && JSON.stringify(madeRow), item?.entry.target, revision]);
  // A clip taken from another hero: its packet, asked through its own queue (newest wins).
  const takenPreviews = useRef(new NewestQueue());
  /** The other hero's name, as the panel learns it from the engine's answers (for the player's words). */
  const [takenName, setTakenName] = useState('');
  const takenPreview: MadePreview | null = useMemo(() => {
    if (!takenRow || !item) return null;
    const target = item.entry.target, row = takenRow;
    return {key: `taken:${JSON.stringify(row.taken)}`, from: takenName, fetch: () => takenPreviews.current.ask(() => api.takenClipPreview(projectRef.current, target, row))};
    // eslint-disable-next-line react-hooks/exhaustive-deps -- `taken` and the clip name the packet
  }, [takenRow && JSON.stringify(takenRow.taken), item?.entry.target, takenName]);
  /** All animation rows written as ONE step (Take all of a set). */
  function writeRows(next: ResourceReplacement[]) {
    const written = changeCustomization(projectRef.current, {animations: next}, true);
    projectRef.current = written;
    update(written);
  }
  const busy = disabled || localBusy;
  // "Switch all replacements off": the old "Preserve original animations" choice, exactly.
  const switchOff = () => update({...changeCustomization(project, {animations: rows.map(row => ({...row, enabled: false}))}, false), animation_policy: 'preserve'});
  const removeAll = () => { setConfirming(false); change([], false); };
  const active = rows.filter(row => row.enabled !== false && (row.source || takenOf(row)) && row.target);
  const replacedCount = project.customization.enabled ? active.length : 0;
  const notFitting = list.items.filter(i => clipState(project, i, fits) === 'does_not_fit').length;
  // The hero's name as the game has it; only shown (also inside the other hero panel's sentences).
  const hero = shownHero(project.hero_name) || project.original_model.split('/').pop()?.replace(/\.vmdl(_c)?$/, '') || t('The hero');
  const status = [hero, catalog ? tp(list.items.filter(i => i.entry.graph_named).length, {one: '{count} animation clips from your game', other: '{count} animation clips from your game'}) : discovering ? t('loading the animation list…') : null, notFitting ? tp(notFitting, {one: '{count} does not fit', other: '{count} does not fit'}) : null].filter(Boolean).join(' · ');
  const policyNote = project.animation_policy !== 'preserve' ? <p className="clip-policy-note" role="note">{t('Advanced policy active: legacy animation references are disabled.')}</p> : null;
  const panel = menus.open === 'graph'
    ? <StudioBarMenuPanel base="motion" id="graph" title={t('Animation graph handling')} onClose={() => menus.close('graph')} className="motion-graph-panel">
      <p className="studio-card-sub">{t('Leave these on their defaults unless a build tells you otherwise.')}</p>
      <div className="motion-graph-options">
        <div><h4>{t('Animation policy')}</h4><StudioOptionCards label={t('Animation policy')} name="motion-animation-policy" options={shownOptions(POLICIES)} value={project.animation_policy} onChange={animation_policy => update({...project, animation_policy})} disabled={disabled}/></div>
        <div><h4>{t('Animation graph mode')}</h4><StudioOptionCards label={t('Animation graph mode')} name="motion-graph-mode" options={shownOptions(GRAPH_MODES, true)} value={project.ag2_mode} onChange={ag2_mode => update({...project, ag2_mode})} disabled={disabled}/></div>
      </div>
      {policyNote}
    </StudioBarMenuPanel>
    : confirming ? <div className="motion-confirm" role="alertdialog" aria-label={t('Remove all replacements')}>
      <p>{rows.length === 1
        ? <T text="<strong>Remove all {count} saved clip replacement?</strong> The hero plays the game’s own clips again; your files stay where they are." values={{count: rows.length}}/>
        : <T text="<strong>Remove all {count} saved clip replacements?</strong> The hero plays the game’s own clips again; your files stay where they are." values={{count: rows.length}}/>}</p>
      <button type="button" className="motion-danger" disabled={disabled} onClick={removeAll}>{t('Remove all')}</button>
      <button type="button" className="tertiary" onClick={() => setConfirming(false)}>{t('Cancel')}</button>
    </div> : null;
  const listStatus = catalog ? null
    : discovering ? <p className="hint" role="status">{t('Loading the hero’s animations…')}</p>
      : <div className="clip-list-empty" role="status"><p className="hint">{!canDiscover ? t('Choose the game’s pak01_dir.vpk in Settings to see the hero’s animations.') : current?.error ? t('The hero’s animation list could not be read: {reason}', {reason: current.error}) : t('The hero’s animation list is not loaded.')}</p>{canDiscover && <button type="button" onClick={discover}>{current?.error ? t('Try again') : t('Load the animation list')}</button>}</div>;
  return <StudioFrame label={t('Motion')} className="motion-page">
    <h1 className="sr-only">{t('Motion')}</h1>
    <StudioTopBar title={t('Motion')} badge={replacedCount ? tp(replacedCount, {one: '{count} clip replaced', other: '{count} clips replaced'}) : t('Original movement')} badgeTone={replacedCount ? undefined : 'ok'} status={status} below={panel}>
      <StudioBarMenuButton base="motion" id="graph" open={menus.open === 'graph'} onToggle={() => { setConfirming(false); menus.toggle('graph'); }} label={t('Animation graph handling')} tone={project.animation_policy !== 'preserve' ? 'attention' : undefined}/>
      {active.length > 0 && <button type="button" disabled={disabled} onClick={switchOff}>{t('Switch all replacements off')}</button>}
      <button type="button" className="tertiary" disabled={disabled || !rows.length} onClick={() => { menus.setOpen(null); setConfirming(true); }}>{t('Remove all replacements…')}</button>
    </StudioTopBar>
    {menus.open !== 'graph' && policyNote}
    {!project.customization.enabled && rows.length > 0 && <p className="hint motion-inactive">{t('Saved replacements are inactive while project customization is off. Enable it in Extras to include them.')}</p>}
    <div className="motion-grid">
      <ClipList project={project} items={list.items} folded={list.folded} fits={fits} selected={item?.entry.target ?? ''} select={pickClip} group={group} setGroup={pickGroup} filter={filter} setFilter={setFilter} query={query} setQuery={setQuery} status={listStatus}/>
      <div className="motion-view-slot"><ClipPlayer project={project} entry={item?.entry ?? null} source={source} made={madePreview ?? takenPreview} preview={preview} load={load} autoLoad={autoLoad} previewCurrent={previewCurrent} busy={busy}/></div>
      <ClipCard item={item} state={item ? clipState(project, item, fits) : 'original'} fit={key ? fits.get(key) : undefined} nothingReplaced={!replacedCount} disabled={busy} fail={fail}
        choose={choose} remove={remove} include={enabled => edit({enabled})} share={allow_shared => edit({allow_shared})} next={next}
        route={route} setRoute={setRoute}
        takenPanel={item && route === 'taken' ? <TakeClip key={item.entry.target} project={project} entry={item.entry} entries={catalog?.animations ?? []} heroName={hero} row={item.row} disabled={busy}
          write={writeRow} writeRows={writeRows} remove={remove} useCompiled={() => setRoute('file')} fail={fail} named={setTakenName}/> : null}
        makePanel={item && route === 'make' ? <MakeClip key={item.entry.target} project={project} entry={item.entry} row={item.row} disabled={busy} revision={revision}
          write={writeRow} remove={remove} operate={run} made={() => { if (mounted.current) setRevision(r => r + 1); }} useCompiled={() => setRoute('file')} openPreparation={openPreparation}/> : null}/>
    </div>
    {list.others.length > 0 && <div className="motion-other-rows">
      <AssetRows kind="animations" only={list.others} title={t('Other saved rows')} project={project} update={next => update(withMadeSources(next))} disabled={disabled} fail={fail} catalog={catalog} discover={discover}/>
      <p className="hint">{sentences(t('Saved rows the list cannot show: a draft without a file or clip, a clip this hero does not have, or a second row for the same clip.'), !catalog ? t('The list is not loaded, so every saved row is here.') : '')}</p>
    </div>}
  </StudioFrame>;
}
