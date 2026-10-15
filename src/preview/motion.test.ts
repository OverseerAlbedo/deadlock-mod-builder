import { describe, expect, it } from 'vitest';
import { MotionRig, sampleSkins, travel, tune, turnAbout, type MotionHeader, type MotionPacket, type PreviewNode } from './motion';

const identity = [1,0,0,0, 0,1,0,0, 0,0,1,0];
const near = (a:number[], b:number[], e = 1e-6) => a.every((v,i) => Math.abs(v-b[i]) < e);

describe('Movement preview', () => {
  it('fine-tunes like the build (same numbers as the Rust test)', () => {
    const auto = {goal:.4,drag:.1,gravity:1,travel:2};
    expect(tune(auto, {})).toEqual(auto);
    const most = tune(auto, {stiffness:1,damping:1,gravity:1,swing:1});
    expect([most.goal,most.drag,most.gravity,most.travel]).toEqual([1,.5,1.5,5]);
    const least = tune({...auto,gravity:0}, {stiffness:-1,damping:-1,gravity:-1,swing:-1});
    expect(near([least.goal,least.drag,least.gravity,least.travel],[.1,0,0,.5])).toBe(true);
    expect(tune({goal:.4,drag:0,gravity:1,travel:0}, {swing:-1}).travel).toBe(0);
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
