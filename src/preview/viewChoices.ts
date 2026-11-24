import type { Project } from '../models';
import type { MotionPacket } from './motion';
import type { PreviewScene } from './packet';

/** Which character preview a choice belongs to. Each one keeps its own choices and its own
 * defaults: only the Physics page's preview remembers them today (the Appearance page and
 * Prepare model's result view start Solid every time). */
export type PreviewView = 'physics';

/** What a character preview showed when its page was left. */
export interface ViewChoice {
  /** The explicit look (Solid, Textures, skin weights); null = the preview's own default. */
  shading: string | null;
  mode: 'still' | 'movement';
  /** The last motion packet and the preview scene it was shown with. */
  packet: MotionPacket | null;
  scene: PreviewScene | null;
}

interface Entry extends ViewChoice { model: string }

/** The prepared model a choice was made for: a new preparation, another hero or another
 * source file starts from the defaults. */
export function modelIdentity(project: Project): string {
  return JSON.stringify([project.dmx ?? '', project.original_model ?? '', project.preparation_report ?? '']);
}

/** The preview's choices for the session, kept outside the components because the preview
 * unmounts on every page change. In memory only: a new session or another project starts from
 * the defaults (`App` clears it with the texture store). It holds at most one motion packet. */
export class ViewChoices {
  private entries = new Map<string, Entry>();
  private key(project: string, view: PreviewView) { return `${view}\u0000${project}`; }
  /** The choices made for this project and prepared model, or null. */
  get(project: Project, view: PreviewView): ViewChoice | null {
    const entry = this.entries.get(this.key(project.id, view));
    return entry && entry.model === modelIdentity(project) ? entry : null;
  }
  set(project: Project, view: PreviewView, choice: ViewChoice) {
    // One packet at most: a packet held for another preview or project is let go.
    if (choice.packet) for (const entry of this.entries.values()) { entry.packet = null; entry.scene = null; }
    this.entries.set(this.key(project.id, view), {...choice, model: modelIdentity(project)});
  }
  clear() { this.entries.clear(); }
}

export const viewChoices = new ViewChoices();
