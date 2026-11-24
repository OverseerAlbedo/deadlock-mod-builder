/** The character the Colors page shows in 3D: the game hero's own model (game hero mode) or
 * the prepared replacement model. Each is loaded once per model and kept while the app runs. */
import { useEffect, useState } from "react";
import { api } from "../api";
import { previewError } from "./media";
import type { Project } from "../models";
import { decodePreview, type PreviewMesh, type PreviewScene } from "../preview/packet";

/** A hero scene mesh: the preview packet's mesh plus where it comes from in the game model. */
export interface HeroSceneMesh extends PreviewMesh {
  /** The game material, as the catalog's `HeroMaterialEntry.target` (`.vmat_c`). */
  target?: string;
  node?: string;
  body_group?: string | null;
  /** Not shown in game by default (another body group choice, a LOD …). */
  default_hidden?: boolean;
  hidden_reason?: string | null;
  /** A part of the weapon, decided in Rust from the game's skin weights (game hero) or from
   * the preparation receipt's fitted gun (replacement model). Absent when false. */
  weapon?: boolean;
}
/** Rust's reason for hiding a gun that is not held (`catalogs.rs scene::GUN_AWAY`, a Rust test
 * keeps them equal). With the Weapon filter on, weapon parts hidden for this reason are drawn;
 * weapon parts hidden as effect layers (glass, glow) stay hidden. */
export const GUN_AWAY_REASON = "gun away from the hand (placed by animation)";
export interface MaterialGroup { name: string; remaps: { from: string; to: string }[] }
export type SceneStatus = "idle" | "loading" | "ready" | "failed";
export interface SceneState { status: SceneStatus; scene: PreviewScene | null; message: string }

export const HERO_SCENE_UNAVAILABLE = "This version of the app cannot show the hero's own model yet.";
/** The hero's own model as a preview packet (decompiled from the game the first time).
 * `api.heroScene` (`hero_scene`); a backend without it fails with HERO_SCENE_UNAVAILABLE and the
 * page falls back to the Texture tab, as for any failure. */
export async function loadHeroScene(project: Project): Promise<ArrayBuffer> {
  const native = api as unknown as { heroScene?: (project: Project) => Promise<ArrayBuffer>; discoverExtras?: (project: Project) => Promise<unknown> };
  if (!native.heroScene) throw new Error(HERO_SCENE_UNAVAILABLE);
  // The scene reads the Colors catalog discover_extras caches and takes the operation lease:
  // discover first, then ask for the scene (never both at once).
  await native.discoverExtras?.(project);
  return native.heroScene(project);
}

const scenes = new Map<string, Promise<PreviewScene>>();
/** Forget a scene so the next request loads it again (Try again, Reload assets). */
export function forgetScene(key: string) { scenes.delete(key); }
function sceneFor(key: string, load: () => Promise<ArrayBuffer>): Promise<PreviewScene> {
  let known = scenes.get(key);
  if (!known) {
    known = Promise.resolve().then(load).then(decodePreview);
    known.catch(() => scenes.delete(key));
    scenes.set(key, known);
    while (scenes.size > 3) scenes.delete(scenes.keys().next().value!);
  }
  return known;
}
/** The scene for `key` (null: none to show), loaded only while `enabled`. */
export function useScene(key: string | null, load: () => Promise<ArrayBuffer>, enabled: boolean, attempt = 0): SceneState {
  const [state, setState] = useState<SceneState & { key: string | null }>({ key: null, status: "idle", scene: null, message: "" });
  useEffect(() => {
    if (!key || !enabled) return;
    let live = true;
    setState((current) => (current.key === key && current.status === "ready" ? current : { key, status: "loading", scene: null, message: "" }));
    sceneFor(key, load).then(
      (scene) => { if (live) setState({ key, status: "ready", scene, message: "" }); },
      (error) => { if (live) setState({ key, status: "failed", scene: null, message: previewError(error) }); },
    );
    return () => { live = false; };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- `load` is keyed by `key`
  }, [key, enabled, attempt]);
  return state.key === key ? state : { status: key && enabled ? "loading" : "idle", scene: null, message: "" };
}
/** Reload assets (`revision`) asks for the hero again. */
export const heroSceneKey = (project: Project, revision = 0) => (project.original_model ? `hero:${project.original_model}:${revision}` : null);
export const replacementSceneKey = (project: Project) => (project.dmx ? `dmx:${project.dmx}` : null);
/** The prepared model, with its fitted gun's meshes marked `weapon` (`replacement_color_scene`);
 * a backend without that command gives the plain preview, whose meshes carry no mark. */
export const loadReplacementScene = (project: Project) => () => {
  const native = api as unknown as { replacementColorScene?: (project: Project) => Promise<ArrayBuffer> };
  return native.replacementColorScene ? native.replacementColorScene(project) : api.preview(project.dmx);
};
