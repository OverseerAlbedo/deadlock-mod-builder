import { Fragment, useEffect, useLayoutEffect, useMemo, useState } from 'react';
import { useFrame, useThree } from '@react-three/fiber';
import { Vector3 } from 'three';
import './studio.css';
import { useT } from '../i18n';
// i18n-area: common

/** One label on the model: plain text, a short detail (a count), a tooltip, and whether it is the
 * selected one. `bones` / `point` say where it sits: the middle of those bones (rest pose, or the
 * moving pose in a moving view), or a fixed point in model space. */
export interface ModelLabelItem { id: string; text: string; detail?: string; title?: string; active?: boolean; bones?: string[]; point?: [number, number, number] }
/** The labels a view shows, what a click does, and an optional legend line in the corner. */
export interface ModelLabelSet { items: ModelLabelItem[]; select: (id: string) => void; legend?: string; label?: string;
  /** What happens to a label that would overlap one already placed: `push` (the default) moves it
   * down; `hide` places the active labels first and hides any other label that does not fit beside
   * its point on either side (many joints stay readable). */
  crowd?: 'push' | 'hide' }

/** The DOM elements of one view's labels, shared by the overlay (which renders them once) and the
 * tracker inside the canvas (which moves them on every drawn frame by writing their styles, never
 * through React state). */
export class LabelLayer {
  buttons = new Map<string, HTMLElement>();
  dots = new Map<string, HTMLElement>();
  sizes = new Map<string, [number, number]>();
}
export function useLabelLayer() { return useState(() => new LabelLayer())[0]; }

/** The labels over a canvas: absolutely placed buttons in a layer that fills the canvas's parent
 * (which must be positioned). Hidden until the tracker has placed them. Tab order follows `items`. */
export function ModelLabels({layer, set}: {layer: LabelLayer; set: ModelLabelSet}) {
  const t = useT();
  const key = set.items.map(item => `${item.id}:${item.text}:${item.detail ?? ''}`).join('|');
  // Sizes are read once per label set (and again by the tracker while still unknown), never per frame.
  useLayoutEffect(() => {
    layer.sizes.clear();
    for (const [id, element] of layer.buttons) if (element.offsetWidth) layer.sizes.set(id, [element.offsetWidth, element.offsetHeight]);
  }, [layer, key]);
  return <div className="model-labels" role="group" aria-label={set.label ?? t('Labels on the model')}>
    {set.items.map(item => <Fragment key={item.id}>
      <span className="model-label-dot" aria-hidden="true" ref={element => { if (element) layer.dots.set(item.id, element); else layer.dots.delete(item.id); }}/>
      <button type="button" className="model-label" aria-pressed={Boolean(item.active)} title={item.title} onClick={() => set.select(item.id)}
        ref={element => { if (element) layer.buttons.set(item.id, element); else layer.buttons.delete(item.id); }}>
        <span>{item.text}</span>{item.detail && <>{' '}<small>{item.detail}</small></>}
      </button>
    </Fragment>)}
    {set.legend && <span className="model-labels-legend" aria-hidden="true">{set.legend}</span>}
  </div>;
}

type Point = {x: number; y: number};
type Placed = {left: number; top: number};
const clamp = (value: number, low: number, high: number) => Math.max(low, Math.min(high, value));
/** Where each label goes: beside its point (on the side of the view it is on), inside the
 * view (`width` × `height`, 4px margin), pushed down past labels already placed above it so none
 * overlap where that fits. Labels without a point stay hidden (null). Pure, for tests. */
export function placeLabels(points: (Point | null)[], sizes: [number, number][], width: number, height: number, gap = 6,
  /** Optional: hide labels that would overlap instead of pushing them (`first` are placed first and
   * always shown). Without it the result is exactly the pushing layout above. */hide?: {first: boolean[]},
  /** Optional: the side each label was on (`true` = left of its point). A label keeps that side
   * while it fits, instead of the side of the view its point is on, so it does not flip the moment
   * its point crosses the middle (`stickySides` decides when it may change). */sides?: (boolean | undefined)[]): (Placed | null)[] {
  const out: (Placed | null)[] = points.map(() => null);
  const order = points.flatMap((point, i) => point ? [i] : []).sort((a, b) => points[a]!.y - points[b]!.y || a - b);
  const boxes: {l: number; t: number; r: number; b: number}[] = [];
  if (hide) {
    const hits = (l: number, at: number, w: number, h: number) => boxes.some(box => l < box.r + gap && l + w + gap > box.l && at < box.b + gap && at + h + gap > box.t);
    const ranked = [...order.filter(i => hide.first[i]), ...order.filter(i => !hide.first[i])];
    for (const i of ranked) {
      const {x, y} = points[i]!, [w, h] = sizes[i];
      const top = clamp(y - h / 2, 4, Math.max(4, height - h - 4));
      const right = clamp(x + 12, 4, Math.max(4, width - w - 4)), left = clamp(x - 12 - w, 4, Math.max(4, width - w - 4));
      const sides = x < width / 2 ? [left, right] : [right, left];
      const at = sides.find(l => !hits(l, top, w, h)) ?? (hide.first[i] ? sides[0] : undefined);
      if (at === undefined) continue;
      boxes.push({l: at, t: top, r: at + w, b: top + h});
      out[i] = {left: at, top};
    }
    return out;
  }
  for (const i of order) {
    const {x, y} = points[i]!, [w, h] = sizes[i];
    // Points left of the middle get their label on the left, the others on the right; the other
    // side when there is no room.
    const right = x + 12, left0 = x - 12 - w;
    const onLeft = sides?.[i] ?? x < width / 2;
    let left = onLeft ? (left0 >= 4 ? left0 : right) : (right + w <= width - 4 ? right : left0);
    left = clamp(left, 4, Math.max(4, width - w - 4));
    let top = clamp(y - h / 2, 4, Math.max(4, height - h - 4));
    for (let tries = 0; tries <= boxes.length; tries++) {
      const hit = boxes.find(box => left < box.r + gap && left + w + gap > box.l && top < box.b + gap && top + h + gap > box.t);
      if (!hit) break;
      top = hit.b + gap;
    }
    top = clamp(top, 4, Math.max(4, height - h - 4));
    boxes.push({l: left, t: top, r: left + w, b: top + h});
    out[i] = {left, top};
  }
  return out;
}

/** Which side of its point a label is on (`true` = left), with a dead band round the middle of the
 * view: a label changes side only once its point is `band` (a share of the width) past the middle,
 * so turning the model does not make labels flip back and forth. Pure, for tests. */
export function stickySides(points: (Point | null)[], before: (boolean | undefined)[], width: number, band = .08): (boolean | undefined)[] {
  return points.map((point, i) => {
    if (!point) return before[i];
    const was = before[i];
    if (was === undefined) return point.x < width / 2;
    return was ? point.x < width * (.5 + band) : point.x < width * (.5 - band);
  });
}
/** One step of a label gliding toward where it belongs: `rate` is the share of the remaining way
 * covered in this frame; within `near` px it lands. Pure, for tests. */
export function glide(at: Placed, to: Placed, rate: number, near = .4): Placed {
  const dx = to.left - at.left, dy = to.top - at.top;
  if (Math.abs(dx) <= near && Math.abs(dy) <= near) return {left: to.left, top: to.top};
  return {left: at.left + dx * rate, top: at.top + dy * rate};
}
/** How quickly a label follows its place: the time (seconds) in which it covers about 63% of the way. */
const GLIDE_SECONDS = .14;

/** Inside a Canvas: on every drawn frame, projects each label's anchor with the view's camera and
 * moves the label's elements there. Labels glide to their place instead of jumping (a new side, a
 * push past another label, a part that bobs while the character runs), and keep their side of the
 * point until it is clearly past the middle. `anchor` writes the model-space point of a label into `out`
 * (false: no point, the label hides). Works with `frameloop="demand"` (it asks for a frame when the
 * labels change) and `"always"`. */
export function ModelLabelTracker({layer, ids, anchor, hide}: {layer: LabelLayer; ids: string[]; anchor: (id: string, out: Vector3) => boolean;
  /** Optional: hide overlapping labels instead of pushing them; these ids are placed first and always shown (`ModelLabelSet.crowd` 'hide'). */hide?: string[]}) {
  const {invalidate} = useThree();
  const key = ids.join('|');
  // A view drawn on demand draws again only when something asks: ask for a few frames when the
  // labels change, so they land on the settled camera and size even if those arrive a little later.
  useEffect(() => {
    let frames = 0, handle = 0;
    const tick = () => { invalidate(); if (++frames < 30) handle = requestAnimationFrame(tick); };
    tick();
    return () => cancelAnimationFrame(handle);
  }, [key, invalidate]);
  const point = useMemo(() => new Vector3(), []);
  // Where each label is drawn now and which side it is on; kept between frames, forgotten with the label set.
  const memory = useMemo(() => ({shown: new Map<string, Placed>(), sides: new Map<string, boolean | undefined>()}), [key]);
  useFrame(({camera, size}, delta) => {
    const points = ids.map(id => {
      if (!anchor(id, point)) return null;
      point.project(camera);
      if (!(point.z > -1 && point.z < 1) || Math.abs(point.x) > 1.2 || Math.abs(point.y) > 1.2) return null;
      return {x: (point.x + 1) / 2 * size.width, y: (1 - point.y) / 2 * size.height};
    });
    const sizes = ids.map((id): [number, number] => {
      let known = layer.sizes.get(id);
      const element = layer.buttons.get(id);
      if ((!known || !known[0]) && element?.offsetWidth) { known = [element.offsetWidth, element.offsetHeight]; layer.sizes.set(id, known); }
      return known ?? [0, 0];
    });
    const sides = stickySides(points, ids.map(id => memory.sides.get(id)), size.width);
    ids.forEach((id, i) => memory.sides.set(id, sides[i]));
    const placed = placeLabels(points, sizes, size.width, size.height, 6, hide ? {first: ids.map(id => hide.includes(id))} : undefined, hide ? undefined : sides);
    // The share of the way a label covers this frame; a long pause between frames lands it at once.
    const rate = delta > .25 ? 1 : 1 - Math.exp(-Math.max(delta, 1 / 240) / GLIDE_SECONDS);
    let moving = false;
    ids.forEach((id, i) => {
      const button = layer.buttons.get(id), dot = layer.dots.get(id), to = placed[i], p = points[i];
      if (!to) memory.shown.delete(id);
      // A label that was hidden appears at its place; one that is shown glides there.
      const before = to && memory.shown.get(id);
      const at = to && (before ? glide(before, to, rate) : to);
      if (at && to) { memory.shown.set(id, at); if (at.left !== to.left || at.top !== to.top) moving = true; }
      if (button) { button.style.visibility = at ? 'visible' : 'hidden'; if (at) button.style.transform = `translate(${at.left.toFixed(1)}px, ${at.top.toFixed(1)}px)`; }
      if (dot) { dot.style.visibility = at && p ? 'visible' : 'hidden'; if (p) dot.style.transform = `translate(${Math.round(p.x - 3.5)}px, ${Math.round(p.y - 3.5)}px)`; }
    });
    // A view drawn on demand would stop mid-glide: ask for the next frame until every label has landed.
    if (moving) invalidate();
  });
  return null;
}

/** The middle of the named bones' positions (`positions` maps a bone name to its point), or the
 * item's fixed point: the rest-pose anchors of a still view. */
export function restAnchors(items: ModelLabelItem[], positions: Map<string, [number, number, number]>): Map<string, Vector3> {
  const out = new Map<string, Vector3>();
  for (const item of items) {
    if (item.point) { out.set(item.id, new Vector3(...item.point)); continue; }
    const found = (item.bones ?? []).map(bone => positions.get(bone)).filter((at): at is [number, number, number] => Boolean(at));
    if (!found.length) continue;
    const sum = new Vector3();
    for (const at of found) sum.add(new Vector3(...at));
    out.set(item.id, sum.multiplyScalar(1 / found.length));
  }
  return out;
}
