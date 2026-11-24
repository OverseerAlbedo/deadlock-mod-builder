import { useId, type ReactNode } from 'react';
import '../../studio/studio.css';
import '../prepParts.css';
/** A tab's main card on Prepare model: a region named by its heading, a line under it and actions
 * on the right of the heading (the studio card look, with the region the old sections had). */
export function PrepCard({title, sub, actions, className = '', children}: {title: string; sub?: ReactNode; actions?: ReactNode; className?: string; children?: ReactNode}) {
  const id = useId();
  return <section className={`studio-card prep-tab-card ${className}`} aria-labelledby={id}>
    <div className="studio-card-head"><h2 id={id} className="prep-tab-card-title">{title}</h2>{actions && <span className="studio-card-actions">{actions}</span>}</div>
    {sub && <p className="studio-card-sub">{sub}</p>}
    {children}
  </section>;
}
/** A small uppercase heading inside a card (Quick fixes, Detected groups, Hands…). */
export function PrepCardHeading({children, actions}: {children: ReactNode; actions?: ReactNode}) {
  return <div className="prep-card-heading"><h3>{children}</h3>{actions && <span className="prep-card-heading-actions">{actions}</span>}</div>;
}
