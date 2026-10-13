import { lazy, Suspense } from 'react';
import type { Project } from '../models';
import type { PreviewScene } from '../preview/packet';
import { Section } from './Primitives';
const Viewport = lazy(() => import('../preview/Viewport'));

/** The prepared character in 3D (with its gun), shown where its look and motion are chosen. */
export function CharacterPreview({ project, preview, load, busy }: { project: Project; preview: PreviewScene | null; load: () => void; busy: boolean }) {
  return <Section title="Your character" className="character-preview" actions={<button className={preview ? '' : 'primary'} disabled={busy || !project.dmx} onClick={load}>{preview ? 'Reload preview' : 'Show my character'}</button>}>
    {preview ? <Suspense fallback={<p className="hint">Loading the 3D view…</p>}><Viewport scene={preview} project={project} disabled={busy} /></Suspense>
      : <p className="hint">{project.dmx ? 'See your prepared character in 3D, with its gun and moving parts. Hair, cloth and charm motion play in the game.' : 'Prepare your model first (step 2).'}</p>}
  </Section>;
}
