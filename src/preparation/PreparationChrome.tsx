import type { ReactNode } from 'react';

export type PrepPanel = 'roles' | 'physics' | 'mapping' | 'weapon' | 'review';
export function PrepIcon({name,size=20}:{name:'model'|'roles'|'physics'|'mapping'|'weapon'|'review'|'arrow'|'check'|'eye'|'info'|'folder'|'refresh';size?:number}) {
  const shapes:Record<typeof name,ReactNode>={
    model:<><path d="m12 3 8 4.5v9L12 21l-8-4.5v-9L12 3Z"/><path d="m4 7.5 8 4.5 8-4.5M12 12v9"/></>,
    roles:<><circle cx="12" cy="4" r="2"/><path d="M12 6v8m-7-5 7 2 7-2m-7 5-5 7m5-7 5 7"/><circle cx="5" cy="9" r="1"/><circle cx="19" cy="9" r="1"/></>,
    physics:<><path d="M4 6c4-5 4 10 8 5s4 8 8 3M4 14c4-5 4 10 8 5s4 3 8-2"/><path d="M5 3v3m14 13v3"/></>,
    mapping:<><rect x="3" y="4" width="6" height="6" rx="1.5"/><rect x="15" y="14" width="6" height="6" rx="1.5"/><path d="M9 7h5a4 4 0 0 1 4 4v3m-8 3H6a3 3 0 0 1-3-3v-1m11 1 4 4 4-4"/></>,
    weapon:<><path d="M3 9h15l2-2h1v4h-3l-1 1h-4l-1 2H9l-1 4H5l1-5H3V9Z"/><path d="M12 13v2"/></>,
    review:<><path d="M8 4H5v17h14V4h-3"/><rect x="8" y="2" width="8" height="4" rx="1"/><path d="m8 13 3 3 5-6"/></>,
    arrow:<><path d="M4 12h16m-6-6 6 6-6 6"/></>,
    check:<path d="m5 12 4 4L19 6"/>,
    eye:<><path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12Z"/><circle cx="12" cy="12" r="3"/></>,
    info:<><circle cx="12" cy="12" r="9"/><path d="M12 11v6m0-10v1"/></>,
    folder:<path d="M3 6h6l2 2h10v12H3V6Z"/>,
    refresh:<><path d="M20 8a8 8 0 1 0 0 8M20 3v5h-5"/></>,
  };
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">{shapes[name]}</svg>;
}
/* The tab strip and the tab panels are the studio frame's (`StudioTabs`, `StudioTabPanel` with
 * base `prep`, so the ids stay `prep-tab-<id>` / `prep-panel-<id>`); the labels are `TAB_NAMES`. */
