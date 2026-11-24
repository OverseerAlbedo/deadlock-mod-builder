/** Types of the Motion page's commands (Rust `motion::FitReport`, `clip_preview::Played`). */

/** One fit check (Rust `motion::FitCheck`). */
export interface FitCheck {
  id: string;
  label: string;
  /** `after_compile`: checked once compiled, at build. `not_checked`: an earlier check stopped it. */
  status: 'pass' | 'fail' | 'not_checked' | 'after_compile';
}
/** The replacement's own numbers (Rust `motion::Summary`). */
export interface ClipSummary { skeleton: string; frames: number; duration: number; tracks: number; events: number; additive: boolean }
/** Whether a replacement file fits the game's clip (Rust `motion::FitReport`): `fits` is false exactly when
 * the build refuses the file, and `reason` is then the build's own message. */
export interface FitReport {
  kind: 'compiled' | 'authored' | 'unreadable' | 'unsupported';
  fits: boolean;
  reason?: string;
  checks: FitCheck[];
  summary?: ClipSummary;
}

/** An event mark on the time bar (Rust `motion::EventMark`): `at` is a fraction of the clip. */
export interface ClipMark { at: number; kind: string; word: string }
/** The played clip of a clip packet (Rust `clip_preview::Played`). */
export interface PlayedClip {
  name: string; sources: string[];
  /** Frames per second of the clip's own frames. */
  fps: number;
  /** Frames in the packet (`clip_frames`, fewer when `cut`). */
  frames: number; segments: number[]; offset: number;
  /** The clip's own length and frame count. */
  duration: number; clip_frames: number;
  additive: boolean;
  /** The standing clip an additive clip is shown over. */
  over?: string;
  blend_may_differ: boolean;
  cut: boolean;
  which: 'game' | 'compiled' | 'authored' | 'made' | 'taken';
  marks: ClipMark[];
}
export interface ClipBone { name: string; parent: number; position: [number, number, number] }
export interface ClipPacket { header: { bones: ClipBone[]; clips: PlayedClip[] }; data: Float32Array }

/** The Motion page's list of the hero's clips (Rust `clips::MotionCatalog`). */
export interface MotionCatalog { hero_path: string; animations: import('../components/AssetCatalog').AnimationEntry[] }
