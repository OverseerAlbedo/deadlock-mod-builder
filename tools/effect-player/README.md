# Effect player

A helper that plays Source 2 particle effects (`.vpcf_c`) offscreen with
[ValveResourceFormat.Renderer](https://github.com/ValveResourceFormat/ValveResourceFormat)
21.1.7798. `serve` keeps one hidden OpenGL 4.6 context open and streams JPEG frames of an effect
over stdout while it takes commands on stdin; it is meant to sit behind an effect preview, one
process per preview. The other commands (`render`, `inspect`, ...) write PNG frames and JSON
reports for testing.

## Build

Needs the .NET 11 SDK (`global.json` pins 11.0.100). Packages come from nuget.org only
(`nuget.config`), at exact versions recorded in `packages.lock.json`.

```powershell
dotnet build -c Release -r win-x64 --property:RestoreLockedMode=true
```

## serve

```text
Dmb.EffectPlayer.exe serve --pak <game>/citadel/pak01_dir.vpk
    [--size 640x480] [--fps 30] [--quality 80] [--msaa 4]
    [--background 0b0c10] [--exposure <stops>] [--verbose]
```

The process makes its hidden window and OpenGL context once, sends `ready`, then reads one JSON
command per line from stdin and writes length-prefixed messages to stdout until it gets `quit` or
stdin reaches end of file. It exits within a few hundred milliseconds of either. Nothing but the
protocol is written to stdout; log lines go to stderr, one plain line each (warnings and errors,
everything with `--verbose`). The caller must keep reading stderr, or the helper stalls once the
pipe buffer is full.

It is a console program: start it with the console window suppressed (`CREATE_NO_WINDOW`). The
OpenGL window is never shown.

**The game files.** `--pak` is the game's `pak01_dir.vpk` (`<Deadlock>/game/citadel/pak01_dir.vpk`).
The helper also mounts, if present, `shaders_pc_dir.vpk` beside it and
`<Deadlock>/game/core/pak01_dir.vpk` and `shaders_pc_dir.vpk` (the engine's own textures and
shaders, which particle files reference). It reads `gameinfo.gi` beside the pak for its settings
only and never mounts the addons folder, so installed mods do not show. Game files are only read.

### Exit codes

| Code | Meaning |
| --- | --- |
| 0 | `quit`, or stdin closed |
| 1 | could not start, or stopped on an internal error (an `error` status with `fatal: true` comes first) |
| 2 | bad command line |
| 3 | OpenGL 4.6 is not available. Before any frame, one `error` status with `"code": "opengl"` and a one-sentence `message` is sent, and the same sentence is written to stderr. The caller should fall back to a preview without the player. |

`--gl-version <major.minor>` asks for another context version (for example `9.9`), which is how
the OpenGL-missing path can be tried on a computer that has OpenGL 4.6.

### Output: the byte layout

Every message on stdout is a 12-byte header and a payload. Numbers are little-endian.

| Offset | Size | Field |
| --- | --- | --- |
| 0 | 4 | magic, the ASCII bytes `DMBE` |
| 4 | 1 | kind: `1` status, `2` frame |
| 5 | 3 | zero |
| 8 | 4 | payload length in bytes (unsigned) |

A **status** payload is one UTF-8 JSON object (no newline). A **frame** payload is a 32-byte frame
header followed by one JPEG (baseline, quality 80 by default):

| Offset | Size | Field |
| --- | --- | --- |
| 0 | 4 | `sequence` (u32): counts every frame the helper made, from 0; a gap means frames were dropped |
| 4 | 4 | `generation` (u32): bumped by `load`, `overlay`, `compare` and `restart` (not by the automatic restarts of a looping effect); the first frame whose generation is at least the one in a reply shows that command's result |
| 8 | 4 | `time` (f32): seconds of effect time since the effect last started over |
| 12 | 4 | `wallMs` (u32): milliseconds since `ready` when the frame was made |
| 16 | 2 | `width` (u16) of the JPEG |
| 18 | 2 | `height` (u16) |
| 20 | 4 | `coverage` (f32): share of pixels of the user's version that differ from the background, 0 to 1 |
| 24 | 1 | flags: bit 0 compare, bit 1 paused, bit 2 finished (looping off and the effect ended) |
| 25 | 1 | edges the user's version touches: bit 0 left, 1 right, 2 top, 3 bottom |
| 26 | 6 | zero |

Reading loop:

```text
loop:
    header = read exactly 12 bytes            # end of stream: the helper has exited
    if header[0..4] != "DMBE": stop, the stream is broken
    kind   = header[4]
    length = u32 little-endian at header[8]
    payload = read exactly length bytes
    if kind == 1:
        status = parse JSON (UTF-8) of payload
        if status.type == "reply": complete the command waiting for status.id
        else if status.type == "error" and status.fatal: show the fallback, stop
        else handle "ready", "finished"
    else if kind == 2:
        sequence   = u32 at payload[0]
        generation = u32 at payload[4]
        time       = f32 at payload[8]
        width      = u16 at payload[16], height = u16 at payload[18]
        jpeg       = payload[32 .. length]
        show jpeg (only the newest; frames never need to be queued)
    other kinds: skip the payload (reserved for later versions)
```

### Pacing

Frames are made at `--fps` (30 by default, 1 to 240; `fps` command) of wall time while an effect
plays, each advancing the effect by `1 / fps × speed` seconds. When a frame takes longer than one
period, the periods it missed are simulated without being drawn (up to 8 at a time) before the
next frame, so the effect keeps its real speed (the frame headers' `time` advances with the wall
clock) even when fewer frames than asked for can be made; the frames are then evenly late rather
than slow. A frame that is later than that never makes the helper fall behind for good: when it
runs more than two frames late it starts its schedule again from now. `stats` reports where the
time of each frame goes (`stages`).
Frames go out through a single slot: when the reader has not taken the previous frame yet (stdout
is blocked), that frame is dropped and replaced, so the picture is always current and nothing is
queued in the helper. The pipe itself still holds what was written before the reader stopped (a
few frames); a reader that falls behind should take every message it has and show the newest
frame. Status messages are never dropped and keep their order. While paused, or after a
non-looping effect has finished, the helper sends a frame only when something visible changes
(view, size, background, brightness, quality) and otherwise sleeps on stdin.

### Commands

One JSON object per line. The command's name goes in the field `cmd`, for example
`{"cmd":"load","id":7,"effect":"particles/x.vpcf_c"}`; the reply names it back in `command`.
Every command may carry an `id` (any JSON value); the reply echoes it.
Each command gets exactly one reply:

```json
{"type":"reply","id":7,"command":"overlay","ok":true,"generation":12,"elapsedMs":8.4, ...}
{"type":"reply","id":8,"command":"load","ok":false,"error":"The effect particles/x.vpcf_c was not found in the game files or the overlay.","generation":12,"elapsedMs":0.6}
```

`error` is one plain sentence. Unknown fields are ignored. Commands are handled in order; all
commands that have arrived are handled before the next frame is drawn.

| Command | Fields | What it does |
| --- | --- | --- |
| `load` | `effect` (game path, `particles/....vpcf_c`; `.vpcf` is accepted), `overlay` (path or null), `compare` (bool, keeps the current choice), `radius` (default 300), `rules` (default true), `floor` (default true), `loop`, `speed`, `paused` (default false), `keepView` (default false) | Loads an effect, replacing the one loaded. Frames it (unless `keepView`) and starts it. Reply has `loaded` and `timing` (`loadMs`, `fitMs`). If it fails, nothing is loaded. |
| `overlay` | `overlay` (path or null) | Replaces the overlay of the loaded effect, rebuilds the user's version from it and restarts both versions; the camera is kept. Reply has `loaded` and `timing.replaceMs`. If the new file cannot be read, or the effect cannot be built from it, the previous version keeps playing. |
| `compare` | `on` (bool, default true) | Game's version left, user's version right, each half the width. Reply has `loaded`. |
| `pause`, `resume` | | Holds or resumes the playback clock. |
| `restart` | | Both versions start over together. |
| `loop` | `on` (bool) | Off: the effect plays once, then a `finished` status comes and frames stop. |
| `speed` | `value` (above 0, at most 8; 1, 0.5, 0.25 or any value) | Playback speed. |
| `size` | `width`, `height` (64 to 2048) | Output size in pixels. |
| `background` | `color` (`"0b0c10"`, six hex digits) | Background colour. Reply `achieved` is the colour the frames actually have. |
| `brightness` | `stops` (-4 to +6) | View brightness (exposure compensation, in stops). Reply `achieved` as above. |
| `fit` | | Frames the effect again from the current angles. Reply has `camera`. |
| `resetView` | | Default angles (yaw 225°, pitch 20°) and frames the effect. |
| `orbit` | `dx`, `dy` (pixels, as from a mouse drag) | 0.4° per pixel: `dx > 0` turns the effect to the right, `dy > 0` raises the camera (looks more from above), pitch kept within ±89°. |
| `zoom` | `steps` (wheel steps, may be fractional) | Each step moves 15 % closer; negative steps move away. |
| `fps` | `value` (1 to 240) | Frame rate. |
| `quality` | `value` (10 to 100) | JPEG quality. |
| `stats` | `collect` (bool, default false: collect managed garbage first) | Reply `stats`: frames sent and dropped, bytes, working set, private and managed memory in MB, CPU seconds, current settings, and `stages` (see below). |
| `quit` | | Exits with code 0. |

**`stats.stages`** (optional; counted since the previous `stats` command, which resets them):
`frames` drawn while playing, `steps` simulated, mean milliseconds per step of `simulateMs` and per
frame of `drawMs` (submitting the views), `readBackMs` (reading the pixels back, which waits for the
GPU to finish the frame), `coverageMs` and `encodeMs` (JPEG); `catchUpSteps` (steps simulated without
a frame to keep real speed) and `scheduleResets` (times the schedule started again from now).

The view is automatic after `load`, `fit` and `resetView`: then a change of `size` or `compare`
frames the effect again for the new shape. After `orbit` or `zoom` the view is kept as it is.

### Status messages

| `type` | When | Fields |
| --- | --- | --- |
| `ready` | once, after the context is made | `protocol` (1), `version`, `library`, `gl` (`vendor`, `renderer`, `version`), `startMs`, `settings` |
| `reply` | once per command | see above |
| `finished` | looping is off and the effect has ended | `generation` |
| `error` | the helper cannot go on (then it exits) | `fatal: true`, `code` (`opengl`, `start`, `internal`), `message` |

**`loaded`** (in the replies to `load`, `overlay` and `compare`):

| Field | Meaning |
| --- | --- |
| `effect`, `overlay`, `compare` | what is playing |
| `bounds` | `min`, `max`, `size` of the particles (position ± radius) over a 2.5 s pre-roll, in game units; `{"empty": true}` when nothing was alive |
| `camera` | `target`, `distance`, `yaw`, `pitch` |
| `files` | `fromOverlay`: every file the user's version took from the overlay; `fromGame`: how many came from the game; `missing`: files looked for and not found. Only lookups made for this load or overlay count: materials and textures already cached from an earlier load are not looked up again, so the particle files are the reliable part of these lists |
| `rules` | one sentence per preview rule applied (see below) |
| `notes.untintedModels` | `renderers`, `parts`, `note`: model renderers whose material variables the preview still cannot apply (see "Model particles") |
| `notes.modelMaterialVars` | optional: `renderers` (model renderers with material variables), `applied` (of them, how many had at least one parameter set from the particle), `fromColour` (how many had a parameter set from the particle's colour), `problems`, and per renderer `parts[]`: `part`, `variables` (each attribute and the particle field it takes, or its constant), `materials`, `applied` and `fromColour` (parameter names), `notApplied` (expressions the preview cannot evaluate, with the reason), `unset` (expressions that read an attribute the renderer does not set), `ignored` (variables the renderer sets that no material it drew reads), `seen` (whether a model of it was drawn), `firstTint` (the colour the first model drawn was tinted with, sRGB hex) |
| `notes.decals` | `renderers`, `parts`, `surface` (`floor` or `none`), `note`: projected decals |
| `notes.renderers`, `notes.screenSpace`, `notes.cannotShow` | renderer classes of the tree with counts; whether it is a screen effect; sentences for what the preview cannot show |
| `notes.undrivenControlPoints` | per control point the effect reads that nothing set: `cp`, `readBy` (up to 8 readers), `reads`, `filled` (rules that gave it a value) |
| `notes.skippedRenderers`, `notes.unsupportedFunctions` | particle functions the library does not implement |
| `notes.warnings` | library warnings logged while loading |
| `seeded`, `compareNote` | whether both versions start from one fixed random seed, and a sentence saying what that means for the comparison |
| `gameVersion` | in compare: whether the game has the effect |

### The overlay

The overlay is a VPK holding the user's edited files. It is the loader's current package, so its
files win over the game's for the user's version; the game's version is built with the overlay
hidden. The helper reads the whole overlay into memory and does not keep the file open, so the
caller may overwrite or delete it as soon as the reply has arrived (a VPK split into numbered
chunk files is the exception: it is read from disk and kept open until it is replaced).

Replacing the overlay, loading the same effect again, switching compare and clearing the overlay
do not make the process grow: private memory settles after the first few dozen and then moves up
and down by about 20 MB. What grows is the cache below: every effect loaded for the first time
adds its materials, textures, models and shaders, which stay for the life of the process. The
growth comes in steps, large for the first effects and for effects with big textures (the first
ten of a hero's effects can add 400 MB; a hundred different effects took the process from about
225 MB to about 920 MB), and loading effects already loaded adds nothing. `stats` reports
`privateMB`, so a caller that keeps one helper open for a long session can start a new one when
it has grown too large.

Materials, textures, models and shaders are cached by name for the life of the process and shared
by both versions. The overlay is meant for particle files (`.vpcf_c`): a material or texture in an
overlay is used the first time that name is loaded, and a later overlay with a different copy of
the same name is not picked up until the helper is started again.

### Compare

With compare on, the game's version and the user's version are two scenes, each drawn by its own
renderer at half the width (left: game, right: user), with the same camera, the same playback
clock (the renderers' time is kept equal) and the same steps, simulated on one thread; they
restart together, and neither loops by itself: the helper starts both over when both have
finished. Both have their random seed pinned to the same value through the library's
`PinRandomSeed` (internal, called by reflection), so every restart plays the same draws. A system
keeps some state over its restarts, so both versions are built new together whenever compare is
switched on and on every `overlay` while compare is on; the two then match particle for particle.
An edit that changes how many particles a part makes can still make them drift apart. When the seed cannot be pinned, `seeded` is false and
the note says the two play in step in time only.

### Preview rules

An effect the game plays is driven by the game: it places control points on characters and
targets, moves projectiles and has a world to collide with. The preview applies five rules, each
read from the effect's own data (`EffectRules.cs`), never from a list of effect names:

1. **Play the top-level effect.** The caller names the effect the game plays (the top of its tree),
   so its children inherit its preview control-point configuration. The effect plays in the
   library's preview mode: the effect's own preview configuration and the library's defaults set
   its control points, and a preview model it names is animated but not drawn.
2. **Radius from an undriven control point.** A radius input (`RadiusTargets`:
   `C_INIT_RingWave.m_flInitialRadius`, `C_INIT_CreateWithinSphereTransform.m_fRadiusMax`,
   `C_INIT_CreateSpiralSphere.m_flInitialRadius`, `C_INIT_CreateInEpitrochoid.m_flRadius1/2`,
   `C_OP_ConstrainDistance.m_fMaxDistance`, and `C_INIT_InitFloat`/`C_OP_SetFloat` writing the
   radius) that reads a control-point component which is still 0 after the effect is set up, and
   that no function of the tree sets (`ControlPointWriters`, such as a parent's
   `C_OP_SetChildControlPoints`), gets
   that component set so the input comes out at `radius` (300 units unless `load` says otherwise):
   the value itself for a direct input, divided by the factor for `PF_MAP_TYPE_MULT`, through the
   inverse of the remap for `PF_MAP_TYPE_REMAP`. `C_INIT_InitFloat`/`C_OP_SetFloat` writing the
   alpha (field 7) from such a component get the value that maps to 1 instead: a strength input
   left at 0 hides the part completely. Compiled files leave out a field that holds its default;
   in the game's particle files `m_nOutputField` of `C_INIT_InitFloat` and `C_OP_SetFloat` is
   often absent and never written as 3, so an absent output field is read as the radius (3).
3. **Paths and beams.** A path function (`PathClasses`) whose two control points sit at the same
   place gets the end that is not control point 0 moved 400 units along +X.
4. **Ground.** An effect with a function that places on or collides with the ground
   (`GroundClasses`) gets the library's ground plane at z = 0. An effect with projected decals
   (`C_OP_RenderProjected`) also gets an invisible floor at z = 0 (`DepthFloor.cs`): a square that
   writes depth and no colour in the opaque pass, with the renderer told to copy the scene depth,
   so the decals, which project onto that copy, have a surface to land on and the background stays
   flat. The floor then takes itself out of the depth again before anything translucent is drawn
   (where it is still the nearest surface it writes the cleared depth back), so sprites, trails and
   translucent models that cross z = 0 are not cut by it: what lies below the floor shows, as it
   would without the floor, where in the game the ground would hide it. Sprites that feather
   against the scene depth still fade into it softly. `load` with `"floor": false` leaves it out.
5. **Projectiles and tracers.** An effect whose file name has the word `tracer` or `bullet` (3000
   units per second, starting over every 0.2 s), or `projectile` or `missile` (800 units per
   second, every 0.75 s), or that has a rope or trail fed by a continuous emitter and no path to
   follow (3000 units per second, every 0.2 s), is carried along +X, unless its name also has a
   word of `StaticWords` (`impact`, `hit`, `explode`, `muzzle`, `flash`, `splash`, `debuff`,
   `glow` and the like: something that happens in one place). An effect that reads its own speed
   (`C_OP_RemapSpeedtoCP`, `C_OP_RemapSpeed`) is carried at 500 units per second, every 1.2 s.
   Control point 0 and a preview model holding it move; the camera moves with them, so the moving
   thing stays in place in the picture with its trail streaming behind it, and the effect starts
   over at the end of each run. Framing and `bounds` are then measured in that moving frame.

Each rule that changes something adds a sentence to `loaded.rules`. `render --rules` applies rules
2 to 5 to a single render for testing.

### Framing

`load`, `fit` and `resetView` play the effect for 2.5 s without drawing and collect where its
particles are (position and radius) at each step. The camera target is the middle of their extent
(2nd to 98th percentile along each camera axis) and the distance is the 90th percentile over the
steps of what each step needs to keep its particles within 80 % of the frame; framing from the
particles rather than from pixels keeps long beams and paths whole. The effect is then played
again and drawn, up to six times: the camera distance is searched between the nearest distance
at which the picture touched an edge and the farthest at which it fitted, the picture is centred,
and the result is a distance that was seen to fit. Model particles are framed by their pixels, and
those whose model is posed by an animation sequence also by the bounds of that model (their parts
move within it, further than the pre-roll may show); `bounds` stays that of the particles'
positions and radii. Sprites with `m_flStartFadeSize` fade out once they cover
that share of the screen: when nothing showed at 80 %, the effect is framed again to fill
0.8 × the smallest such value of the tree. A screen-space effect is not framed.

### What the preview cannot show

- **Model particles.** The library tints each model a `C_OP_RenderModels` renderer draws by the
  particle's colour (unless `m_bSuppressTint`), but does not apply the renderer's `m_MaterialVars`.
  The player applies them itself (`ModelMaterialVars.cs`): each entry names a render attribute
  (`$COLORTINT`, `$SELFILLUM`, `$ALPHATEST`, ...) and the particle field it takes its value from,
  times `m_flScale`. An entry without a field holds the default field, which the player reads as
  the scratch float (18): compiled files leave out a field that holds its default, no entry in the
  game's files names 18 explicitly, and the effects whose entries leave the field out write the
  scratch float (`C_OP_SetFloat` with output field 18). This is inferred from the data, not
  documented. The model's material turns attributes into parameters through its dynamic expressions
  (`g_vSelfIllumTint1 = return $SELFILLUMTINT;`), which the player evaluates per particle
  (`MaterialExpression.cs`) and writes into a copy of the material that only that model draw uses.
  An expression is left at the material's own value when it reads an attribute the renderer does
  not set (the game then reads whatever an unset attribute holds, which the preview does not know;
  `notes.modelMaterialVars` lists these as `unset`), when it uses something the player cannot
  evaluate, or when it feeds a texture parameter (`g_tColor = return $COLOR;`: what a texture given
  a number shows in the game is not known, so the material's own texture is drawn); renderers with
  the latter two are counted in `notes.untintedModels`. Which model draws which particle is internal to the
  library and read by reflection (`LibraryAccess.cs`); when that fails, every such renderer is
  counted as untinted.
- **Projected decals.** `C_OP_RenderProjected` renderers project onto whatever depth the scene
  has. The invisible floor gives them a flat surface at the effect's base; in the game they land on
  walls and slopes too, which the preview does not show (`notes.decals`, `surface` is `floor`).
- **Lights, models and other effects.** An effect that only gives off light has nothing to light;
  parts placed on a character model (`C_INIT_CreateOnModel` and the like) have no model unless the
  effect names a preview model; a top system with no renderers only starts other effects in the
  game. `notes.cannotShow` gives one sentence for each of these, and `notes.renderers` counts the
  renderer classes of the tree.
- **Screen effects.** A screen-space effect covers the player's view in the game; it fills the
  frame and is not framed (`notes.screenSpace`).
- **Control points nothing drives.** Every control point the tree reads (number inputs taken from
  a control-point component, vector inputs, and fields naming a control point, other than the
  fields of functions that set control points) is listed when it stays at the origin of the top
  system through the pre-roll, with what reads it and the rule that filled it, if any
  (`notes.undrivenControlPoints`). A control point a parent sets only on its children can appear
  here although it is driven.
- Renders are not bit-for-bit repeatable across runs; compare statistics, not pixels.

## Other commands

```text
effect-player render --pak <pak01_dir.vpk> --effect <particles/...vpcf_c> --out <folder>
    [--overlay <vpk>] [--size 640x480] [--fps 30] [--seconds 3] [--speed 1]
    [--background 0b0c10] [--exposure <stops>] [--gamma 2.2] [--msaa 4]
    [--frame auto|node|fixed] [--yaw <deg>] [--pitch <deg>] [--distance <units>] [--target x,y,z]
    [--pad 1.15] [--preroll <seconds>] [--cp N=x,y,z]... [--cp-velocity N=x,y,z]... [--cp-period 1]
    [--no-sun] [--show-model] [--mode preview|game] [--ground <z>] [--serial] [--no-frames]
    [--stream-cost] [--quality 80] [--rules] [--radius 300] [--gl-version <major.minor>]
effect-player inspect --pak <pak01_dir.vpk> --effect <path> [--overlay <vpk>]
effect-player scan --pak <pak01_dir.vpk> --prefix <particles/...> --out <file.json>
effect-player fields --pak <pak01_dir.vpk> [--prefix particles/] --class <C_OP_...,...> --key <m_...,...>
effect-player models --pak <pak01_dir.vpk> (--effect <path> | --prefix <particles/...>) [--overlay <vpk>]
effect-player compare --pak <pak01_dir.vpk> --overlay <vpk> [--verbose]
effect-player image-stats --out <folder of frames> [--background 0b0c10]
```

Exit codes as for `serve`, plus 4: effect not found.

- `render` writes PNG frames and a `run.json` with measurements: OpenGL vendor, renderer and
  version; time to load the effect, create the context, calibrate the background, pre-roll and
  reach the first frame; per-frame simulate, draw, read-back and encode times; particle and system
  counts; bounds and camera; renderer classes the library skipped; every library log message; the
  loader's resolved files. `--stream-cost` also encodes every frame as JPEG and WebP without
  writing them and reports their cost.
- `inspect`, `scan`: the control points an effect reads, its children and its control-point
  configurations.
- `fields`: how often each value of a key occurs in the particle functions of the given classes
  across a package (`absent` usually means the default value). Arrays and collections are written
  out whole on one line (`[{m_strVariable=$COLORTINT m_nVariableField=6}]`).
- `models`: for every model renderer of an effect tree (or of every particle file under a prefix,
  each on its own): its material variables, colour blend, models and the materials they draw with
  (or the override material), with each material's shader, dynamic expressions, render attributes,
  and its int, float, vector and texture parameters, as JSON.
- `compare`: the parsed data of every particle system in an overlay against the game's copy.
- `image-stats`: per frame, the pixels that differ from the background, their summed brightness,
  brightness-weighted mean hue and the colour of the brightest 2 %.

## How a frame is made

- **Context** (`RenderHost.cs`): a hidden GLFW window (`StartVisible = false`) with an OpenGL 4.6
  core, forward-compatible context and the library's `RendererContext`. Each `RenderView` has its
  own `Renderer`, scene and post-processing, a multisampled RGBA16F scene target and an 8-bit
  output target read back with `glReadPixels`. No grid, sky or background material is drawn
  (`Renderer.Skybox2D = null`); the scene target is cleared to a flat colour, lit by the default
  sun. Texture streaming is immediate, so the first frame is complete.
- **Brightness and background**: brightness is `Postprocess.ExposureCompensation` in stops; tone
  mapping is the library's, as configured from the game's `gameinfo.gi`. The clear colour passes
  through exposure, tone mapping and gamma like the effect does, so the linear clear value whose
  output equals the requested sRGB colour is searched for per channel (with the effect hidden)
  and cached per colour and brightness.
- **Loader** (`EffectLoader.cs`): the overlay is the loader's current package; the game's packages
  are search packages. Every file the loader resolves is recorded with the package it came from.

## Publishing

The app ships one self-contained, trimmed, compressed file. Those settings are in
`EffectPlayer.csproj` itself (so build and publish restore the same packages, including the SDK's
`Microsoft.NET.ILLink.Tasks`, and one `packages.lock.json` covers both). From the repository root:

```powershell
pwsh -NoProfile -File scripts/publish-effect-player.ps1    # DMB_DOTNET = a dotnet.exe with SDK 11.0.100
```

It runs `dotnet publish tools/effect-player/EffectPlayer.csproj -c Release -r win-x64
-p:RestoreLockedMode=true`, checks that the output is the single file and nothing else, that
what went into it matches `licenses/effect-player/DEPENDENCIES.json`, and that it starts
(`--version`, a usage error, `image-stats` of a small PNG), then stages it at
`resources/native/effect-player/win-x64/Dmb.EffectPlayer.exe` (a build output, git-ignored).
`scripts/build-windows.ps1` runs it before the app build.

Trimmed builds keep `OpenTK.Graphics`, `ValveResourceFormat` and `Renderer` whole and turn
reflection-based JSON back on: OpenTK loads its GL bindings by reflection, and the player reads a
few library members by reflection (all of them in `LibraryAccess.cs`). No symbol files ship: `libSkiaSharp.pdb` from the SkiaSharp
native package is left out of the publish, and the trimmer writes no symbols.

## Licences

The program's own code follows the repository's licence. The notices of everything in the
published file are in `licenses/effect-player/` (`DEPENDENCIES.json` lists each package, the file
it accounts for and where each text came from); `node scripts/generate-effect-player-notices.mjs`
rebuilds them after a publish. Packages in the published output:

| Package | Version | Licence | Licence text |
| --- | --- | --- | --- |
| ValveResourceFormat, ValveResourceFormat.Renderer | 21.1.7798 | MIT | package `THIRD_PARTY_NOTICES.txt`; licenses.nuget.org/MIT |
| ValveKeyValue | 0.80.0.542 | MIT | licenses.nuget.org/MIT |
| ValvePak | 7.2.0.221 | MIT | licenses.nuget.org/MIT |
| KeyValues2 (Datamodel.NET.dll) | 2.0.0-rc2 | MIT | licenses.nuget.org/MIT |
| OpenTK.Core, .Graphics, .Mathematics, .Windowing.Common, .Windowing.Desktop, .Windowing.GraphicsLibraryFramework | 4.9.4 | MIT | licenses.nuget.org/MIT |
| OpenTK.redist.glfw (glfw3.dll) | 3.4.0.44 | zlib (GLFW) | package `COPYING.md` |
| SkiaSharp, SkiaSharp.NativeAssets.Win32 (libSkiaSharp.dll) | 4.153.1 | MIT; Skia and its dependencies as listed | package `LICENSE.txt`, `THIRD-PARTY-NOTICES.txt` |
| Vortice.SpirvCross, Vortice.SPIRV (spirv-cross.dll) | 1.6.0, 1.0.6 | MIT; the bundled SPIRV-Cross library is Apache-2.0 OR MIT (used under Apache-2.0) | licenses.nuget.org/MIT; SPIRV-Cross's terms as the ValveResourceFormat notices state them |
| SharpGLTF.Core | 1.0.7 | MIT | licenses.nuget.org/MIT |
| TinyBCSharp | 0.1.2 | MIT | licenses.nuget.org/MIT |
| NLayer | 3.0.0 | MIT | licenses.nuget.org/MIT |
| K4os.Compression.LZ4 | 1.3.8 | MIT | licence URL in the package (project `LICENSE`) |
| System.IO.Hashing, System.Numerics.Tensors | 11.0.0 | MIT | package `THIRD-PARTY-NOTICES.TXT` |
| .NET runtime (self-contained builds) | 11.0.0 | MIT | runtime pack `LICENSE.TXT`, `THIRD-PARTY-NOTICES.TXT` |
