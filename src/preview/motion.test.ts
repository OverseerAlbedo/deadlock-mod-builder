/// <reference types="node" />
import { existsSync, readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { MOTION_LAYOUT, MotionRig, capTravel, decodeMotion, sampleSkins, travel, tune, turnAbout, type MotionHeader, type MotionPacket, type PreviewNode } from './motion';
import { decodePreview, floatBuffer } from './packet';

const identity = [1,0,0,0, 0,1,0,0, 0,0,1,0];
const near = (a:number[], b:number[], e = 1e-6) => a.every((v,i) => Math.abs(v-b[i]) < e);
/** A packet's bytes: u32 header length, the header as JSON padded to 4 bytes, then the floats. */
function packetBytes(header:unknown, floats:number[]):ArrayBuffer {
  const json = new TextEncoder().encode(JSON.stringify(header)), start = Math.ceil((4+json.length)/4)*4;
  const bytes = new ArrayBuffer(start + floats.length*4), view = new DataView(bytes);
  view.setUint32(0, json.length, true);
  new Uint8Array(bytes, 4, json.length).set(json);
  for (let k = 4+json.length; k < start; k++) view.setUint8(k, 32);
  new Float32Array(bytes, start).set(floats);
  return bytes;
}
const fileBytes = (path:string) => { const b = readFileSync(path), a = new ArrayBuffer(b.length); new Uint8Array(a).set(b); return a; };
const moved = (x:number) => [1,0,0,x, 0,1,0,0, 0,0,1,0];

describe('Movement packet layout', () => {
  // Bones: root (stored), arm under it (stored), a tip under the arm (not stored), a loose root (not stored).
  const bones = [{name:'root',parent:-1,position:[0,0,0]},{name:'arm',parent:0,position:[0,0,1]},{name:'tip',parent:1,position:[0,0,2]},{name:'loose',parent:-1,position:[0,0,3]}];
  const stored = [moved(1),moved(2), moved(3),moved(4), moved(5),moved(6)].flat();
  const header = {bones, layout:{version:MOTION_LAYOUT,stored:[0,1],frames:3}, clips:[
    {name:'idle',sources:['a'],fps:60,frames:2,segments:[2],runs:[[0,2]]},
    // A stop: the run's frames again, then a frame of its own.
    {name:'stop',sources:['a','b'],fps:60,frames:4,segments:[2,2],runs:[[0,2],[2,1],[0,1]]},
  ]};
  it('plays every clip frame by frame from the stored frames, in the full layout', () => {
    const {header:h, data} = decodeMotion(packetBytes(header, stored));
    expect(h.layout).toBeUndefined();
    expect(h.clips.map(c => [c.name, c.offset, c.frames, 'runs' in c])).toEqual([['idle',0,2,false],['stop',2*4*12,4,false]]);
    expect(data.length).toBe(6*4*12);
    const at = (clip:number, f:number, b:number) => [...data.subarray(h.clips[clip].offset + (f*4 + b)*12, h.clips[clip].offset + (f*4 + b)*12 + 12)];
    expect(at(0,1,1)).toEqual(moved(4));
    expect([0,1,2,3].map(f => at(1,f,0)[3])).toEqual([1,3,5,1]);
    // A bone not stored takes its nearest stored ancestor's matrix; without one, identity.
    expect(at(1,2,2)).toEqual(moved(6));
    expect(at(1,2,3)).toEqual(identity);
    // sampleSkins reads the expanded frames as before.
    expect(sampleSkins({header:h,data}, h.clips[1], 2/60, new Float32Array(48))[15]).toBe(6);
  });
  it('refuses a stored packet whose runs or frames do not add up', () => {
    expect(() => decodeMotion(packetBytes(header, stored.slice(0, 24)))).toThrow('Truncated');
    expect(() => decodeMotion(packetBytes({...header, clips:[{...header.clips[0], runs:[[2,2]]}]}, stored))).toThrow('Invalid');
    expect(() => decodeMotion(packetBytes({...header, clips:[{...header.clips[0], frames:3}]}, stored))).toThrow('Invalid');
    expect(() => decodeMotion(packetBytes({...header, layout:{...header.layout, version:3}}, stored))).toThrow('Unknown');
    expect(() => decodeMotion(packetBytes({...header, layout:{...header.layout, stored:[0,9]}}, stored))).toThrow('Invalid');
  });
  it('still reads a packet in the full layout', () => {
    const full = {bones:bones.slice(0,1), clips:[{name:'idle',sources:[],fps:60,frames:2,segments:[2],offset:0}]};
    const {header:h, data} = decodeMotion(packetBytes(full, [...moved(1), ...moved(2)]));
    expect(h.clips[0].offset).toBe(0);
    expect(data[15]).toBe(2);
  });
  // Real packets of one character in both layouts (the stored and the full layout of
  // motion_preview's movement packet), its scene packet, cloth packet and analyzed moving bones: entries
  // `stored.bin|full.bin|scene.dmbp|preview-cloth.json|preview-nodes.json` separated by `;` (the
  // last three may be empty).
  const cases = (process.env.DMB_MOVEMENT_LAYOUT_CASES ?? '').split(';').filter(Boolean).map(c => c.split('|'));
  it.skipIf(!cases.length || !cases.every(c => existsSync(c[0]) && existsSync(c[1])))('plays every motion of real packets exactly as the full layout, for every bone anything reads', () => {
    for (const [storedPath, fullPath, scenePath, clothPath, nodesPath] of cases) {
      const length = new DataView(fileBytes(storedPath)).getUint32(0, true);
      const raw = JSON.parse(new TextDecoder().decode(new Uint8Array(fileBytes(storedPath), 4, length))) as MotionHeader;
      const kept = new Set(raw.layout!.stored);
      const a = decodeMotion(fileBytes(storedPath)), b = decodeMotion(fileBytes(fullPath));
      expect(a.header).toEqual(b.header);
      const n = a.header.bones.length, index = new Map(a.header.bones.map((x, i) => [x.name, i]));
      // Every bone the scene skins, the cloth packet names and the moving bones use is stored.
      const read = new Set<number>();
      if (scenePath) {
        const scene = decodePreview(fileBytes(scenePath));
        for (const mesh of scene.metadata.meshes) {
          if (!mesh.skin) continue;
          const skin = floatBuffer(scene, mesh.skin.data);
          for (let k = 0; k < skin.length; k += 2) if (skin[k+1] > 1e-4) read.add(index.get(mesh.skin.joint_names[Math.round(skin[k])])!);
        }
      }
      if (clothPath) for (const bone of (JSON.parse(readFileSync(clothPath, 'utf8')) as {bones:{name:string}[]}).bones) read.add(index.get(bone.name)!);
      if (nodesPath) for (const node of JSON.parse(readFileSync(nodesPath, 'utf8')) as PreviewNode[]) read.add(index.get(node.bone)!);
      expect([...read].filter(b => b === undefined || !kept.has(b))).toEqual([]);
      for (const clip of a.header.clips) {
        let differ = 0;
        for (let f = 0; f < clip.frames; f++) for (const bone of kept) for (let k = 0; k < 12; k++) {
          const at = clip.offset + (f*n + bone)*12 + k;
          if (!Object.is(a.data[at], b.data[at])) differ++;
        }
        expect(differ, `${storedPath} ${clip.name}`).toBe(0);
      }
      console.log(`${storedPath}: ${kept.size} of ${n} bones stored, ${raw.layout!.frames} of ${a.header.clips.reduce((s, c) => s + c.frames, 0)} frames; ${read.size} bones read by the scene, cloth and moving bones, all stored`);
    }
  });
});

describe('Movement preview', () => {
  it('fine-tunes like the build (same numbers as the Rust test)', () => {
    const auto = {goal:.4,drag:.1,gravity:1,travel:2};
    expect(tune(auto, {})).toEqual(auto);
    const most = tune(auto, {stiffness:1,damping:1,gravity:1,swing:1});
    expect([most.goal,most.drag,most.gravity,most.travel]).toEqual([1,.5,1.5,5]);
    const least = tune({...auto,gravity:0}, {stiffness:-1,damping:-1,gravity:-1,swing:-1});
    expect(near([least.goal,least.drag,least.gravity,least.travel],[.1,0,0,.5])).toBe(true);
    expect(tune({goal:.4,drag:0,gravity:1,travel:0}, {swing:-1}).travel).toBe(0);
    // A breast's travel cap holds after Fine-tune (same numbers as the Rust test).
    expect(capTravel(tune(auto, {swing:1}).travel, 3)).toBe(3);
    expect(capTravel(1, 3)).toBe(1);
    expect(capTravel(5)).toBe(5);
    expect(capTravel(0, 3)).toBe(0);
  });
  it('turns a direction about a pivot', () => {
    const m = turnAbout([1,1,0],[1,0,0],[0,1,0]);
    const p = [2,1,0], moved = [0,1,2].map(r => m[r*4]*p[0]+m[r*4+1]*p[1]+m[r*4+2]*p[2]+m[r*4+3]);
    expect(near(moved,[1,2,0])).toBe(true);
  });
  it('blends between frames so slow motion stays smooth', () => {
    const packet:MotionPacket = {header:{bones:[{name:'root',parent:-1,position:[0,0,0]}],clips:[{name:'run',sources:[],fps:30,frames:2,segments:[2],offset:0}]},
      data:new Float32Array([...identity, 1,0,0,10, 0,1,0,0, 0,0,1,0])};
    const out = sampleSkins(packet, packet.header.clips[0], .5/30, new Float32Array(12));
    expect(out[3]).toBeCloseTo(5);
    // The last frame holds to the end of the clip (no blend back into the first).
    expect(sampleSkins(packet, packet.header.clips[0], 1.5/30, new Float32Array(12))[3]).toBeCloseTo(10);
  });
  // A body bone and a 10-inch strand: hanging down, or sticking out forward.
  const strand = (tip:[number,number,number]):MotionHeader => ({bones:[{name:'body',parent:-1,position:[0,0,50]},{name:'strand',parent:0,position:[0,0,50]},{name:'tip',parent:1,position:tip}],clips:[]});
  const hanging = strand([0,0,40]), forward = strand([10,0,50]);
  const frame = () => [...identity,...identity,...identity];
  const node = (values:Partial<PreviewNode>):PreviewNode => ({bone:'strand',root:'strand',kind:'hair',goal:.5,drag:.1,gravity:0,travel:0,...values});
  const tipOf = (skins:Float32Array, rest:number[]) => [0,1,2].map(r => skins[24+r*4]*rest[0]+skins[24+r*4+1]*rest[1]+skins[24+r*4+2]*rest[2]+skins[24+r*4+3]);
  it('follows the animation exactly when the part holds its pose', () => {
    const rig = new MotionRig(hanging,[node({goal:1})],{});
    let out = rig.step(frame(),[0,0,0],1/30);
    for (let i = 0; i < 20; i++) out = rig.step(frame(),[i*5,0,0],1/30);
    expect(near(Array.from(out),frame(),1e-4)).toBe(true);
  });
  it('lets a loose part trail when running, overshoot and settle, and sag with gravity', () => {
    // Standing, then a quarter second into a run (the start is a kick, delivered over a few frames) the strand lags behind the body.
    const run = (rig:MotionRig) => { rig.step(frame(),[0,0,0],1/30); let out = rig.step(frame(),[0,0,0],1/30); for (let i = 1; i <= 8; i++) out = rig.step(frame(),[i*260/30,0,0],1/30); return tipOf(out,[0,0,40]); };
    const trailing = run(new MotionRig(hanging,[node({goal:.02,drag:.05})],{}));
    expect(trailing[0]).toBeLessThan(-1);
    expect(Math.abs(Math.hypot(trailing[0],trailing[1],trailing[2]-50)-10)).toBeLessThan(1e-3);
    // Stiffer through Fine-tune: the same run trails less.
    const stiff = run(new MotionRig(hanging,[node({goal:.02,drag:.05})],{hair:{stiffness:1}}));
    expect(stiff[0]).toBeGreaterThan(trailing[0]);
    // A springy part pulled back after a shove swings past its pose before it settles.
    const springy = new MotionRig(hanging,[node({goal:.45,drag:.1})],{});
    springy.step(frame(),[0,0,0],1/30); springy.step(frame(),[0,0,0],1/30);
    const xs:number[] = [];
    for (let i = 1; i <= 90; i++) { const o = springy.step(frame(),[i < 4 ? i*8 : 24,0,0],1/30); xs.push(tipOf(o,[0,0,40])[0]); }
    expect(Math.min(...xs)).toBeLessThan(-.5);
    expect(Math.max(...xs.slice(6))).toBeGreaterThan(.2);
    expect(Math.abs(xs[xs.length-1])).toBeLessThan(.3);
    // Under gravity a forward strand falls through the bottom of its swing.
    const heavy = new MotionRig(forward,[node({goal:.02,gravity:1})],{});
    let lowest = 50;
    for (let i = 0; i < 60; i++) lowest = Math.min(lowest, tipOf(heavy.step(frame(),[0,0,0],1/30),[10,0,50])[2]);
    expect(lowest).toBeLessThan(43);
  });
  it('hangs steady during a run instead of streaming back like in a wind', () => {
    // A hanging strand under gravity: three seconds into a steady run it hangs
    // where it hangs when standing (the old world-space drag pinned it back).
    const settle = (moving:boolean) => {
      const rig = new MotionRig(hanging,[node({goal:.3,drag:.2,gravity:1})],{});
      let out = rig.step(frame(),[0,0,0],1/60);
      for (let i = 1; i <= 180; i++) out = rig.step(frame(),[moving ? i*260/60 : 0,0,0],1/60);
      return tipOf(out,[0,0,40]);
    };
    const running = settle(true), standing = settle(false);
    expect(Math.abs(running[0]-standing[0])).toBeLessThan(.3);
    expect(Math.abs(running[2]-standing[2])).toBeLessThan(.3);
  });
  it('kicks the parts down at a landing, then lets them settle', () => {
    const jump = {name:'jump',sources:[],fps:60,frames:180,segments:[30,36,60],offset:0};
    const rig = new MotionRig(forward,[node({goal:.5,drag:.2,gravity:0})],{});
    const zs:number[] = [];
    for (let i = 0; i <= 179; i++) zs.push(tipOf(rig.step(frame(),travel(jump,i/60),i?1/60:0),[10,0,50])[2]);
    const landing = Math.round((30+36+.1*60)/1)+1;
    // In the air the strand floats up a little (never pinned straight up: the arc
    // counts for one gravity at most); just after touching down it has swung
    // down; later it is back level.
    expect(Math.max(...zs.slice(40,landing))).toBeGreaterThan(51);
    expect(Math.max(...zs.slice(40,landing))).toBeLessThan(58);
    expect(Math.min(...zs.slice(landing,landing+20))).toBeLessThan(48);
    expect(Math.abs(zs[179]-50)).toBeLessThan(.5);
  });
  it('treats a clip looping round as a teleport, not a kick', () => {
    const rig = new MotionRig(hanging,[node({goal:.3,drag:.2,gravity:1})],{});
    let out = rig.step(frame(),[0,0,0],1/60);
    for (let i = 1; i <= 60; i++) out = rig.step(frame(),[i*260/60,0,0],1/60);
    const before = tipOf(out,[0,0,40]);
    const after = tipOf(rig.step(frame(),[0,0,0],1/60),[0,0,40]);
    expect(Math.hypot(after[0]-before[0],after[1]-before[1],after[2]-before[2])).toBeLessThan(.3);
  });
  it('moves the character forward when running and up between take-off and landing', () => {
    expect(travel({name:'run',sources:[],fps:30,frames:16,segments:[16],offset:0},.5)[0]).toBe(130);
    const jump = {name:'jump',sources:[],fps:30,frames:60,segments:[20,20,20],offset:0};
    expect(travel(jump,.1)[2]).toBe(0);
    expect(travel(jump,1.0)[2]).toBeGreaterThan(40);
    expect(travel(jump,1.9)[2]).toBe(0);
  });
});
