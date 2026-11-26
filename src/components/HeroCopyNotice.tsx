/** What "Make a copy for another hero" did: shown above Prepare model (and Character & model)
 * after the copy, and kept in Notifications (the automatic preparation moves on to the next
 * page). The lists come from Rust `deadlock_project::hero_copy`. */
import type { CopyReport } from '../models';
import { m, useT } from '../i18n';
import { englishMessage, type Message } from './Notifications';
import { shownHero, useGameNames } from '../i18n/gameNames';
// i18n-area: shell

export interface CopyNotice extends CopyReport { to: string }

/** The notice as sentences for its notification, kept in English and translated where they are
 * drawn. The listed items come from the backend and stay as they are. */
export function copyMessages(notice: CopyNotice): Message[] {
  const list = (items: string[]) => items.join('; ');
  return [
    { text: m('The copy for {to} is a new project and is not saved yet. The project for {from} is unchanged.'), values: { to: notice.to, from: notice.from_hero } },
    ...(notice.carried.length ? [{ text: m('Carried over: {items}.'), values: { items: list(notice.carried) } }] : []),
    ...(notice.not_carried.length ? [{ text: m('Not carried, because they belong to {from}: {items}.'), values: { from: notice.from_hero, items: list(notice.not_carried) } }] : []),
    ...(notice.remade.length ? [{ text: m('Made again for {to}: {items}.'), values: { to: notice.to, items: list(notice.remade) } }] : []),
  ];
}
/** The notice as plain English sentences. */
export function copyLines(notice: CopyNotice): string[] {
  return copyMessages(notice).map(englishMessage);
}

export function HeroCopyNotice({ notice, dismiss }: { notice: CopyNotice; dismiss: () => void }) {
  const t = useT();
  useGameNames();
  // The heroes' names as the game shows them in the interface language (the notice keeps English).
  const to = shownHero(notice.to), from = shownHero(notice.from_hero);
  const column = (title: string, items: string[], tone: string) => items.length > 0 && <div className={`hero-copy-column ${tone}`}>
    <h3>{title} <span className="hero-copy-count">({items.length})</span></h3>
    <ul>{items.map(item => <li key={item}>{item}</li>)}</ul>
  </div>;
  return <section className="hero-copy-notice" aria-label={t('What the copy carried over')}>
    <div className="hero-copy-head">
      <div><h2>{t('Copy for {hero}', { hero: to })}</h2><p className="hint">{t('A new project, not saved yet: save it as its own file. The project for {hero} and its files are unchanged.', { hero: from })}</p></div>
      <button type="button" className="tertiary" onClick={dismiss}>{t('Close')}</button>
    </div>
    <div className="hero-copy-columns">
      {column(t('Carried over'), notice.carried, 'carried')}
      {column(t('Not carried, because they belong to {hero}', { hero: from }), notice.not_carried, 'not-carried')}
      {column(t('Made again for {hero}', { hero: to }), notice.remade, 'remade')}
    </div>
  </section>;
}
