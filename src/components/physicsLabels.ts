import { createContext } from 'react';

/** Labels on the character in the Physics page's preview: the page provides this in Automatic
 * mode (null otherwise), and the preview shows one label per part type the last Analyze recorded.
 * `active`: the part type whose label is marked; `select`: a label was clicked. The preview element
 * is made by App, so a context reaches it without new props there. */
export interface PhysicsLabels { active: string | null; select: (kind: string) => void }
export const PhysicsLabelsContext = createContext<PhysicsLabels | null>(null);
