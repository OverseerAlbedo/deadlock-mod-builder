import { lazy, Suspense, useEffect, useState } from 'react';
import type { Project } from '../models';
import type { ProjectInspection } from '../build/models';
import type { PreviewScene } from '../preview/packet';
import { decodeMotion, type MotionPacket, type PreviewNode, type Tuning } from '../preview/motion';
import { api } from '../api';
import { Section } from './Primitives';
const Viewport = lazy(() => import('../preview/Viewport'));
const MovementViewport = lazy(() => import('../preview/MovementViewport'));

/** The prepared character in 3D (with its gun), shown where its look and motion are
 * chosen: still, or moving through the hero's stand, run and jump clips with its
 * moving parts swinging (from the last Analyze and the Fine-tune sliders). */
export function CharacterPreview({ project, preview, load, busy, report = null, fail, analyze, analyzing = false }: { project: Project; preview: PreviewScene | null; load: () => void; busy: boolean; report?: ProjectInspection | null; fail?: (title: string, error: unknown) => void;
  /** Analyze project (records the moving parts the movement preview swings); runs by itself when Movement opens without them. */analyze?: () => void; analyzing?: boolean }) {
  const [mode, setMode] = useState<'still' | 'movement'>('still');
  const [packet, setPacket] = useState<MotionPacket | null>(null);
  const [loading, setLoading] = useState(false);
  useEffect(() => { setPacket(null); setMode('still'); }, [project.dmx, project.original_model]);
  async function showMovement() {
    setMode('movement');
    if (!preview) load();
    // The moving parts come from Analyze; run it now rather than show a still character.
    if (!analyzed && analyze && !busy) analyze();
    if (packet) return;
    setLoading(true);
    try { setPacket(decodeMotion(await api.motionPreview(project))); }
    catch (error) { setMode('still'); fail?.('The movement preview could not be loaded', error); }
    finally { setLoading(false); }
  }
  const analyzed = Boolean(report && 'dmx' in report);
  const nodes: PreviewNode[] = report && 'dmx' in report ? report.secondary_motion?.preview_nodes ?? report.motion_preview_nodes ?? [] : [];
  const tuning = (project.customization.secondary_motion?.tuning ?? {}) as Record<string, Tuning | undefined>;
  const switcher = <div className="segmented" role="radiogroup" aria-label="Character view">
    <button type="button" role="radio" aria-checked={mode === 'still'} onClick={() => setMode('still')}>Still</button>
    <button type="button" role="radio" aria-checked={mode === 'movement'} disabled={busy || !project.dmx} onClick={() => void showMovement()}>Movement</button>
  </div>;
  return <Section title="Your character" className="character-preview" actions={<div className="row">{switcher}<button className={preview ? '' : 'primary'} disabled={busy || !project.dmx} onClick={load}>{preview ? 'Reload preview' : 'Show my character'}</button></div>}>
    {mode === 'movement' && !nodes.length && <div className="capability-notice" role="status"><strong>{analyzing ? 'Analyzing your moving parts…' : analyzed ? 'No moving parts to swing' : 'Moving parts not analyzed yet'}</strong><p>{analyzing ? 'Hair, cloth and tails start swinging here as soon as the analysis finishes.' : analyzed ? 'The analysis found no moving parts for this model: switch Automatic secondary motion on below and keep hair, cloth and tails as moving parts in Prepare model, then Analyze again.' : 'The movement preview swings the parts recorded by Analyze project; until then the character plays the clips with everything still.'}</p>{analyze && !analyzing && <button className="primary" disabled={busy} onClick={analyze}>{analyzed ? 'Analyze again' : 'Analyze now'}</button>}</div>}
    {mode === 'movement'
      ? (preview && packet ? <Suspense fallback={<p className="hint">Loading the 3D view…</p>}><MovementViewport scene={preview} packet={packet} nodes={nodes} tuning={tuning}/></Suspense>
        : <p className="hint">{loading || !preview ? 'Loading your character and the hero’s stand, run and jump clips…' : 'Loading…'}</p>)
      : preview ? <Suspense fallback={<p className="hint">Loading the 3D view…</p>}><Viewport scene={preview} project={project} disabled={busy} /></Suspense>
        : <p className="hint">{project.dmx ? 'See your prepared character in 3D, with its gun and moving parts. Movement plays the hero’s stand, run and jump clips with hair, cloth and other moving parts swinging.' : 'Prepare your model first (step 2).'}</p>}
  </Section>;
}
