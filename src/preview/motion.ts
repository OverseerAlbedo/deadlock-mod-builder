/** Movement preview: the hero's stand, run and jump clips on the prepared
 * character, with the moving parts simulated from the values Analyze recorded
 * (secondary motion `preview_nodes`) and the live Fine-tune sliders. An
 * approximation of the game's cloth solver for tuning by eye; the game decides. */

export type Vec3 = [number, number, number];
export interface MotionBone {name:string;parent:number;position:Vec3}
export interface MotionClip {name:string;sources:string[];fps:number;frames:number;segments:number[];offset:number}
export interface MotionHeader {bones:MotionBone[];clips:MotionClip[]}
export interface MotionPacket {header:MotionHeader;data:Float32Array}
export interface PreviewNode {bone:string;root:string;kind:string;goal:number;drag:number;gravity:number;travel:number}
export type Tuning = Partial<Record<'stiffness'|'damping'|'gravity'|'swing', number>>;

export function decodeMotion(buffer:ArrayBuffer):MotionPacket {
  if(!(buffer instanceof ArrayBuffer)||buffer.byteLength<4)throw new Error('Invalid movement preview packet.');
  const length=new DataView(buffer).getUint32(0,true);
  if(length>16*1024*1024||4+length>buffer.byteLength)throw new Error('Truncated movement preview header.');
  const header=JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(new Uint8Array(buffer,4,length))) as MotionHeader;
  const start=Math.ceil((4+length)/4)*4;
  const data=new Float32Array(buffer,start,(buffer.byteLength-start)/4);
  const bones=header.bones?.length??0;
  for(const clip of header.clips??[]){if(clip.offset+clip.frames*bones*12>data.length)throw new Error('Truncated movement preview frames.');}
  return {header,data};
}

/** The rest-to-pose skins of a clip at `time` (seconds, looping), blended between
 * the two nearest frames so slow motion stays smooth. */
export function sampleSkins(packet:MotionPacket,clip:MotionClip,time:number,out:Float32Array):Float32Array{
  const bones=packet.header.bones.length,stride=bones*12;
  const f=((time*clip.fps)%clip.frames+clip.frames)%clip.frames;
  const i0=Math.floor(f),a=f-i0,i1=i0+1<clip.frames?i0+1:i0;
  const s0=clip.offset+i0*stride,s1=clip.offset+i1*stride,d=packet.data;
  for(let k=0;k<stride;k++)out[k]=d[s0+k]*(1-a)+d[s1+k]*a;
  return out;
}

/** Fine-tune on top of a node's automatic values; identical to Rust `secondary_auto::tune`. */
export function tune(node:{goal:number;drag:number;gravity:number;travel:number},tuning?:Tuning){
  const k=(v:number|undefined)=>Math.min(1,Math.max(-1,v??0));
  const toward=(value:number,amount:number,high:number,lowShare:number)=>amount>=0?value+Math.max(high-value,0)*amount:value*(1+(1-lowShare)*amount);
  const s=k(tuning?.swing);
  return {
    goal:Math.min(1,Math.max(.01,toward(node.goal,k(tuning?.stiffness),1,.25))),
    drag:Math.min(1,Math.max(0,toward(node.drag,k(tuning?.damping),.5,0))),
    gravity:Math.max(0,toward(node.gravity,k(tuning?.gravity),1.5,0)),
    travel:node.travel>0?node.travel*(s>=0?1+1.5*s:1+.75*s):node.travel,
  };
}

const apply=(m:ArrayLike<number>,p:Vec3,o=0):Vec3=>[0,1,2].map(r=>m[o+r*4]*p[0]+m[o+r*4+1]*p[1]+m[o+r*4+2]*p[2]+m[o+r*4+3]) as Vec3;
const sub=(a:Vec3,b:Vec3):Vec3=>[a[0]-b[0],a[1]-b[1],a[2]-b[2]];
const add=(a:Vec3,b:Vec3):Vec3=>[a[0]+b[0],a[1]+b[1],a[2]+b[2]];
const scale=(a:Vec3,s:number):Vec3=>[a[0]*s,a[1]*s,a[2]*s];
const len=(a:Vec3)=>Math.hypot(a[0],a[1],a[2]);
const cross=(a:Vec3,b:Vec3):Vec3=>[a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]];
/** Row-major 3x4 a∘b (b first). */
function compose(a:ArrayLike<number>,b:ArrayLike<number>,out:Float32Array|number[],o=0){
  for(let r=0;r<3;r++){for(let c=0;c<4;c++){out[o+r*4+c]=a[r*4]*b[c]+a[r*4+1]*b[4+c]+a[r*4+2]*b[8+c]+(c===3?a[r*4+3]:0);}}
}
/** The rotation taking direction u to v, about `pivot`, as a 3x4. */
export function turnAbout(pivot:Vec3,u:Vec3,v:Vec3):number[]{
  const lu=len(u),lv=len(v);
  if(lu<1e-9||lv<1e-9)return [1,0,0,0,0,1,0,0,0,0,1,0];
  const a=scale(u,1/lu),b=scale(v,1/lv),axis=cross(a,b),s=len(axis),c=a[0]*b[0]+a[1]*b[1]+a[2]*b[2];
  let m:number[];
  if(s<1e-9){m=c>0?[1,0,0,0,1,0,0,0,1]:[-1,0,0,0,-1,0,0,0,1];}
  else{const [x,y,z]=scale(axis,1/s),t=1-c;m=[t*x*x+c,t*x*y-s*z,t*x*z+s*y,t*x*y+s*z,t*y*y+c,t*y*z-s*x,t*x*z-s*y,t*y*z+s*x,t*z*z+c];}
  const r=[m[0],m[1],m[2],0,m[3],m[4],m[5],0,m[6],m[7],m[8],0];
  const moved=apply(r,pivot);
  r[3]=pivot[0]-moved[0];r[7]=pivot[1]-moved[1];r[11]=pivot[2]-moved[2];
  return r;
}

/** Inches per second² of gravity; the pose pull at full strength as a spring
 * frequency (rad/s), and the least damping ratio: the game's solver damps a
 * good deal on its own (hair with no drag and no gravity of its own still
 * settles within a swing or two in game), so a part never rings freely here. */
const GRAVITY=386.09, OMEGA_MAX=22, MIN_ZETA=.55, PHYSICS_HZ=120;
/** A travel jump beyond this in one frame is a teleport (a clip looping round),
 * not a kick. A kick (take-off, landing) reaches the parts over the next frames
 * at up to `KICK_RATE` (knees bend, the body does not stop in one frame), and
 * never more than `KICK_MAX` of speed is owed at once. */
const TELEPORT=60, KICK_MAX=300, KICK_RATE=3*GRAVITY;
interface SimNode {bone:number;tip:Vec3;length:number;goal:number;drag:number;gravity:number;travel:number;p:Vec3;v:Vec3;pivotBefore:Vec3|null;tipBefore:Vec3|null}
const lerp=(a:Vec3,b:Vec3,f:number):Vec3=>[a[0]+(b[0]-a[0])*f,a[1]+(b[1]-a[1])*f,a[2]+(b[2]-a[2])*f];

/** The prepared skeleton with its moving parts. `tips` gives the rest-pose
 * point a single bone swings (the middle of a breast, a charm); a bone with a
 * child swings toward that child. Each moving bone's swing point is a spring
 * (the pose pull) on a pendulum (its length from the bone's head, which follows
 * the parent's own swing), with drag, gravity and the travel limit; children
 * follow their parent's swing. */
export class MotionRig {
  readonly bones:MotionBone[];
  private nodes:SimNode[]=[];
  private depthOrder:number[];
  private fresh=true;
  private frame:{offset:Vec3;velocity:Vec3|null}|null=null;
  private owed:Vec3=[0,0,0];
  constructor(header:MotionHeader,nodes:PreviewNode[],tuning:Record<string,Tuning|undefined>,tips:Map<number,Vec3>=new Map()){
    this.bones=header.bones;
    const index=new Map(this.bones.map((b,i)=>[b.name,i]));
    const depth=(i:number)=>{let d=0;for(let at=this.bones[i].parent;at>=0;at=this.bones[at].parent)d++;return d;};
    this.depthOrder=this.bones.map((_,i)=>i).sort((a,b)=>depth(a)-depth(b));
    const moving=new Set(nodes.map(n=>index.get(n.bone)).filter((i):i is number=>i!==undefined));
    for(const node of nodes){
      const bone=index.get(node.bone);if(bone===undefined)continue;
      const rest=this.bones[bone].position;
      // The point the bone swings: a moving child, any child away from it, the
      // middle of its mesh (a breast), or a short way on from its parent.
      const away=(i:number)=>len(sub(this.bones[i].position,rest))>.05;
      const child=[this.bones.findIndex((b,i)=>b.parent===bone&&moving.has(i)&&away(i)),this.bones.findIndex((b,i)=>b.parent===bone&&away(i))].find(i=>i>=0)??-1;
      const parent=this.bones[bone].parent;
      let tip=child>=0?this.bones[child].position:tips.get(bone);
      if(!tip){let back=parent>=0?sub(rest,this.bones[parent].position):[0,0,-1] as Vec3;if(len(back)<.05)back=[0,0,-1];const l=len(back);tip=add(rest,scale(back,Math.min(Math.max(l*.5,1),8)/l));}
      const values=tune(node,tuning[node.kind]);
      this.nodes.push({bone,tip,length:Math.max(len(sub(tip,rest)),1e-3),...values,p:[0,0,0],v:[0,0,0],pivotBefore:null,tipBefore:null});
    }
    const order=new Map(this.depthOrder.map((b,i)=>[b,i]));
    this.nodes.sort((a,b)=>(order.get(a.bone)??0)-(order.get(b.bone)??0));
  }
  get moving(){return this.nodes.length;}
  /** Start again from the animated pose (a new clip, or after a pause). */
  reset(){this.fresh=true;}
  /** The character's travel as the inertia its parts feel: the frame's change of
   * speed (a take-off, a landing), never its steady speed, so a run does not
   * blow the parts back like a wind. A jump of the offset beyond `TELEPORT` is
   * a clip looping round, not a kick. */
  private inertia(offset:Vec3,dt:number):Vec3{
    const before=this.frame;
    if(this.fresh||dt<=0||!before){this.frame={offset,velocity:null};this.owed=[0,0,0];return [0,0,0];}
    const moved=sub(offset,before.offset);
    if(len(moved)>TELEPORT){this.frame={offset,velocity:before.velocity};return [0,0,0];}
    const velocity=scale(moved,1/dt);
    // The first measured speed is the clip's own (a run is already under way):
    // no kick for starting it.
    if(!before.velocity){this.frame={offset,velocity};return [0,0,0];}
    const change=sub(velocity,before.velocity),k=len(change);
    this.frame={offset,velocity};
    let push:Vec3=[0,0,0];
    // A smooth arc pushes the parts by at most one gravity (free fall leaves them
    // weightless, no more); a sudden change of speed (take-off, landing) is owed
    // to the parts and delivered over the next frames.
    if(k/dt<=2*GRAVITY)push=k/dt>GRAVITY?scale(change,GRAVITY*dt/k):change;
    else{this.owed=add(this.owed,change);const total=len(this.owed);if(total>KICK_MAX)this.owed=scale(this.owed,KICK_MAX/total);}
    const owed=len(this.owed);
    if(owed>1e-6){const part=Math.min(owed,KICK_RATE*dt),piece=scale(this.owed,part/owed);this.owed=sub(this.owed,piece);push=add(push,piece);}
    return scale(push,-1/dt);
  }
  /** One frame: `skins` holds the animated rest-to-pose 3x4 of every bone; `offset`
   * is how far the character has travelled in the world (run, jump), which the
   * moving parts feel. Returns the skins with the moving parts swung, for display.
   * The parts are simulated in the character's own space: the bone's head carries
   * each part along (the animated pose and the ancestors' swings move it between
   * frames, smoothly across the substeps), drag acts on the swing relative to that
   * head, and the travel's changes of speed push the parts the other way. */
  step(skins:ArrayLike<number>,offset:Vec3,dt:number,simulate=true):Float32Array{
    const count=this.bones.length,out=new Float32Array(count*12);
    for(let i=0;i<count*12;i++)out[i]=skins[i];
    if(!simulate||!this.nodes.length)return out;
    const delta:Array<ArrayLike<number>|null>=new Array(count).fill(null);
    const span=Math.min(dt,.1),substeps=Math.max(1,Math.ceil(span*PHYSICS_HZ)),h=span/substeps;
    const inertia=this.inertia(offset,dt);
    for(const node of this.nodes){
      const b=node.bone,rest=this.bones[b].position;
      const inherited=this.inherited(b,delta);
      const head=apply(skins,rest,b*12),tip=apply(skins,node.tip,b*12);
      const pivot=inherited?apply(inherited,head):head;
      const bent=inherited?apply(inherited,tip):tip;
      const still=this.fresh||dt<=0||node.goal>=.98;
      const pivotFrom=still||!node.pivotBefore?pivot:node.pivotBefore,tipFrom=still||!node.tipBefore?tip:node.tipBefore;
      node.pivotBefore=pivot;node.tipBefore=tip;
      if(still){node.p=bent;node.v=[0,0,0];}
      else{
        const omega=OMEGA_MAX*node.goal;
        const perSecond=-30*Math.log(1-Math.min(node.drag,.99));
        const zeta=Math.max(MIN_ZETA,perSecond/(2*omega));
        for(let s=0;s<substeps;s++){
          const f=(s+1)/substeps,pivotNow=lerp(pivotFrom,pivot,f),goal=lerp(tipFrom,tip,f);
          const before=node.p;
          const pull=scale(sub(goal,node.p),omega*omega);
          // Drag on the part's own motion through the character's space (the
          // game damps in the world; at a steady speed that is the same thing).
          // Damping against the carrying bone's speed instead whipped long
          // chains: every link chased its parent's swing.
          const damp=scale(node.v,-2*zeta*omega);
          const a:Vec3=[pull[0]+damp[0]+inertia[0],pull[1]+damp[1]+inertia[1],pull[2]+damp[2]+inertia[2]-GRAVITY*node.gravity];
          node.v=add(node.v,scale(a,h));
          node.p=add(node.p,scale(node.v,h));
          const arm=sub(node.p,pivotNow),l=len(arm);
          if(l>1e-6)node.p=add(pivotNow,scale(arm,node.length/l));
          if(node.travel>0){const away=sub(node.p,goal),d=len(away);if(d>node.travel)node.p=add(goal,scale(away,node.travel/d));}
          node.v=scale(sub(node.p,before),1/h);
        }
      }
      const turn=turnAbout(pivot,sub(bent,pivot),sub(node.p,pivot));
      const total=new Array(12) as number[];
      if(inherited)compose(turn,inherited,total);else for(let i=0;i<12;i++)total[i]=turn[i];
      delta[b]=total;
    }
    this.fresh=false;
    for(const b of this.depthOrder){
      const d=delta[b]??this.inherited(b,delta);
      if(!d)continue;
      delta[b]=d;
      for(let r=0;r<3;r++)for(let c=0;c<4;c++){
        let v=d[r*4]*skins[b*12+c]+d[r*4+1]*skins[b*12+4+c]+d[r*4+2]*skins[b*12+8+c];
        if(c===3)v+=d[r*4+3];
        out[b*12+r*4+c]=v;
      }
    }
    return out;
  }
  private inherited(bone:number,delta:Array<ArrayLike<number>|null>):ArrayLike<number>|null{
    for(let at=this.bones[bone].parent;at>=0;at=this.bones[at].parent){if(delta[at])return delta[at];}
    return null;
  }
}

/** Inches per second and inches: about a hero's run speed and jump height. */
export const RUN_SPEED=260, JUMP_HEIGHT=55;
/** Where the jump leaves the ground and lands, in seconds into the clip: from the
 * end of the take-off part through the in-air part into the start of the landing. */
export function jumpArc(clip:MotionClip):[number,number]|null{
  if(clip.name!=='jump'||clip.segments.length<2)return null;
  const start=clip.segments[0]/clip.fps,air=clip.segments[1]/clip.fps;
  return [Math.max(0,start-.08),start+air+.1];
}
/** How far the character has travelled in the world at `time` into a clip: a run
 * moves forward at run speed, a jump rises and falls between take-off and landing.
 * The moving parts feel this; the view shows only the jump's height. */
export function travel(clip:MotionClip,time:number):Vec3{
  if(clip.name==='run')return [RUN_SPEED*time,0,0];
  const arc=jumpArc(clip);
  if(arc){const u=(time-arc[0])/(arc[1]-arc[0]);if(u>0&&u<1)return [0,0,4*JUMP_HEIGHT*u*(1-u)];}
  return [0,0,0];
}
