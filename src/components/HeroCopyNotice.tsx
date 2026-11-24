/** What "Make a copy for another hero" did: shown above Prepare model (and Character & model)
 * after the copy, and kept in Notifications (the automatic preparation moves on to the next
 * page). The lists come from Rust `deadlock_project::hero_copy`. */
import type { CopyReport } from '../models';

export interface CopyNotice extends CopyReport { to: string }

/** The notice as plain sentences, for its notification. */
export function copyLines(notice: CopyNotice): string[] {
  const list = (items: string[]) => items.join('; ');
  return [
    `The copy for ${notice.to} is a new project and is not saved yet. The project for ${notice.from_hero} is unchanged.`,
    ...(notice.carried.length ? [`Carried over: ${list(notice.carried)}.`] : []),
    ...(notice.not_carried.length ? [`Not carried, because they belong to ${notice.from_hero}: ${list(notice.not_carried)}.`] : []),
    ...(notice.remade.length ? [`Made again for ${notice.to}: ${list(notice.remade)}.`] : []),
  ];
}

export function HeroCopyNotice({ notice, dismiss }: { notice: CopyNotice; dismiss: () => void }) {
  const column = (title: string, items: string[], tone: string) => items.length > 0 && <div className={`hero-copy-column ${tone}`}>
    <h3>{title} <span className="hero-copy-count">({items.length})</span></h3>
    <ul>{items.map(item => <li key={item}>{item}</li>)}</ul>
  </div>;
  return <section className="hero-copy-notice" aria-label="What the copy carried over">
    <div className="hero-copy-head">
      <div><h2>Copy for {notice.to}</h2><p className="hint">A new project, not saved yet: save it as its own file. The project for {notice.from_hero} and its files are unchanged.</p></div>
      <button type="button" className="tertiary" onClick={dismiss}>Close</button>
    </div>
    <div className="hero-copy-columns">
      {column('Carried over', notice.carried, 'carried')}
      {column(`Not carried, because they belong to ${notice.from_hero}`, notice.not_carried, 'not-carried')}
      {column(`Made again for ${notice.to}`, notice.remade, 'remade')}
    </div>
  </section>;
}
