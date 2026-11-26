//! What to do about an error. Each rule recognises a failure by words in its
//! message or details and gives the user concrete steps plus the page where
//! the fix happens. The first matching rule wins, so specific rules come
//! before general ones. Rules are written from real failure logs; add a test
//! with the real message when adding a rule.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HelpAction {
    pub label: String,
    /// App page: settings, setup, project, prepare, appearance, physics, build, logs.
    pub page: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Help {
    /// A one-line plain explanation of what went wrong.
    pub cause: String,
    pub steps: Vec<String>,
    pub action: Option<HelpAction>,
}

struct Rule {
    /// Any of these (lowercase) words in the message or details.
    any: &'static [&'static str],
    /// And all of these.
    all: &'static [&'static str],
    cause: &'static str,
    steps: &'static [&'static str],
    action: Option<(&'static str, &'static str)>,
}

const MODEL_TOOLSET_STEPS: &[&str] = &[
    "Download Reduced CSDK 13 (Settings → Get programs and download links), extract it to a short folder such as C:\\CSDK13 and choose that folder in Settings → Community SDK. It compiles the hero model too; nothing else is needed.",
    "Click Detect tools, then Save and check tools. If the Model compiler line offers Compile models with Reduced CSDK 13, click it and save.",
    "Build again.",
    "Only if you keep a model toolset downloaded from Steam: choose it under Settings → Advanced configuration → Downloaded model toolset.",
];

const RULES: &[Rule] = &[
    // The build analyzes the project again first (build.rs) and refuses what still
    // needs a choice; Build & install may start a build without a current analysis.
    // Keep these phrases in step with `BUILD_REFUSALS` in src/build/readiness.ts.
    Rule { any: &["review mesh mappings, materials, lods and unskinned meshes before building", "model compilation stopped by vertex preflight"], all: &[],
        cause: "The build checked the project again before starting, and something still needs a choice: a mesh mapping, a material, the detail levels, meshes without skin weights, or a mesh over the vertex limit.",
        steps: &["Open Build & install and click Analyze project: the list there shows what is left, each item with a button that opens the place to fix it.", "Fix the items on the list, then build again."],
        action: Some(("Open Build & install", "build")) },
    Rule { any: &["not included in this build"], all: &["tangtang"],
        cause: "This copy of the app was built without the TangTang presets.",
        steps: &["Open Physics & preview → Automatic motion and choose Automatic — match movement to each part.", "Click Analyze project again. Automatic authors the movement from your model; the TangTang presets only ever applied to the accepted Haze conversion."],
        action: Some(("Open Physics & preview", "physics")) },
    Rule { any: &["another operation is running"], all: &[],
        cause: "Another task was still running when you clicked.",
        steps: &["Wait until the status bar at the bottom says Ready, then click again.", "If a step is shown as running and you do not need it, click Cancel in the status bar.", "If it keeps happening with nothing running, restart the app."],
        action: None },
    // Preparing a model without a skeleton: estimate_landmarks (algorithms.py) could
    // not find the joints from the shape. Inside a Blender failure, so before the
    // Blender rules. A file saved without its armature is a common cause.
    Rule { any: &["too little valid humanoid geometry", "expected an upright conventional t/a-pose humanoid", "cannot isolate one head and two legs",
                  "two separated legs were not identified", "missing arm or leg on", "arms must be separated from the torso", "strong asymmetry or multiple characters"], all: &[],
        cause: "The app could not find your model's joints from its shape. That is how a model without a skeleton (armature) is prepared, and this shape did not fit: it needs one person standing upright in a T or A pose.",
        steps: &["Look at the notes under the top bar of Prepare model. If one says the file has no skeleton, the mesh was saved without its armature: choose a file that includes the skeleton, for example an FBX or the original model this mesh came from, or add the skeleton back in Blender and save the file again.",
                 "If your model really has no skeleton, pose it in Blender as one person standing upright, arms away from the body and legs apart, save the file and prepare again.",
                 "If the model lies down or faces sideways, set Source orientation on Model preparation and check it again."],
        action: Some(("Open Model preparation", "prepare")) },
    // Saved preparation choices the current body roles no longer allow
    // (deadlock-preparation secondary_bones::stale_groups_message, also raised by
    // secondary_bones.py inside a Blender run, so before the Blender rules; and
    // following::stale_message). The review normally shows them on the page first.
    Rule { any: &["saved moving parts are no longer found in your model", "no longer fits your body roles", "no longer fit your body roles"], all: &[],
        cause: "Some choices saved on Prepare model no longer fit the model's body roles: a moving part or a bone that follows a body joint was chosen for joints that changed (for example after correcting a role by hand).",
        steps: &["Open Prepare model and click Check for problems.",
                 "Click the button next to each item: Refresh the matches (the same as Refresh suggestions using my roles on the Joint matches tab) or Use the detected groups (also on the Moving parts tab). Your body roles stay as you chose them.",
                 "Click Prepare model again."],
        action: Some(("Open Prepare model", "prepare")) },
    // More kept moving-part bones than the limit (deadlock-preparation
    // secondary_bones::too_many_bones_message, also raised by secondary_bones.py
    // inside a Blender run). Older reports and logs hold the earlier sentence that
    // covered this and shared bones together.
    Rule { any: &["the kept moving parts have", "selected physics branches overlap or exceed"], all: &[],
        cause: "The moving parts kept from your model's rig have more bones in total than one model can keep. The message gives the total, the limit and the largest groups.",
        steps: &["Open Prepare model → Moving parts and read the kept bones count under Detected groups.",
                 "Untick the largest groups that matter least to you (the message names the largest ones) until the count is at or under the limit. A part that is left out follows the body and stays still.",
                 "Click Prepare model again."],
        action: Some(("Open Prepare model", "prepare")) },
    // Two kept moving parts that share bones (secondary_bones::shared_bones_message,
    // also raised by secondary_bones.py).
    Rule { any: &["share bones, so they cannot both be kept"], all: &[],
        cause: "Two of the kept moving parts contain the same bones, so the app cannot keep both.",
        steps: &["Open Prepare model → Moving parts and untick one of the two groups the message names.",
                 "If the list looks out of date, press Use the detected groups (or Load and check only if you changed the model file).",
                 "Click Prepare model again."],
        action: Some(("Open Prepare model", "prepare")) },
    // Extras → Items (items.rs, deadlock-components item_icons.rs).
    Rule { any: &["to see the game's items"], all: &[],
        cause: "The app needs the game's archive to list its items.",
        steps: &["Open Settings and choose the current Deadlock pak01_dir.vpk (in Deadlock\\game\\citadel).", "Save, then open Extras → Items again."],
        action: Some(("Open Settings", "settings")) },
    // Renamed shop items (deadlock-components item_names.rs, deadlock-project item_names.rs).
    Rule { any: &["the game's item list changed since this mod was built"], all: &[],
        cause: "A game update changed the game's item names files after this mod was built. The mod carries the game's whole list of names, so installing it now would bring back the old list (and leave new items without a name).",
        steps: &["Open Build & install and build the mod again: it is made from the game's current names.", "Then install the new build."],
        action: Some(("Open Build & install", "build")) },
    Rule { any: &["renamed items changed since this mod was built"], all: &[],
        cause: "The renamed items in the project are not the ones this build was made with.",
        steps: &["Open Build & install and build again, then install the new build."],
        action: Some(("Open Build & install", "build")) },
    Rule { any: &["a renamed item is not in the game's item list now"], all: &[],
        cause: "A saved renamed item names an item the game's names file does not have now (a game update removed or renamed it).",
        steps: &["Open Extras → Items, find the renamed item the message names and remove its row (or switch it off).", "Build again."],
        action: Some(("Open Extras", "extras")) },
    Rule { any: &["the game's item names were not found beside the game archive", "the game's item names file could not be read and written back exactly"], all: &[],
        cause: "The app makes renamed items from the game's own names files (Deadlock\\game\\citadel\\resource\\localization\\citadel_gc_mod_names), and could not find or read them.",
        steps: &["Open Settings and check that the game archive is the current Deadlock pak01_dir.vpk (in Deadlock\\game\\citadel); the names folder sits beside it.", "If it is, let Steam verify the game files, then build again.", "To build without renamed items, switch them off in Extras → Items."],
        action: Some(("Open Settings", "settings")) },
    Rule { any: &["type a name for the item", "an item name is at most", "an item name is one line", "an item name cannot", "an item name has no spaces", "two renamed item rows name the same item", "invalid item in a renamed item row", "item names support at most"], all: &[],
        cause: "A renamed item's new name or row is not one the game's names file can carry.",
        steps: &["Open Extras → Items and change the name: one line of at most 32 characters, without \" \\ < > { } or %, not starting with #.", "Save, then build again."],
        action: Some(("Open Extras", "extras")) },
    Rule { any: &["the item list is not loaded", "since the items were loaded", "the game's item icon changed after it was chosen"], all: &[],
        cause: "The game's items were read before the game files changed, or not read yet.",
        steps: &["Open Extras → Items and click Reload assets.", "Select the item again, check your picture, then build again."],
        action: Some(("Open Extras", "extras")) },
    Rule { any: &["is not one of the game's item icons now"], all: &[],
        cause: "This picture is no longer one of the game's item icons (a game update removed or renamed it, or it is an older item icon the app no longer offers), so a saved item icon points at a picture that cannot be built.",
        steps: &["Open Extras → Items and click Reload assets.", "Turn on Replaced only, remove the replacement the build names, and add your picture to the item again.", "Build again."],
        action: Some(("Open Extras", "extras")) },
    Rule { any: &["the game's item list could not be read"], all: &[],
        cause: "The game archive chosen in Settings has no readable item definitions.",
        steps: &["Open Settings and check that the game archive is the current Deadlock pak01_dir.vpk.", "If it is, let Steam verify the game files, then click Reload assets in Extras → Items."],
        action: Some(("Open Settings", "settings")) },
    Rule { any: &["add a picture for each item icon you replace", "replaced both in extras → icons and in extras → items", "two enabled item icon replacements use the same picture"], all: &[],
        cause: "A saved item icon needs a change before the build.",
        steps: &["Open Extras → Items and turn on Replaced only.", "Give every replaced item a picture, or remove the ones you do not want. A picture can be replaced in Icons or in Items, not in both.", "Build again."],
        action: Some(("Open Extras", "extras")) },
    // A picture row whose file is not an image (icons.rs PICTURE_NOT_READABLE, at
    // Analyze and at build; for example a text file renamed to .png).
    Rule { any: &["a replacement picture is not a png or jpeg image the app can read", "the image format could not be determined"], all: &[],
        cause: "A file chosen as a replacement picture in Extras is not a PNG or JPEG image the app can read. It may be another kind of file with a .png or .jpg name.",
        steps: &["Open Extras → Icons (or Items) and find the picture the message names.", "Choose a real PNG or JPEG image for it, or remove that replacement.", "Analyze and build again."],
        action: Some(("Open Extras", "extras")) },
    // Effect edits at build (deadlock-components effect_build.rs).
    Rule { any: &["an edited effect is no longer one of this hero's effects"], all: &[],
        cause: "A game update removed or renamed an effect you edited, so the saved edit points at an effect the game no longer has.",
        steps: &["Open Extras and click Reload assets.", "Open Effects, find the effect named in the message (it may have a new name), and edit it again or remove the old edit.", "Build again."],
        action: Some(("Open Extras", "extras")) },
    Rule { any: &["the game's files of an edited effect changed", "effect edits changed. rebuild before installing"], all: &[],
        cause: "The game's effect files, or your effect edits, changed after this mod was built, so the built files no longer match.",
        steps: &["Open Build & install and build again.", "Install the new build."],
        action: Some(("Open Build & install", "build")) },
    // Ability objects (Extras → Objects; deadlock-components ability_objects.rs,
    // deadlock-preparation ability_object.rs, deadlock-pipeline ability_objects.rs, objects.rs).
    Rule { any: &["ability objects are compiled only with reduced csdk 13"], all: &[],
        cause: "Ability objects are compiled with the model compiler of Reduced CSDK 13 (or a later Community SDK); the setup chosen in Settings is another one.",
        steps: &["Open Settings and choose the Reduced CSDK 13 folder as the Community SDK.", "Leave the model compiler on Auto (or choose Compile models with Reduced CSDK 13) and save.", "Build again."],
        action: Some(("Open Settings", "settings")) },
    Rule { any: &["to see the hero's ability objects", "ability objects are prepared with blender"], all: &[],
        cause: "The app needs the game's archive to list the hero's ability objects, and Blender to prepare your file for one.",
        steps: &["Open Settings and choose the current Deadlock pak01_dir.vpk (in Deadlock\\game\\citadel) and blender.exe.", "Save, then open Extras → Objects again."],
        action: Some(("Open Settings", "settings")) },
    Rule { any: &["this ability object cannot be replaced"], all: &[],
        cause: "The game model this ability object row replaces is no longer one the app can replace for this hero (a game update changed it, or the row is for another hero).",
        steps: &["Open Extras → Objects and click Reload assets.", "Remove the row the message names, or choose the object again from the list.", "Build again."],
        action: Some(("Open Extras", "extras")) },
    Rule { any: &["ability object's file is not prepared", "an ability object comes from a blender, fbx, glb or gltf file", "choose a file for each enabled ability object"], all: &[],
        cause: "An ability object needs its file chosen and prepared before the build, and the file changed or was never prepared.",
        steps: &["Open Extras → Objects and select the object named in the message.", "Choose its file (Blender, FBX, GLB or glTF) and click Prepare.", "Build again."],
        action: Some(("Open Extras", "extras")) },
    // Preparing the object's file (staging in source.rs, then ability_object.py;
    // the words of the staging and of Blender 5.3 when a file cannot be read:
    // .blend, FBX, glTF and GLB).
    Rule { any: &["has no visible mesh object to use as the ability object", "has no polygons to use as the ability object", "the prepared object has no points", "the prepared object has no meshes"], all: &[],
        cause: "The file you chose for the ability object has no visible mesh with faces to use: it holds only empties, armatures, curves or hidden objects, or meshes without polygons.",
        steps: &["In Blender, make the object a visible mesh with faces: show it, enable it for rendering, and convert a curve or text to a mesh (Object → Convert → Mesh).", "Save the file (or export it again), then choose it again in Extras → Objects."],
        action: Some(("Open Extras", "extras")) },
    Rule { any: &["missing or escaping gltf dependency"], all: &["the object's file could not be prepared"],
        cause: "The glTF file (.gltf) names a data or image file that is not beside it (or lies outside its folder), so it cannot be read.",
        steps: &["Keep the .gltf together with its .bin and image files in one folder, or export the object again as one .glb file.", "Choose the file again in Extras → Objects."],
        action: Some(("Open Extras", "extras")) },
    Rule { any: &["file format is not supported", "failed to read blend file", "not a blend file", "ascii fbx files are not supported", "couldn't open file", "bad gltf", "bad glb", "invalid glb", " at line "], all: &["the object's file could not be prepared"],
        cause: "Blender could not open the file you chose for the ability object: it is damaged or incomplete (a download or copy that did not finish), or it is not really the kind of file its name says (an ASCII FBX, for example, which Blender does not read).",
        steps: &["Open the file in Blender yourself to see whether it loads. If it does not, export or download it again.", "For an FBX, export it again as a binary FBX; for a glTF, export a .glb.", "Choose the file again in Extras → Objects."],
        action: Some(("Open Extras", "extras")) },
    Rule { any: &["the game's ability object changed since this build", "ability objects changed. rebuild before installing"], all: &[],
        cause: "The game's object, or your ability object rows, changed after this mod was built, so the built files no longer match.",
        steps: &["Open Build & install and build again.", "Install the new build."],
        action: Some(("Open Build & install", "build")) },
    // How a model follows an object with several bones (deadlock-components object_follow.rs).
    Rule { any: &["using your file's own skeleton is not offered yet"], all: &[],
        cause: "This ability object row uses your file's own skeleton (Same skeleton), and the app does not offer that way yet.",
        steps: &["Open Extras → Objects and select the object named in the message.", "Under How your model moves, choose In one piece or By parts.", "Build again."],
        action: Some(("Open Extras", "extras")) },
    Rule { any: &["your file has no armature, so it cannot use the game object's skeleton", "your file's armature has no bone named as the game object's bones", "your file weights a part to a bone the game's object does not have", "part of your file follows no bone"], all: &[],
        cause: "Same skeleton uses your file's own armature and weights, and they do not fit the game object's bones: the armature is missing, its bones are named otherwise, or some points are weighted to other bones or to none.",
        steps: &["In Blender, name your armature's bones exactly as the game object's bones (the message names one) and weight every point to them; then prepare the file again in Extras → Objects.", "Or choose In one piece or By parts under How your model moves: they need no armature.", "Build again."],
        action: Some(("Open Extras", "extras")) },
    Rule { any: &["your file weights a part to a bone that one of the game object's states does not follow"], all: &[],
        cause: "This ability object shows one of several states (for example whole and broken), and each state follows only some of its bones. Same skeleton uses your file's own weights in every state, and some points are weighted to a bone that one state does not follow.",
        steps: &["In Blender, weight your file only to bones that every state of the object follows (the message names the bone that does not fit); then prepare the file again in Extras → Objects.", "Or choose In one piece or By parts under How your model moves: each state then follows its own bones by itself.", "Build again."],
        action: Some(("Open Extras", "extras")) },
    Rule { any: &["your model can follow only a bone that carries part of the game object's mesh"], all: &[],
        cause: "A part of your model is set to follow a bone that carries none of the game object's mesh (the message lists the bones that do).",
        steps: &["Open Extras → Objects and select the object.", "Under Your parts (or Follows, in one piece), choose one of the bones the message lists.", "Build again."],
        action: Some(("Open Extras", "extras")) },
    Rule { any: &["the game's object has no bone that carries its mesh"], all: &[],
        cause: "The game's object has bones, but none of them carries its mesh, so there is nothing for your model to follow.",
        steps: &["Remove this row in Extras → Objects; the object cannot be replaced this way.", "Open Logs and report the object named in the message."],
        action: Some(("Open Extras", "extras")) },
    // Objects that keep the game's own animation or collision (deadlock-components keep_blocks.rs).
    Rule { any: &["the game's own animation or collision cannot be kept in the rebuilt ability object"], all: &[],
        cause: "This ability object keeps the game's own animation or collision unchanged, and the compiled model did not fit them (its skeleton, its blocks or its references came out otherwise), so the build stopped before packaging it.",
        steps: &["Open Logs and copy the build's last lines.", "Report them with the object and the file you used; until then, remove the row in Extras → Objects to build the rest of your mod."],
        action: Some(("Open Logs", "logs")) },
    // Objects whose cloth is kept as the game's own: the object is decompiled a second time
    // without its cloth for the bones' resting places (deadlock-pipeline object_cloth.rs).
    // Before the general decompile rule, whose word "decompil" the second message contains.
    Rule { any: &["did not write the game's ability object without its cloth"], all: &[],
        cause: "This ability object has cloth, so the app decompiles the game's object a second time without its cloth to read where its bones rest, and Source 2 Viewer wrote nothing that time, usually because the Source 2 Viewer CLI is older than the app needs.",
        steps: &["Download the latest Source 2 Viewer CLI (Settings → Get programs) and replace the old Source2Viewer-CLI.exe.", "Build again.", "If it still fails, open Logs and report the build's last lines with the object; until then, remove the row in Extras → Objects to build the rest of your mod."],
        action: Some(("Open Settings", "settings")) },
    Rule { any: &["decompiles with other bones without its cloth"], all: &[],
        cause: "This ability object has cloth, and the game's object decompiled with its cloth and without it names different bones, so the app cannot take the bones' resting places from the one without the cloth and stopped before compiling.",
        steps: &["Open Logs and copy the build's last lines.", "Report them with the object; until then, remove the row in Extras → Objects to build the rest of your mod.", "A newer Source 2 Viewer CLI (Settings → Get programs) may decompile it otherwise: replace it and build again."],
        action: Some(("Open Logs", "logs")) },
    // Objects with cloth: game data the decompile leaves out is put back from the game's own
    // key values (deadlock-pipeline object_cloth.rs `with_dropped_game_data`).
    Rule { any: &["in a form the app does not know", "decompiles without exactly one game data list", "game data list has no entries"], all: &["ability object"],
        cause: "This ability object has cloth, and the app puts back game data that Source 2 Viewer leaves out of the decompiled object (such as the cloth's ground plane). The game's object or its decompile held that game data in a form the app does not know, so the build stopped before compiling.",
        steps: &["Open Logs and copy the build's last lines.", "Report them with the object; until then, remove the row in Extras → Objects to build the rest of your mod.", "A newer Source 2 Viewer CLI (Settings → Get programs) may decompile it otherwise: replace it and build again."],
        action: Some(("Open Logs", "logs")) },
    // Objects with parts kept as the game's own in every level of detail: the decompiled model is
    // changed to bind and order the bones as the game's file does (deadlock-pipeline object_levels.rs).
    // Before the general decompile rule, whose word "decompil" some of these messages contain.
    Rule { any: &["the game's kept parts state no bones they are bound to", "states no do_not_discard", "the game's kept parts are bound to is not in the decompiled model", "the bones of the game's object could not be put in the game's order", "the game's object holds no single attachment list", "the decompiled model has no format line"], all: &[],
        cause: "This ability object keeps some of its parts as the game's own in every level of detail, so the app marks and orders its bones and moves its attachments as the game's file has them before compiling. The decompiled object did not hold what that needs, so the build stopped before compiling.",
        steps: &["Open Logs and copy the build's last lines.", "Report them with the object; until then, remove the row in Extras → Objects to build the rest of your mod.", "A newer Source 2 Viewer CLI (Settings → Get programs) may decompile it otherwise: replace it and build again."],
        action: Some(("Open Logs", "logs")) },
    Rule { any: &["the ability object's kept animation or collision is no longer the game's own"], all: &[],
        cause: "The built ability object no longer holds the game's own animation or collision exactly (the game's object changed, or the built file was changed after the build).",
        steps: &["Open Build & install and build again.", "Install the new build."],
        action: Some(("Open Build & install", "build")) },
    // Objects whose model data is written back from the game's own file (deadlock-components write_back.rs),
    // and objects with parts kept as the game's own.
    Rule { any: &["the game's own model data cannot be written into the rebuilt ability object"], all: &[],
        cause: "This ability object takes a few details of its model data from the game's own file, and the compiled model differed from the game's in more than those details, so the build stopped before packaging it.",
        steps: &["Open Logs and copy the build's last lines.", "Report them with the object and the file you used; until then, remove the row in Extras → Objects to build the rest of your mod."],
        action: Some(("Open Logs", "logs")) },
    Rule { any: &["the ability object's model data written from the game's own file is no longer the game's", "the ability object's parts kept as the game's own no longer match the game's object"], all: &[],
        cause: "The built ability object no longer matches the game's own object where it takes its data or parts from it (the game's object changed, or the built file was changed after the build).",
        steps: &["Open Build & install and build again.", "Install the new build."],
        action: Some(("Open Build & install", "build")) },
    Rule { any: &["the compiled ability object differs from the game's", "does not use exactly its own materials", "the compiler wrote a file the ability object build did not expect"], all: &[],
        cause: "The compiled ability object kept more than its new mesh different from the game's (bones, attachments, physics, references or materials), so the build stopped before packaging it.",
        steps: &["Open Logs and copy the build's last lines.", "Report them with the object and the file you used; the build packages nothing for this object until it compiles the same as the game's."],
        action: Some(("Open Logs", "logs")) },
    // Clips taken from another hero (Motion; deadlock-project taken_clip.rs,
    // deadlock-components hero_clips.rs, taken_clips.rs).
    Rule { any: &["this clip cannot be taken from another hero"], all: &[],
        cause: "The game uses this clip in a way that cannot be taken from another hero: it is added on top of other clips (a lean or a layer), holds a single pose, or stores bones the app does not write.",
        steps: &["On the Motion page, replace this clip with a compiled clip file instead, or take another of this hero's clips from the other hero."],
        action: Some(("Open Motion", "motion")) },
    Rule { any: &["that clip of the other hero cannot be used"], all: &[],
        cause: "The other hero's clip is not a full pose of the body (the game adds it on top of other clips, or samples bones in the model's space), so it cannot be carried onto this hero.",
        steps: &["On the Motion page, choose another of that hero's clips for this clip: the list marks the ones that can be used."],
        action: Some(("Open Motion", "motion")) },
    Rule { any: &["the hero this clip is taken from is not in the game", "the other hero's clip this clip is taken from is not in the game", "a clip is taken from another hero, not from this hero itself"], all: &[],
        cause: "The clip is taken from a hero or a clip this game does not have (a game update may have renamed or removed it), or from the project's own hero.",
        steps: &["Open Motion and select the clip named in the message.", "Under How the replacement is made, choose another hero and one of its clips, or another way to replace the clip.", "Build again."],
        action: Some(("Open Motion", "motion")) },
    Rule { any: &["do not share the body a clip is carried on"], all: &[],
        cause: "The two heroes' skeletons do not share the body bones a clip is carried on (the message names what is missing or hangs differently). Some heroes are built differently, for example a rider on a mount or a body without the usual spine.",
        steps: &["On the Motion page, take the clip from another hero: the hero list marks the heroes whose body cannot be carried over."],
        action: Some(("Open Motion", "motion")) },
    Rule { any: &["the other hero's clip could not be read"], all: &[],
        cause: "The app could not decode the other hero's compiled clip, so nothing can be taken from it.",
        steps: &["On the Motion page, choose another of that hero's clips, or another hero.", "If it happens with every clip after a game update, open Logs, copy the last lines and report them."],
        action: Some(("Open Motion", "motion")) },
    Rule { any: &["the part of the other hero's clip that plays does not fit"], all: &[],
        cause: "At its own speed, the part of the other hero's clip that plays starts too late: from that start the other clip ends before this hero's clip does (the game fixes this clip's length).",
        steps: &["On the Motion page, move the start of the part earlier, or let the whole clip play faster or slower to fit (stretched).", "Build again."],
        action: Some(("Open Motion", "motion")) },
    Rule { any: &["only animations can be taken from another hero", "either made from your animation or taken from another hero", "a clip taken from another hero has no file of its own",
                  "choose the other hero (its model, .vmdl) and one of its clips", "the part of the other hero's clip that plays must start",
                  "choose the other hero and the clip this clip is taken from", "choose one of this hero's clips to take from another hero"], all: &[],
        cause: "A clip taken from another hero is saved with a choice the app does not accept: on another kind of row, beside a clip made from your animation or a file of its own, or with a hero, clip or start out of range.",
        steps: &["Open Motion and select the clip.", "Under How the replacement is made, choose the other hero and its clip again (or one of the other ways), then save.", "Build again."],
        action: Some(("Open Motion", "motion")) },
    Rule { any: &["the other hero's clip or a hero's skeleton changed in the game since this build", "a clip taken from another hero changed since this build"], all: &[],
        cause: "A clip taken from another hero no longer matches this mod: the choice changed on the Motion page, or a game update changed the other hero's clip or a skeleton since the build.",
        steps: &["Open Build & install and build again.", "Install the new build."],
        action: Some(("Open Build & install", "build")) },
    // Clips made from an animation (Motion; deadlock-preparation clip_making.rs and
    // animation_export.py, deadlock-project made_clip.rs, deadlock-components
    // made_clips.rs, made_clips.rs). Before the Blender rules: some come from a Blender run.
    Rule { any: &["clips are made from an animation with blender"], all: &[],
        cause: "Making a clip from your animation reads your animation file with Blender, and no Blender is chosen.",
        steps: &["Open Settings and choose blender.exe.", "Save, then make the clip again on the Motion page."],
        action: Some(("Open Settings", "settings")) },
    Rule { any: &["a clip is made from an animation of a rigged character that the app prepared"], all: &[],
        cause: "A clip is made from an animation of your own rigged character, using what the app saved when it prepared that character for the hero. This project's model is a ready-made DMX, or was prepared as Unrigged humanoid (without a skeleton of its own), so it has nothing to animate and preparing it again would not change that.",
        steps: &["Open Prepare model and choose your rigged character file (a Blender, FBX, glTF or MMD model with its skeleton) as Rigged humanoid, then prepare it.", "Animate that character, then make the clip on the Motion page.", "Or keep this model and replace the clip with a compiled clip file instead."],
        action: Some(("Open Model preparation", "prepare")) },
    Rule { any: &["prepare the model first: a clip is made from an animation", "was prepared before clips could be made from an animation"], all: &[],
        cause: "A clip is made from an animation of your prepared character, using what the preparation saved about how it fits the hero. This project has no prepared model, or it was prepared by an earlier version of the app that did not save that.",
        steps: &["Open Prepare model and click Prepare model (your choices stay as they are).", "Open Motion and make the clip again."],
        action: Some(("Open Model preparation", "prepare")) },
    Rule { any: &["skeleton differs from your prepared character", "no armature with the bones of your prepared character"], all: &[],
        cause: "The animation file's armature is not the one your model was prepared from: a bone the animation needs is missing or was moved.",
        steps: &["Animate the file you prepared the model from, or a copy of it (an FBX or glTF file exported from it works too), without changing the armature's bones.", "On the Motion page, choose that file again and make the clip."],
        action: Some(("Open Motion", "motion")) },
    Rule { any: &["has fewer than two frames to sample", "is too long to sample"], all: &[],
        cause: "The chosen animation in your file cannot be sampled: it holds one frame only (a pose, not a motion), or it is longer than 20,000 frames.",
        steps: &["In Blender (or the program you exported the file from), give the animation at least two frames (a start and an end key) and at most 20,000, then save or export the file again.", "On the Motion page, choose the action again and make the clip."],
        action: Some(("Open Motion", "motion")) },
    Rule { any: &["this clip cannot be made from an animation"], all: &[],
        cause: "The game uses this clip in a way the app cannot write from an animation (it is added on top of other clips, or some of its bones are sampled in the model's space).",
        steps: &["On the Motion page, replace this clip with a compiled clip file instead, or make another clip from your animation."],
        action: Some(("Open Motion", "motion")) },
    Rule { any: &["path is too long for blender to open"], all: &["gltf"],
        cause: "Blender cannot open a path of 260 characters or more, and a glTF file (.gltf) cannot be copied to a shorter place by itself because its data and images are read from beside it.",
        steps: &["Move the .gltf file and the files beside it (.bin and images) to a folder with a shorter path, for example C:\\Anim, or export the animation as one .glb file.", "On the Motion page, choose the file again and make the clip."],
        action: Some(("Open Motion", "motion")) },
    Rule { any: &["is not made from its animation yet", "changed while the clip was being made", "the file has no animation named", "the part of the animation that plays does not fit",
                  "choose the animation file and the animation the clip is made from", "choose the animation (action) the clip is made from", "the part that plays must start", "the loop blend must be"], all: &[],
        cause: "A clip made from your animation needs to be made again on the Motion page: its animation file, its animation or the prepared model changed since, or a choice for it is missing or out of range.",
        steps: &["Open Motion and select the clip named in the message.", "Check the animation file, the animation and the part that plays, then make the clip again.", "Build again."],
        action: Some(("Open Motion", "motion")) },
    Rule { any: &["a clip is made from an animation in a blender, fbx or gltf file", "needs a blender, fbx or gltf file", "only animations can be made from an animation"], all: &[],
        cause: "A clip is made from an animation saved in a Blender, FBX or glTF file (.blend, .fbx, .glb or .gltf); other files and other kinds of rows cannot be used for it.",
        steps: &["On the Motion page choose your .blend, .fbx, .glb or .gltf file for the clip, or use A compiled clip file for a .vnmclip_c."],
        action: Some(("Open Motion", "motion")) },
    Rule { any: &["animation replacement needs a compiled .vnmclip_c"], all: &[],
        cause: "A clip file replacement takes a compiled clip (.vnmclip_c) or a .vnmclip that names an animation DMX. An animation in a Blender, FBX or glTF file is not a clip file: the app makes it into the clip instead.",
        steps: &["Open Motion, select the clip, and under How the replacement is made choose Make it from my animation, then choose your file and its animation.", "Or choose a .vnmclip_c or .vnmclip file for the clip."],
        action: Some(("Open Motion", "motion")) },
    Rule { any: &["a clip made from an animation changed since this build"], all: &[],
        cause: "A clip made from your animation changed on the Motion page after this mod was built, so the built clip no longer matches.",
        steps: &["Open Build & install and build again.", "Install the new build."],
        action: Some(("Open Build & install", "build")) },
    // Hero colors (Extras → Colors) recolor the game's own materials.
    Rule { any: &["hero colors change the game's own model"], all: &[],
        cause: "Hero colors recolor the game's own model, so they cannot be combined with a replacement model.",
        steps: &["Open Extras → Build options and turn off Include replacement model and textures.", "To recolor your replacement model instead, use Appearance & weapons → Surface changes.", "Build again."],
        action: Some(("Open Extras", "extras")) },
    Rule { any: &["recolor everywhere it is used"], all: &[],
        cause: "Another game model uses the same material, so recoloring it changes that model too.",
        steps: &["Open Extras → Colors and select the material named in the message.", "Turn on Recolor everywhere it is used if that is fine, or remove it and choose another material.", "Build again."],
        action: Some(("Open Extras", "extras")) },
    Rule { any: &["changed since you chose it", "is no longer one of this hero's materials", "is not one of this hero's materials"], all: &[],
        cause: "The hero's materials in the game changed since you chose this color, usually after a game update.",
        steps: &["Open Extras and click Reload assets.", "Open Colors, check the material and its color, then build again."],
        action: Some(("Open Extras", "extras")) },
    // Extras: commands that need the character's assets loaded (catalogs.rs).
    Rule { any: &["load this character’s assets first", "load this character's assets first"], all: &[],
        cause: "This needs the hero's game assets loaded in Extras, and they are not loaded for this hero and game.",
        steps: &["Open Extras and click Reload assets.", "Then open the tab you were using again."],
        action: Some(("Open Extras", "extras")) },
    // The effect editor (catalogs.rs effect_trees / effect_preview, effect_trees.rs).
    Rule { any: &["the effect list is not loaded", "is not one of this hero's effects", "the game files changed since the effects were loaded"], all: &[],
        cause: "The effect list shown is not the one loaded for this hero, or the game was updated since it was loaded.",
        steps: &["Open Extras and click Reload assets.", "Open Effects again and choose the effect."],
        action: Some(("Open Extras", "extras")) },
    // Hero color ranges recolor the base-color texture.
    Rule { any: &["select none of its base color"], all: &[],
        cause: "The material's color ranges match none of its base-color pixels, so they would change nothing.",
        steps: &["Open Extras → Colors and select the material named in the message.", "Turn on Show selection and click Pick color on the preview to center a range on the part you want, or widen the hue, saturation or brightness range.", "Build again."],
        action: Some(("Open Extras", "extras")) },
    Rule { any: &["color ranges cannot change", "color ranges need a bc7"], all: &[],
        cause: "This material's base-color texture is stored in a form that color ranges cannot rewrite.",
        steps: &["Open Extras → Colors and select the material named in the message.", "Remove its color ranges and use Tint, Saturation and Brightness instead.", "Build again."],
        action: Some(("Open Extras", "extras")) },
    // Color ranges that give a new color or change one part (`color_ranges` switches, `color_parts`).
    Rule { any: &["cannot give a color range a new color of its own yet", "cannot recolor one part only yet"], all: &[],
        cause: "This color range uses something this version of the app keeps switched off until it has been checked in the game.",
        steps: &["Open Extras → Colors and select the material named in the message.", "Remove that color range, or set it to Everywhere this color when it changes one part only.", "Build again."],
        action: Some(("Open Extras", "extras")) },
    Rule { any: &["is no longer on a part of the model"], all: &[],
        cause: "The spot a one-part color range was picked on is not on any part of the model any more, usually because a game update changed the model.",
        steps: &["Open Extras → Colors and select the material named in the message.", "Pick the part again on the character, or set the range to Everywhere this color.", "Build again."],
        action: Some(("Open Extras", "extras")) },
    Rule { any: &["the parts of the model could not be read"], all: &[],
        cause: "The build could not read the model's parts that a color range for one part needs.",
        steps: &["For the game's own hero: open Extras, click Reload assets and wait for the hero's model to appear on the Colors page.", "For your own model: prepare it again on Prepare model.", "Or set the range to Everywhere this color, then build again."],
        action: Some(("Open Extras", "extras")) },
    Rule { any: &["so a color range cannot change one part of it"], all: &[],
        cause: "The material has one plain base color and no image, so there is no part of it to recolor on its own.",
        steps: &["Open Extras → Colors and select the material named in the message.", "Set the range to Everywhere this color, or use Tint.", "Build again."],
        action: Some(("Open Extras", "extras")) },
    Rule { any: &["the hero's model changed since this mod was built"], all: &[],
        cause: "The game's hero model changed since the mod was built (a game update), and a color range changes one part of it.",
        steps: &["Open Build & install and build again.", "Install the new build."],
        action: Some(("Open Build & install", "build")) },
    Rule { any: &["new color needs both its hue and its saturation"], all: &[],
        cause: "A saved color range holds only half of a new color, so it cannot be built.",
        steps: &["Open Extras → Colors and select the material.", "Remove that color range and pick its color again.", "Save the project."],
        action: Some(("Open Extras", "extras")) },
    Rule { any: &["the game texture of", "no longer names the base-color texture"], all: &[],
        cause: "The game's base-color texture of this material changed since the mod was built, usually after a game update.",
        steps: &["Open Build & install and build again.", "Install the new build."],
        action: Some(("Open Build & install", "build")) },
    Rule { any: &["hero colors support pbr.vfx materials only", "has no tint setting", "has no saturation or brightness setting", "changes nothing. set a tint", "could not be read as a compiled material"], all: &[],
        cause: "This hero material does not have the color setting you changed.",
        steps: &["Open Extras → Colors and select the material named in the message.", "Use the controls it offers, or remove it and choose another material.", "Build again."],
        action: Some(("Open Extras", "extras")) },
    Rule { any: &["particlefloattype_t", "schema mismatch"], all: &[],
        cause: "The character model was compiled with a compiler that cannot build hero models, such as Reduced CSDK 12's. Hero models need Reduced CSDK 13.",
        steps: MODEL_TOOLSET_STEPS, action: Some(("Open Settings", "settings")) },
    Rule { any: &["model compiler is not set up", "model compiler is missing", "model game configuration is missing", "model compiler executable must be"], all: &[],
        cause: "No model compiler is selected (Reduced CSDK 13 as the Community SDK compiles the hero model), or the selected one is incomplete.",
        steps: MODEL_TOOLSET_STEPS, action: Some(("Open Settings", "settings")) },
    Rule { any: &["no valid format conversion"], all: &["modeldoc"],
        cause: "The selected model compiler is not the version the app needs (it cannot read ModelDoc 41).",
        steps: &["Open Settings → Advanced configuration → Downloaded model toolset → Set up model compiler….", "Download the pinned toolset, or click Verify existing acquisition for the one you have.", "Click Test original hero compatibility, then Use this toolset in settings.", "Build again."],
        action: Some(("Set up model compiler", "setup")) },
    // DepotDownloader asked for a password nobody can type: tool runs have no
    // console and no input. It then crashes in SteamKit's LogOn.
    Rule { any: &["enter account password", "logon requires a username and password or access token", "did you forget to specify -remember-password"], all: &[],
        cause: "DepotDownloader has no saved Steam sign-in for this account. The app runs it in the background without a console window, so it cannot ask for your password.",
        steps: &["On Model tool setup, click Sign in to Steam… under Steam username.", "Type your Steam password and Steam Guard code in the DepotDownloader window that opens. The app never sees them; DepotDownloader saves the sign-in and the window closes by itself.", "Click Fetch manifests and plan again.", "Or choose Steam mobile QR sign-in and scan the code that appears on screen with the Steam app."],
        action: Some(("Open Model tool setup", "setup")) },
    Rule { any: &["sign-in window closed before the sign-in was saved"], all: &[],
        cause: "DepotDownloader's sign-in window closed before Steam accepted the sign-in (for example after a wrong password or Steam Guard code).",
        steps: &["Click Sign in to Steam… again.", "In the window that opens, type the password of the Steam account named in Steam username, then the Steam Guard code from your phone or email.", "Wait until the window closes by itself; the app then says Signed in."],
        action: Some(("Open Model tool setup", "setup")) },
    Rule { any: &["access token was rejected"], all: &[],
        cause: "The Steam sign-in DepotDownloader saved has expired or was signed out.",
        steps: &["On Model tool setup, click Sign in to Steam… and sign in again.", "Click Fetch manifests and plan again."],
        action: Some(("Open Model tool setup", "setup")) },
    // The QR code was not scanned before Steam gave up (or the sign-in was refused).
    Rule { any: &["failed to authenticate with steam", "initializesteam failed"], all: &[],
        cause: "Steam sign-in did not finish: the QR code was not scanned in time, or Steam refused the sign-in.",
        steps: &["Click Fetch manifests and plan again. When the Steam QR code appears on screen, scan it with the Steam app on your phone (Steam Guard → scan a QR code). It renews every few seconds; scan the one shown.", "Your account needs Counter-Strike 2 in its library (it is free); installing it is not needed.", "If Steam reports too many attempts, wait 30 minutes and try again.", "Or choose a locally authenticated Steam account and click Sign in to Steam…."],
        action: Some(("Open Model tool setup", "setup")) },
    Rule { any: &["logon", "login", "password", "steam guard", "rate limit", "ratelimit", "access token"], all: &["steam"],
        cause: "Steam sign-in did not finish inside DepotDownloader.",
        steps: &["Use Steam mobile QR sign-in and scan the code that appears on screen with the Steam app on your phone.", "With a Steam username, click Sign in to Steam… first so DepotDownloader can save the sign-in.", "Your account needs Counter-Strike 2 in its library (it is free); installing it is not needed.", "If Steam reports too many attempts, wait 30 minutes and try again."],
        action: Some(("Open Logs", "logs")) },
    Rule { any: &["choose depotdownloader.exe", "depotdownloader.exe was not found", "is the downloaded zip. extract it first (right-click → extract all), then select the depotdownloader.exe", "select the depotdownloader.exe inside the extracted"], all: &[],
        cause: "The app could not find DepotDownloader.exe at the path you gave.",
        steps: &["Download DepotDownloader-windows-x64.zip with the link on the Set up model compiler… page.", "Right-click the ZIP → Extract All.", r"In DepotDownloader executable, select the DepotDownloader.exe inside the extracted folder, for example C:\Tools\DepotDownloader\DepotDownloader.exe.", "Click Fetch manifests and plan again."],
        action: Some(("Open model tool setup", "setup")) },
    Rule { any: &["depotdownloader"], all: &["failed"],
        cause: "DepotDownloader stopped with an error while talking to Steam or downloading.",
        steps: &["Open Logs and read DepotDownloader's last lines; they say whether sign-in, the licence or the network failed.", "Use Steam mobile QR sign-in and scan the code that appears on screen; the account needs Counter-Strike 2 in its library (free).", "Check your internet connection and free disk space, then click Fetch manifests and plan again. A download continues where it stopped."],
        action: Some(("Open Logs", "logs")) },
    Rule { any: &["nothing has been downloaded into"], all: &[],
        cause: "Verify only checks a finished download, and this folder has none yet.",
        steps: &["Click Fetch manifests and plan and approve the Steam sign-in.", "Review the plan, then click Download.", "Verify runs by itself after the download."],
        action: Some(("Open model tool setup", "setup")) },
    Rule { any: &["community sdk is incomplete", "folder around the sdk", "sdk game configuration is missing", "choose the community sdk folder"], all: &[],
        cause: "Settings → Community SDK does not point at a complete Community SDK folder (Reduced CSDK 13, or the older Reduced CSDK 12).",
        steps: &[r"Open the Reduced_CSDK_13 folder you extracted. The right folder directly contains the folders game and content.", r"If there is a Reduced_CSDK_13 folder inside another Reduced_CSDK_13 folder (the ZIP extracts that way), choose the inner one, or move it to C:\CSDK13.", "In Settings, click Detect tools, then Check paths; fix any field marked red.", "If game\\citadel\\gameinfo.gi is missing, extract the whole ZIP again into a new, empty folder."],
        action: Some(("Open Settings", "settings")) },
    Rule { any: &["resource compiler points outside", "select the material compiler", "select resourcecompiler.exe", "the resource compiler must belong"], all: &[],
        cause: "Settings → Resource compiler is not the Community SDK's own material compiler.",
        steps: &[r"Set Resource compiler to <Community SDK>\game\bin\win64\resourcecompiler.exe, for example C:\CSDK13\game\bin\win64\resourcecompiler.exe (in the older Reduced CSDK 12 it is game\bin_cs2\win64). Detect tools fills it in.", "The model toolset's compiler belongs only under Advanced configuration → Model compiler executable.", "Click Check paths, then build again."],
        action: Some(("Open Settings", "settings")) },
    Rule { any: &["not enough space on the disk", "os error 112", "no space left"], all: &[],
        cause: "The drive with the workspace ran out of free space.",
        steps: &["Open Settings → Storage and click the button that clears build data (caches the next build recreates).", "Empty the Recycle Bin.", "A build needs several GB free; move the workspace to a drive with more space if needed."],
        action: Some(("Open Settings", "settings")) },
    Rule { any: &["os error 32", "being used by another process", "os error 33"], all: &[],
        cause: "Another program has a file open that the app needs to change.",
        steps: &["Close Deadlock (the game locks installed VPK files).", "Close Blender and any File Explorer window showing the project or addon folder.", "If the project or SDK is inside OneDrive, pause syncing while you build.", "Try again."],
        action: None },
    Rule { any: &["access is denied", "os error 5)", "permission denied"], all: &[],
        cause: "Windows refused access to a file or folder.",
        steps: &["Close Deadlock, Blender and File Explorer windows showing the project, then try again.", r"Avoid protected folders such as C:\Program Files for the workspace, the SDK and the toolset; use for example C:\CSDK13.", "If an antivirus quarantined resourcecompiler.exe or DepotDownloader.exe, restore it and allow it."],
        action: None },
    // MMD models (.pmx, .pmd) are imported with the MMD Tools add-on in the
    // user's Blender (deadlock-preparation mmd_source.py; the tests read the
    // phrases from that file).
    Rule { any: &["mmd tools is not installed for blender"], all: &[],
        cause: "MMD models (.pmx and .pmd files) are imported with the MMD Tools add-on for Blender, and the Blender this app uses does not have it.",
        steps: &["Open the Blender set in Settings → Blender executable: the version named in the message. The add-on must be installed in that same version.", "In Blender, open Edit → Preferences → Get Extensions. If Blender asks, allow online access.", "Search for MMD Tools and click Install, then close Blender. The MMD Tools page explains the installation: https://extensions.blender.org/add-ons/mmd-tools/ (Settings → Blender executable has the link).", "Prepare the model again.", "Or import the model in Blender with MMD Tools yourself, save it as a .blend file and choose that file instead."],
        action: Some(("Open Settings", "settings")) },
    Rule { any: &["could not be turned on"], all: &["mmd tools is installed for blender"],
        cause: "MMD Tools is installed in the Blender this app uses, but it failed to load.",
        steps: &["In Blender, open Edit → Preferences → Get Extensions and update MMD Tools, or remove it and install it again.", "Check that the MMD Tools version supports this Blender version (its page lists the Blender versions).", "Close Blender and prepare the model again.", "Or import the model in Blender with MMD Tools yourself, save it as a .blend file and choose that file instead."],
        action: Some(("Open Logs", "logs")) },
    Rule { any: &["mmd tools could not read"], all: &[],
        cause: "MMD Tools could not read the model file: it is damaged, cut off, or in a form the add-on does not support.",
        steps: &["Download or extract the model again; a cut-off download often causes this.", "Check that the file opens in MikuMikuDance or PMX Editor.", "Or import it in Blender with MMD Tools yourself, save it as a .blend file and choose that file instead."],
        action: Some(("Open Character & model", "project")) },
    Rule { any: &["too long for blender: its path is"], all: &[],
        cause: "Your model file, or the Blender copy the app makes of it in the workspace, has a path too long for Blender.",
        steps: &[r"If the message names your model file, move its folder to a shorter path such as C:\Models and choose the model again.", r"If it names the copy, open Settings and choose a shorter Workspace folder, for example C:\DMB.", "Prepare the model again."],
        action: Some(("Open Settings", "settings")) },
    // FBX staging searches the model's folder for its textures (issue #7).
    Rule { any: &["fbx texture folder too large", "too many files to search for its textures"], all: &[],
        cause: "Your model's folder holds too many files for the app to search it for the model's textures, for example when the model sits in Downloads.",
        steps: &["Make a new folder for the model alone and move (or export) the model file and its textures into it, keeping any texture subfolders.", "In Character & model, choose the model file in its new folder.", "Prepare the model again."],
        action: Some(("Open Character & model", "project")) },
    // Issue #7: Blender cannot open a staged file from 260 characters on.
    // Textures get a shorter copy by themselves (source.rs shorter_copies);
    // these messages remain for the model file, a glTF data file, and a
    // workspace too long for even a short name.
    Rule { any: &["too long for blender"], all: &[],
        cause: "A file of your model gets a path too long for Blender once the app copies it into the workspace, and the app cannot shorten that path by itself.",
        steps: &["If the message names your model file, give it a shorter name and choose it again in Character & model. If it names a glTF data file (.bin), export the model again with shorter folder and file names.", r"Or open Settings and choose a shorter Workspace folder, for example C:\DMB. This is the only way when the message says even a short name is too long.", "Prepare the model again."],
        action: Some(("Open Settings", "settings")) },
    Rule { any: &["os error 206", "filename or extension is too long", "path is too long", "too long for the compiler"], all: &[],
        cause: "A file path is longer than Windows allows.",
        steps: &[r"If the message names a file of your model, give it a shorter name, or choose a shorter Workspace folder in Settings, for example C:\DMB.", r"If it names the model compiler or the SDK, move the Community SDK folder to a short path such as C:\CSDK13 and update Settings → Community SDK and Resource compiler.", "In Model preparation, click Prepare again so material names are shortened.", "Try again."],
        action: Some(("Open Settings", "settings")) },
    Rule { any: &["incorrect function. (os error 1)"], all: &[],
        cause: "A path points at a drive or a location Windows cannot use as a normal folder.",
        steps: &[r"Choose a real folder, not a drive letter on its own (C:\ or D:\).", "Network drives, cloud placeholders and virtual drives can fail; use a folder on a local disk.", "Click Check paths in Settings."],
        action: Some(("Open Settings", "settings")) },
    Rule { any: &["os error 2)", "os error 3)", "cannot find the file specified", "cannot find the path specified"], all: &[],
        cause: "A file or folder the app expected is not there anymore (moved, renamed or deleted).",
        steps: &["Click Check paths in Settings and fix any field marked red.", "If your model file moved, choose it again in Character & model.", "Reopen the project if its folder moved."],
        action: Some(("Open Settings", "settings")) },
    Rule { any: &["blender is not configured", "blender executable not found"], all: &[],
        cause: "Blender is needed to prepare Blender, FBX, glTF and MMD models, and the app cannot find it.",
        steps: &["Install Blender 5.2 LTS (Settings → Get programs has the link).", r"In Settings → Advanced configuration → Blender executable, select blender.exe, for example C:\Program Files\Blender Foundation\Blender 5.2\blender.exe.", "Click Save settings and prepare again."],
        action: Some(("Open Settings", "settings")) },
    // Blender's own import messages for a model file it cannot read (the ability
    // object's file has its own rule above). Before the general Blender rule: the
    // cause is the file, not Blender.
    Rule { any: &["ascii fbx files are not supported"], all: &[],
        cause: "Blender cannot read this FBX file. It is an ASCII (text) FBX, or not an FBX file at all; Blender reads only binary FBX files.",
        steps: &["Export the file again as a binary FBX (in most programs the FBX export has a Binary or ASCII choice), or export it as glTF (.glb), or save it as a .blend.", "Choose the new file where you chose this one (Character & model for your model, the Gun tab of Prepare model for a gun file, Motion for an animation) and try again."],
        action: None },
    Rule { any: &["file format is not supported", "failed to read blend file", "not a blend file", "couldn't open file", "bad gltf:", "bad glb:", "unsupported or invalid glb"], all: &[],
        cause: "Blender cannot read the file you chose: it is damaged or incomplete (a download or copy that did not finish), or it is not really the kind of file its name says.",
        steps: &["Export or download the file again, as a .blend, a binary FBX or a glTF (.glb), and check that it opens in Blender on its own.", "Choose the new file where you chose this one (Character & model for your model, the Gun tab of Prepare model for a gun file, Motion for an animation) and try again."],
        action: None },
    Rule { any: &["export_scene_ot_gltf"], all: &[],
        cause: "Blender failed while exporting the model for the preview.",
        steps: &["Open Logs and look for the Python error under EXPORT_SCENE_OT_gltf.", "Use Blender 5.2 LTS (Settings → Blender executable, then Tool diagnostics → Validate Blender).", "In Blender, apply or remove broken modifiers and save the file, then try again."],
        action: Some(("Open Logs", "logs")) },
    Rule { any: &["blender"], all: &["exit code"],
        cause: "Blender stopped while preparing your model.",
        steps: &["Open Logs and look for the lines starting with Traceback or Error just before the failure.", "Check that the file opens in Blender on its own.", "Use a supported Blender: 4.2–4.5, 5.0, 5.2 LTS or 5.3 (5.2 LTS is recommended). Set it in Settings → Blender executable and click Validate Blender.", "Save the file as a new .blend without add-on data and prepare again."],
        action: Some(("Open Logs", "logs")) },
    Rule { any: &["confirm replacement of the existing output"], all: &[],
        cause: "This VPK slot already holds a file from this project.",
        steps: &["On Build & install, confirm that the old file may be replaced, or choose another free slot.", "Close Deadlock first if it is running."],
        action: Some(("Open Build & install", "build")) },
    // Automatic motion without kept rig bones (issue #5: the old message here
    // never matched this rule, so it came with no help).
    Rule { any: &["prepared secondary group is missing", "has no moving-part bones", "keep existing physics bones is off"], all: &[],
        cause: "Automatic secondary motion moves the bones your model's rig already has for hair, cloth and tails, and the model in use has none of them kept.",
        steps: &["Open Model preparation → Hair, cloth and other moving parts and turn on Keep existing physics bones.", "Click Convert, then Use prepared model.", "Run Analyze again, then build.", "If no moving parts are listed there, your model's clothes have no bones of their own and cannot move; Preserve original physics keeps only the hero's own hair and cloth."],
        action: Some(("Open Model preparation", "prepare")) },
    // The compiled model's per-mesh bone list does not fit its skeleton (issue #6;
    // the message names the mesh and the value).
    Rule { any: &["invalid compiled mesh remapping table"], all: &[],
        cause: "The model compiler wrote a mesh whose bone list does not fit the compiled skeleton, so the build cannot check how that mesh moves. The message names the mesh.",
        steps: &["Open Build & install → Analysis details and fix any note about meshes without skin weights or meshes no render node uses (Mesh mapping).", "Prepare the model again (Model preparation → Convert, then Use prepared model), then Analyze and build.", "If it still happens, send the Copy for support text and the saved log (Logs → Save log)."],
        action: Some(("Open Build & install", "build")) },
    // A face of the prepared model crosses itself (deadlock-preview triangulate.rs);
    // flat folded faces are no longer refused.
    Rule { any: &["cannot triangulate"], all: &[],
        cause: "A face of your model crosses over itself (a bow-tie shaped face), so its surface cannot be split into triangles for automatic movement.",
        steps: &["In Blender, select the character's (or gun's) meshes, enter Edit Mode, select all, and use Mesh → Clean Up → Degenerate Dissolve, then Face → Triangulate Faces for anything still twisted.", "Save the file, then Convert and Use prepared model in Model preparation.", "Run Analyze again."],
        action: Some(("Open Model preparation", "prepare")) },
    // Two kept bones the model compiler writes with one name (skirt panels
    // such as 侧裙/前裙/后裙): found at Analyze, or after compiling a model
    // prepared before the app gave such bones distinct names.
    Rule { any: &["would get the same name in the compiled model", "missing or ambiguous generated bone"], all: &[],
        cause: "Two of your model's moving-part bones get the same name when the model compiler writes them: it turns every character other than A–Z, 0–9 and _ into _, so names such as 侧裙_0_0 and 前裙_0_0 become one. The model was prepared before the app gave such bones distinct names.",
        steps: &["Open Model preparation and click Convert, then Use prepared model. The app now gives these bones distinct names; your moving-part choices stay.", "Run Analyze again, then build."],
        action: Some(("Open Model preparation", "prepare")) },
    // A game update changed the hero's model after preparation.
    Rule { any: &["skeleton reference belongs to different original model"], all: &[],
        cause: "The game updated this hero's model after your character was prepared, so the prepared model no longer matches the hero.",
        steps: &["Open Model preparation and click Convert, then Use prepared model. Your choices are kept.", "Run Analyze again, then build."],
        action: Some(("Open Model preparation", "prepare")) },
    Rule { any: &["model fitting changed", "convert the original source"], all: &[],
        cause: "Preparation settings changed after the model was converted.",
        steps: &["Open Model preparation, click Convert, then Use prepared model.", "Build again."],
        action: Some(("Open Model preparation", "prepare")) },
    Rule { any: &["bone identities differ", "must retain every original bone", "missing original identities", "replacement dmx skeleton"], all: &[],
        cause: "The prepared model does not belong to the selected hero (the hero changed, or the model was prepared for another one).",
        steps: &["Check the hero in Character & model.", "Open Model preparation and prepare the model again for this hero, then click Use prepared model.", "Build again."],
        action: Some(("Open Model preparation", "prepare")) },
    Rule { any: &["compiled skeleton bind transform changed"], all: &[],
        cause: "A moving-part bone came out of the compiler in a different place than prepared.",
        steps: &["Prepare the model again (Convert, then Use prepared model) and build.", "If it happens again, turn off the moving part named in the message on Physics & preview and build."],
        action: Some(("Open Physics & preview", "physics")) },
    // Replacement cloth setup with the game's own model as the tested setup (issue #4).
    Rule { any: &["is the hero's own game cloth"], all: &[],
        cause: "The tested cloth reference is the hero's own game model (or carries its cloth). Replacement cloth setup needs a hair and cloth setup compiled and tested for your replacement model; the game's own models fail its checks.",
        steps: &["Open Physics & preview and choose Preserve original physics to keep the hero's own hair and cloth.", "Or choose Automatic secondary motion to make your model's own hair and clothes move.", "Analyze project again, then build."],
        action: Some(("Open Physics & preview", "physics")) },
    Rule { any: &["cloth proxy face", "fabric fold constraint", "native secondary fitting is incomplete", "cloth validation"], all: &[],
        cause: "The motion setup for one or more moving parts could not be built.",
        steps: &["Open Physics & preview and find the parts named in the message.", "Set their part type to Rigid (or switch them off), then Analyze again.", "Build again. Other moving parts keep their motion."],
        action: Some(("Open Physics & preview", "physics")) },
    // An older Source 2 Viewer CLI that does not know an option the app passes (for
    // example --skip_softbody_reconstruction, the second decompile of an ability object
    // with cloth). 21.1 answers "Argument '--…' is not recognized." with exit code 1.
    Rule { any: &["' is not recognized.", "unrecognized command or argument", "unrecognized option"], all: &["source2viewer"],
        cause: "Your Source 2 Viewer CLI is older than this app needs: it does not know an option the app passes to it (the message names the option).",
        steps: &["Update Source 2 Viewer CLI: download the latest version (Settings → Get programs) and replace the old Source2Viewer-CLI.exe.", "If the new Source2Viewer-CLI.exe is in another folder, choose it in Settings → Source 2 Viewer CLI and click Save settings.", "Try again."],
        action: Some(("Open Settings", "settings")) },
    // Source 2 Viewer's summary when it exits with code 2; it runs in the temp
    // folder, where it leaves exceptions.txt (compiler.rs source2viewer).
    Rule { any: &["file(s) failed to process"], all: &[],
        cause: "Source 2 Viewer could not decompile one or more of the hero's game files, usually after a Deadlock update or with an older Source 2 Viewer CLI.",
        steps: &["Download the latest Source 2 Viewer CLI (Settings → Get programs) and replace the old Source2Viewer-CLI.exe.", "If it still fails, open the exceptions.txt named in the message (in your temp folder) or Logs: they name the game file that could not be read. Check for a newer version of this app.", "Analyze or build again."],
        action: Some(("Open Settings", "settings")) },
    // Before the next rule: these messages contain "decompile" too, but nothing
    // failed to read (sources.rs ensure_base, compiler.rs decompile_from_archive).
    Rule { any: &["select source2viewer cli in setup", "auto-decompile is disabled"], all: &[],
        cause: "The hero's own game files have not been extracted yet, and the app cannot extract them: Source 2 Viewer CLI is not set up (or automatic extraction is off for this project).",
        steps: &["Open Settings and choose Source2Viewer-CLI.exe in the Source 2 Viewer CLI field (Settings → Get programs has the download).", "Click Save settings, then try again."],
        action: Some(("Open Settings", "settings")) },
    Rule { any: &["unsupported or malformed kv3", "decompil"], all: &[],
        cause: "Source 2 Viewer could not read a game file, usually after a Deadlock update.",
        steps: &["Download the latest Source 2 Viewer CLI (Settings → Get programs) and replace the old Source2Viewer-CLI.exe.", "Check for a newer version of this app.", "Build again."],
        action: Some(("Open Settings", "settings")) },
    Rule { any: &["choose a workspace"], all: &[],
        cause: "No workspace folder is set.",
        steps: &["In Settings, click Detect tools to use the default workspace, or choose a folder.", "Click Save settings."],
        action: Some(("Open Settings", "settings")) },
    Rule { any: &["select an original hero", "choose a hero", "choose the original character"], all: &[],
        cause: "No hero is selected for this mod.",
        steps: &["Open Character & model and pick the hero your mod replaces."],
        action: Some(("Open Character & model", "project")) },
    Rule { any: &["resource compiler failed"], all: &[],
        cause: "The compiler reported an error.",
        steps: &["Open Logs and look for lines with ERROR; they name the file that failed.", "Click Check paths in Settings; every compiler field should be green.", "Missing or unreadable textures: open Appearance & weapons and check the materials listed in the error."],
        action: Some(("Open Logs", "logs")) },
];

const GENERIC_TOOL: &[&str] = &["Open Logs for the full tool output; the last lines before the failure usually say what is wrong.", "Click Check paths in Settings and fix any field marked red.", "Try again."];
const GENERIC: &[&str] = &["Try again.", "If it keeps happening, open Logs, copy the lines around the failure and send them with the technical details below."];

/// Help for an error. `kind` is the AppError kind (cancelled errors get none).
pub fn help(kind: &str, message: &str, details: &str) -> Option<Help> {
    if kind == "cancelled" {
        return None;
    }
    let text = format!("{message}\n{details}").to_lowercase();
    if let Some(rule) = RULES.iter().find(|r| r.any.iter().any(|w| text.contains(w)) && r.all.iter().all(|w| text.contains(w))) {
        return Some(Help {
            cause: rule.cause.into(),
            steps: rule.steps.iter().map(|s| (*s).into()).collect(),
            action: rule.action.map(|(label, page)| HelpAction { label: label.into(), page: page.into() }),
        });
    }
    let (cause, steps) = match kind {
        "tool" => ("An external tool stopped with an error.", GENERIC_TOOL),
        "validation" => return None,
        _ => ("Something unexpected went wrong.", GENERIC),
    };
    Some(Help { cause: cause.into(), steps: steps.iter().map(|s| (*s).into()).collect(), action: (kind == "tool").then(|| HelpAction { label: "Open Logs".into(), page: "logs".into() }) })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn page(message: &str) -> Option<String> {
        help("validation", message, "").and_then(|h| h.action).map(|a| a.page)
    }
    /// The preparation review's messages for saved choices the body roles no longer
    /// allow, as the review, a refused conversion and the Blender copy deliver them.
    #[test]
    fn stale_preparation_choices_point_to_prepare_model() {
        use deadlock_preparation::{following, secondary_bones};
        let groups = secondary_bones::stale_groups_message();
        let one = following::stale_message(&["Hair_01".into()]);
        let many = following::stale_message(&(0..20).map(|i| format!("Bone_{i:02}")).collect::<Vec<_>>());
        for message in [groups.clone(), one, many.clone(), format!("Humanoid review needs attention before Blender starts:\n{many}")] {
            let help = help("validation", &message, "").unwrap();
            assert_eq!(help.action.as_ref().map(|a| a.page.as_str()), Some("prepare"), "{message}");
            // The steps name the page's current labels (deadlock-preparation page_names.rs).
            use deadlock_preparation::page_names::{JOINT_MATCHES_TAB, MOVING_PARTS_TAB, REFRESH_MATCHES, REFRESH_THE_MATCHES, USE_DETECTED_GROUPS};
            for label in [JOINT_MATCHES_TAB, MOVING_PARTS_TAB, REFRESH_MATCHES, REFRESH_THE_MATCHES, USE_DETECTED_GROUPS] {
                assert!(help.steps.iter().any(|s| s.contains(label)), "{label}: {help:?}");
            }
        }
        let blender = help("tool", "Blender preparation failed", &format!("Traceback (most recent call last):\n  File \"worker.py\", line 1\nValueError: {groups}")).unwrap();
        assert_eq!(blender.action.unwrap().page, "prepare");
    }
    /// The kept moving parts as a whole: too many bones, or two groups that share
    /// bones, as the review, a refused conversion and the Blender copy deliver
    /// them; and the earlier sentence that older reports and logs may hold.
    #[test]
    fn kept_bone_limit_and_shared_bones_point_to_the_moving_parts_tab() {
        use deadlock_preparation::{page_names::{LOAD_AND_CHECK, MOVING_PARTS_TAB, USE_DETECTED_GROUPS}, secondary_bones::{self, PhysicsGroup, MAX_KEPT_BONES}};
        let group = |root: &str, count: usize| PhysicsGroup { root: root.into(), source_anchor: "head".into(), target_anchor: "head".into(),
            source_bones: (0..count).map(|i| format!("{root}_{i:03}")).collect(), weighted_bones: vec![] };
        let groups = [group("hair_a", MAX_KEPT_BONES / 2), group("tail_a", MAX_KEPT_BONES + 1 - MAX_KEPT_BONES / 2)];
        let too_many = secondary_bones::too_many_bones_message(MAX_KEPT_BONES + 1, &groups);
        assert_eq!(secondary_bones::kept_bones_problem(&groups).as_deref(), Some(too_many.as_str()));
        let shared = secondary_bones::shared_bones_message("hair_a", "hair_b");
        let earlier = format!("Selected physics branches overlap or exceed {MAX_KEPT_BONES} bones.");
        for (message, cause) in [(too_many.clone(), "more bones in total"), (earlier, "more bones in total"), (shared.clone(), "contain the same bones")] {
            for (kind, text, details) in [("validation", message.clone(), String::new()),
                ("validation", format!("Humanoid review needs attention before Blender starts:\nMoving parts: {message}"), String::new()),
                ("tool", "Blender preparation failed".to_string(), format!("Traceback (most recent call last):\n  File \"worker.py\", line 1\nValueError: {message}"))] {
                let help = help(kind, &text, &details).unwrap();
                assert!(help.cause.contains(cause), "{text}: {help:?}");
                assert_eq!(help.action.as_ref().map(|a| a.page.as_str()), Some("prepare"), "{text}");
                assert!(help.steps.iter().any(|s| s.contains(MOVING_PARTS_TAB)), "{help:?}");
            }
        }
        assert!(help("validation", &shared, "").unwrap().steps.iter().any(|s| s.contains(USE_DETECTED_GROUPS) && s.contains(LOAD_AND_CHECK)));
        // The page's count line is what the first step tells the user to read.
        assert!(help("validation", &too_many, "").unwrap().steps[0].contains("kept bones count"));
    }
    /// Messages copied from real failure logs.
    #[test]
    fn real_failures_point_to_the_page_that_fixes_them() {
        for (message, expected) in [
            ("The TangTang presets are not included in this build. Choose Automatic for the movement.", "physics"),
            // The fix is choosing Reduced CSDK 13, in Settings.
            ("Compiler startup schema mismatch (ParticleFloatType_t). This happens before VMDL loading", "settings"),
            (r"Model compiler is missing: \\?\C:\Deadlockmoddingtools\Reduced_CSDK_12\game\bin_cs2\win64\resourcecompiler.exe.", "settings"),
            (r"The Community SDK is incomplete: \\?\C:\Deadlockmoddingtools\Reduced_CSDK_12\game\citadel\gameinfo.gi is missing.", "settings"),
            // Reduced CSDK 13: toolchain::MODEL_TOOLSET_NOT_SET and sdk_game's messages.
            (deadlock_tools::toolchain::MODEL_TOOLSET_NOT_SET, "settings"),
            ("The model compiler is not set up. Choose Reduced CSDK 13 as the Community SDK in Settings: it compiles the hero model too, so nothing else needs installing.", "settings"),
            ("The model compiler is not set up. Reduced CSDK 12 cannot compile hero models; Reduced CSDK 13 replaces it and compiles materials and the hero model, so choose it as the Community SDK. An already downloaded model toolset can still be chosen under Advanced configuration.", "settings"),
            (r"Select the material compiler in Settings → Resource compiler: \\?\C:\Tools\Reduced_CSDK_13\Reduced_CSDK_13\game\bin\win64\resourcecompiler.exe.", "settings"),
            (r"Settings → Community SDK points to the folder around the SDK. Choose \\?\C:\Tools\Reduced_CSDK_13\Reduced_CSDK_13 instead (the SDK was extracted inside a folder of the same name).", "settings"),
            ("Choose DepotDownloader.exe. Download DepotDownloader-windows-x64.zip", "setup"),
            (r"Nothing has been downloaded into C:\Tools\DMBTools yet.", "setup"),
            (r"Copy staged archive: There is not enough space on the disk. (os error 112)", "settings"),
            (r"Confirm replacement of the existing output before building: C:\w\output\pak18_dir.vpk", "build"),
            ("Prepared secondary group is missing: Bone_Breast_L_00. Prepare with Keep existing physics bones.", "prepare"),
            // Issue #5: secondary.rs PRESERVE_OFF and NO_MOVING_PART_BONES.
            ("Automatic secondary motion moves the hair, cloth and tail bones kept from your model's rig, but Keep existing physics bones is off. In Model preparation, turn it on, click Convert, then Use prepared model, and Analyze again.", "prepare"),
            ("The model in use has no moving-part bones kept from your rig, so automatic secondary motion has nothing to move. If you turned on Keep existing physics bones after converting, click Convert and then Use prepared model in Model preparation, and Analyze again. Clothes weighted only to body bones have no bones of their own to move.", "prepare"),
            ("Model fitting changed. Convert the original source and choose Use prepared model", "prepare"),
            // Issue #6, with the detail the audit now gives.
            ("Invalid compiled mesh remapping table: mesh 1 (Face) refers to bone 5 but the compiled skeleton has 3 bones (2 such entries in 1 meshes)", "build"),
            // Analyze on a model with a crossing face (a gun mesh, before folded faces were accepted).
            ("Integrity check failed: Preview cannot triangulate a self-intersecting polygon", "prepare"),
            ("Integrity check failed: Preview cannot triangulate this DMX polygon; check for self-intersections", "prepare"),
            // Skirt panels the compiler would merge, after compiling and (with the new check) at Analyze.
            ("Cloth validation: Missing or ambiguous generated bone: dmb_secondary_侧裙_0_0", "prepare"),
            ("Bones dmb_secondary_侧裙_0_0 and dmb_secondary_前裙_0_0 would get the same name in the compiled model (dmb_secondary________0_0): the model compiler writes every character other than A–Z, 0–9 and _ as _. Prepare the model again (Model preparation → Convert, then Use prepared model) so the app gives them distinct names, or rename one of them in Blender.", "prepare"),
            // A prepared model after a game update changed the hero.
            ("Skeleton reference belongs to different original model", "prepare"),
            ("Extended skeleton missing original identities", "prepare"),
            ("Cloth validation: Native secondary fitting is incomplete: dmb_secondary_HB_0_0", "physics"),
            ("Native float32 precision collapses or reverses a cloth proxy face", "physics"),
            (r"C:\Downloads: The system cannot find the file specified. (os error 2)", "settings"),
            (r"\\?\C:: Incorrect function. (os error 1)", "settings"),
            // Issue #7 (Russian Windows), then the staging check that replaced it.
            (r"Operation could not be completed: Publish E:\workspace\0123456789abcdef0123456789abcdef\humanoid_preparation\fedcba9876543210fedcba9876543210\source\Programs\#фон #небо #город #море #лес #поле #река #гора #сад #дом #мост #парк #луна #снег #да #wallpaper #sky #city #sea #forest #field #river #hill #garden #house #bridge #park #moon #snow #example #long #file .jpg: Системе не удается найти указанный путь. (os error 3)", "settings"),
            // The staging checks that remain now that long textures get shorter copies (source.rs).
            (r"D:\Models\Hero\textures\#фон #небо #город.jpg is too long for Blender once copied into the workspace, even under a short name: its path there would be 271 characters, and Blender can only open paths shorter than 260. Choose a shorter Workspace folder in Settings, for example C:\DMB.", "settings"),
            (r"The model file D:\Models\Hero\hero.fbx is too long for Blender once copied into the workspace: its path there would be 261 characters, and Blender can only open paths shorter than 260. Choose a shorter Workspace folder in Settings, for example C:\DMB, or give the model file a shorter name.", "settings"),
            (r"D:\Models\Hero\data\hero.bin is too long for Blender once copied into the workspace: its path there would be 289 characters, and Blender can only open paths shorter than 260. Blender reads this glTF data file from exactly that path, so choose a shorter Workspace folder in Settings, for example C:\DMB, or export the model again with shorter folder and file names.", "settings"),
            (r"Operation could not be completed: Publish C:\w\source\Textures\body.png: The filename or extension is too long. (os error 206)", "settings"),
            // FBX staging's two folder limits, in source.rs's words.
            ("FBX texture folder too large. Export this character into a dedicated folder.", "project"),
            (r"The folder of the model, C:\Users\me\Downloads, holds too many files to search for its textures (more than 200000). Put the model and its textures in a folder of their own.", "project"),
            ("Load error: Failed to load document kv3 data: No valid format conversion from 'dmb_modeldoc_probe' to 'modeldoc41'", "setup"),
        ] {
            assert_eq!(page(message).as_deref(), Some(expected), "{message}");
        }
        // The staging message gets its own help, not the compiler path help.
        let blender_path = help("validation", r"The model file D:\m\hero.fbx is too long for Blender once copied into the workspace: its path there would be 261 characters, and Blender can only open paths shorter than 260. Choose a shorter Workspace folder in Settings, for example C:\DMB, or give the model file a shorter name.", "").unwrap();
        assert!(blender_path.cause.contains("too long for Blender") && blender_path.steps[1].contains("Workspace folder"), "{blender_path:?}");
        assert!(blender_path.steps[0].contains("model file") && !blender_path.steps.iter().any(|s| s.contains("material")), "{blender_path:?}");
        let short = help("validation", r"D:\m\a.jpg is too long for Blender once copied into the workspace, even under a short name: its path there would be 271 characters, and Blender can only open paths shorter than 260. Choose a shorter Workspace folder in Settings, for example C:\DMB.", "").unwrap();
        assert!(short.steps[1].contains("even a short name"), "{short:?}");
        // A DepotDownloader run that fails is not a missing program.
        let run = help("tool", "External tool failed", r"Tool C:\Tools\DepotDownloader\DepotDownloader.exe failed (exit code 1).").unwrap();
        assert_eq!(run.action.unwrap().page, "logs");
        assert!(run.cause.contains("stopped with an error"));
        for missing in [r"C:\Tools\DD does not exist. Select the DepotDownloader.exe inside the extracted DepotDownloader-windows-x64 folder, for example C:\Tools\DepotDownloader\DepotDownloader.exe.",
                        r"C:\Downloads\DepotDownloader-windows-x64.zip is the downloaded ZIP. Extract it first (right-click → Extract All), then select the DepotDownloader.exe inside the extracted folder."] {
            assert!(help("validation", missing, "").unwrap().cause.contains("could not find"), "{missing}");
        }
        let blender = help("tool", "External tool failed", r"Tool C:\Program Files\Blender Foundation\Blender 5.0\blender.exe failed (exit code 23).").unwrap();
        assert!(blender.steps.iter().any(|s| s.contains("Validate Blender")));
        assert!(help("cancelled", "Operation cancelled", "").is_none());
        assert!(help("busy", "Another operation is running", "Cancel it or wait for it to finish.").unwrap().cause.contains("still running"));
        // Steam sign-in failures, from real DepotDownloader runs: no saved
        // sign-in for a username and a QR code nobody could scan (issue #2).
        let password = help("tool", "External tool failed", concat!(
            r"Tool C:\DeadlockModdingTools\DepotDownloader\DepotDownloader.exe failed (exit code -532462766).", "\n",
            "stdout:\nEnter account password for \"example_account\": \r\nConnecting to Steam3... Done!\r\nLogging 'example_account' into Steam3...\n",
            "stderr:\nUnhandled exception. System.ArgumentException: LogOn requires a username and password or access token to be set in 'details'.\r\n",
            "   at SteamKit2.SteamUser.LogOn(LogOnDetails details)")).unwrap();
        assert!(password.cause.contains("no saved Steam sign-in"), "{password:?}");
        assert!(password.steps[0].contains("Sign in to Steam"));
        assert_eq!(password.action.unwrap().page, "setup");
        let qr = help("tool", "External tool failed", concat!(
            r"Tool C:\Users\me\Desktop\1\DepotDownloader-windows-x64\DepotDownloader.exe failed (exit code 1).", "\n",
            "stdout:\nConnecting to Steam3... Done!\r\nLogging in with QR code...\r\nError: InitializeSteam failed\r\n",
            "stderr:\nFailed to authenticate with Steam: Exception of type 'SteamKit2.AsyncJobFailedException' was thrown.")).unwrap();
        assert!(qr.cause.contains("QR code was not scanned"), "{qr:?}");
        assert!(qr.steps[0].contains("appears on screen"));
        let window = help("tool", "Steam sign-in did not finish", "The DepotDownloader sign-in window closed before the sign-in was saved (exit code 1). Enter your Steam password and Steam Guard code in that window.").unwrap();
        assert!(window.cause.contains("wrong password"), "{window:?}");
        // Issue #4: Silver's own werewolf.vmdl_c chosen as the tested cloth setup.
        let own = help("validation", concat!(
            "Cloth validation: The tested cloth reference is the hero's own game cloth, not a setup made for your replacement model. ",
            "To keep the hero's own hair and cloth, choose Preserve original physics. Check that failed: Compiled mesh inverse bind differs ",
            "from model skeleton: finger_ring_0_R (linear residual 0.000026627; translation residual 0.034709 mm; limits 0.0001 and 0.025 mm)"), "").unwrap();
        assert!(own.cause.contains("hero's own game model"), "{own:?}");
        assert!(own.steps[0].contains("Preserve original physics"));
        assert_eq!(own.action.unwrap().page, "physics");
        // A build started from a kept analysis list whose own check still finds open items (build.rs).
        for refused in [r"Review mesh mappings, materials, LODs and unskinned meshes before building. Inspection: C:\w\inspection.json",
                        "Model compilation stopped by vertex preflight:\nSkinned DMX mesh \"Body\": 600,000 source vertices exceeds the 524,288 animated vertex-cache limit. Choose Automatically split safely, or export separate skinned pieces and map all pieces to the original render node. Aim below about 450,000 vertices per piece, with room for UV/normal/material seams. Skinning must remain enabled; DiscardSkeleton is not a workaround."] {
            let found = help("validation", refused, "").unwrap();
            assert!(found.steps[0].contains("click Analyze project") && found.steps[0].contains("what is left"), "{found:?}");
            assert_eq!(found.action.unwrap().page, "build", "{refused}");
        }
        // Unknown tool failures still say where to look.
        assert_eq!(help("tool", "External tool failed", "Tool x.exe failed (exit code 9)").unwrap().action.unwrap().page, "logs");
    }
    /// An older Source 2 Viewer CLI given an option it does not know: 21.1's own answer to an
    /// unknown option ("Argument '…' is not recognized.", exit code 1), as the runner reports it.
    #[test]
    fn an_older_source_2_viewer_without_an_option_is_told_to_update() {
        let details = format!(
            "Tool C:\\Tools\\Source2Viewer\\Source2Viewer-CLI.exe failed (exit code 1).\nstdout:\nArgument '{}' is not recognized.\n\nstderr:\n\nFull stdout: C:\\w\\logs\\1.stdout\nFull stderr: C:\\w\\logs\\1.stderr",
            deadlock_pipeline::object_cloth::WITHOUT_CLOTH_FLAG
        );
        let found = help("tool", "External tool failed", &details).unwrap();
        assert_eq!(found.action.as_ref().unwrap().page, "settings");
        assert!(found.steps[0].starts_with("Update Source 2 Viewer CLI"), "{found:?}");
        assert!(found.cause.contains("older"), "{found:?}");
        // Another tool's unknown option is not taken for Source 2 Viewer.
        let other = help("tool", "External tool failed", "Tool C:\\Tools\\x.exe failed (exit code 1).\nstdout:\nArgument '--y' is not recognized.\n").unwrap();
        assert_eq!(other.action.unwrap().page, "logs");
    }
    /// A model file Blender cannot read, as model preparation reports it: the cause and
    /// the file come first, never the Blender setup.
    #[test]
    fn a_model_file_blender_cannot_read_points_to_the_file() {
        let run = |stdout: &str| format!("Tool C:\\Program Files\\Blender Foundation\\Blender 5.3\\blender.exe failed (exit code 1).\nstdout:\n{stdout}\nBlender quit\nstderr:\n\nFull stdout: C:\\w\\logs\\1.stdout\nFull stderr: C:\\w\\logs\\1.stderr");
        let ascii = help("tool", "External tool failed", &run(r"Error: ASCII FBX files are not supported 'C:\w\source\model.fbx'")).unwrap();
        assert!(ascii.cause.contains("ASCII"), "{ascii:?}");
        assert!(ascii.steps[0].contains("binary FBX") && ascii.steps[0].contains("glTF") && ascii.steps[0].contains(".blend"), "{ascii:?}");
        for details in [
            run(r#"Error: File format is not supported in file "C:\w\source\model.blend""#),
            run(r#"Error: Loading "C:\w\source\model.blend" failed: Failed to read blend file 'C:\w\source\model.blend': Missing DNA block"#),
            run(r"Error: Couldn't open file 'C:\w\source\model.fbx' (failed to read complete nested block, expected 9366 bytes, but only got 334)"),
            run("Error: Bad glTF: json error: Expecting value: line 1 column 1 (char 0)"),
            run("Error: Bad GLB: file size doesn't match"),
        ] {
            let found = help("tool", "External tool failed", &details).unwrap();
            assert!(found.cause.starts_with("Blender cannot read the file you chose"), "{details}: {found:?}");
            assert!(found.steps[1].contains("Character & model"), "{details}: {found:?}");
            // The same messages come for a gun file or an animation file: no button to one page.
            assert!(found.action.is_none(), "{details}");
            assert!(!found.steps.iter().any(|s| s.contains("Validate Blender")), "{details}: {found:?}");
        }
        assert!(!ascii.steps.iter().any(|s| s.contains("Validate Blender")));
        assert!(ascii.action.is_none());
        // An ability object's file keeps its own help.
        let object = help("validation", "The object's file could not be prepared", r"Error: ASCII FBX files are not supported 'C:\w\object-1\source\shape.fbx'").unwrap();
        assert_eq!(object.action.unwrap().page, "extras");
    }
    /// A text file named .png as a replacement picture, as Analyze and the build report it.
    #[test]
    fn a_file_that_is_not_a_picture_points_to_extras() {
        let temp = tempfile::tempdir().unwrap();
        let fake = temp.path().join("icon.png");
        std::fs::write(&fake, b"not an image").unwrap();
        let message = deadlock_components::icons::readable_picture(&fake, "panorama/images/heroes/haze_card.vtex_c", &deadlock_core::CancellationToken::new())
            .unwrap_err()
            .to_string();
        assert!(message.contains(&fake.display().to_string()), "{message}");
        let help = help("validation", &message, "").unwrap();
        assert_eq!(help.action.unwrap().page, "extras");
        assert!(help.cause.contains("not a PNG or JPEG image"), "{}", help.cause);
        // The page marks a row "Not a picture" on the same words.
        let checks = include_str!("../../src/extras/pictureChecks.ts");
        assert!(checks.contains(&format!("export const PICTURE_NOT_READABLE = \"{}\";", deadlock_components::icons::PICTURE_NOT_READABLE)));
        // The older message without the file name gets the same help.
        assert_eq!(page("The image format could not be determined").as_deref(), Some("extras"));
    }
    /// The page compares error messages with the texts in `src/extras/effectTypes.ts`.
    #[test]
    fn the_page_knows_the_effect_command_messages() {
        let types = include_str!("../../src/extras/effectTypes.ts");
        for (name, text) in [
            ("EFFECTS_NOT_LOADED", crate::catalogs::EFFECTS_NOT_LOADED),
            ("EFFECTS_GAME_CHANGED", crate::catalogs::EFFECTS_GAME_CHANGED),
            ("EFFECT_UNKNOWN_PREFIX", deadlock_components::effect_trees::TREE_UNKNOWN),
            ("NO_OWN_COLOR_NOTE", deadlock_components::effect_trees::NO_OWN_COLOR_NOTE),
        ] {
            assert!(types.contains(&format!("export const {name} = '{text}';")) || types.contains(&format!("export const {name} = \"{text}\";")), "{name}");
        }
    }
    /// The Objects page compares these texts with `src/extras/objectTypes.ts`.
    #[test]
    fn the_page_knows_the_ability_object_messages() {
        let types = include_str!("../../src/extras/objectTypes.ts");
        for (name, text) in [
            ("OBJECTS_NO_GAME", crate::objects::OBJECTS_NO_GAME),
            ("OBJECTS_NO_BLENDER", crate::objects::OBJECTS_NO_BLENDER),
            ("OBJECT_NOT_OFFERED", deadlock_components::ability_objects::NOT_OFFERED),
            ("OBJECT_NOT_PREPARED", deadlock_preparation::ability_object::NOT_PREPARED),
            ("OBJECT_UNSUPPORTED_FILE", deadlock_preparation::ability_object::UNSUPPORTED_FILE),
            ("OBJECT_NEEDS_CURRENT_SDK", deadlock_components::ability_objects::NEEDS_CURRENT_SDK),
        ] {
            assert!(types.contains(&format!("export const {name} = \"{text}\";")), "{name}");
        }
        for (name, value) in [
            ("MAX_ABILITY_OBJECTS", deadlock_project::ability_objects::MAX_ABILITY_OBJECTS.to_string()),
            ("OBJECT_SCALE_MIN", deadlock_project::ability_objects::FIT_SCALE.0.to_string()),
            ("OBJECT_SCALE_MAX", deadlock_project::ability_objects::FIT_SCALE.1.to_string()),
            ("OBJECT_MAX_QUARTER_TURNS", deadlock_project::ability_objects::MAX_QUARTER_TURNS.to_string()),
        ] {
            assert!(types.contains(&format!("export const {name} = {value};")), "{name}");
        }
    }
    /// The build's own failure when an object's two decompiles (with and without its
    /// cloth) name different bones (`object_cloth::rest_from`).
    fn cloth_bones_differ() -> String {
        let vmdl = |bone: &str| format!("<!-- kv3 encoding:text:version{{e21c7f3c-8a33-41c5-9977-a76d3a32aa0d}} format:modeldoc29:version{{3cec427c-1b0e-4d48-a90a-0436f33a6041}} -->\n{{\n\trootNode = \n\t{{\n\t\t_class = \"RootNode\"\n\t\tchildren = \n\t\t[\n\t\t\t{{\n\t\t\t\t_class = \"Skeleton\"\n\t\t\t\tchildren = \n\t\t\t\t[\n\t\t\t\t\t{{\n\t\t\t\t\t\t_class = \"Bone\"\n\t\t\t\t\t\tname = \"{bone}\"\n\t\t\t\t\t\torigin = [ 0.0, 0.0, 1.0 ]\n\t\t\t\t\t\tangles = [ 0.0, 0.0, 0.0 ]\n\t\t\t\t\t}},\n\t\t\t\t]\n\t\t\t}},\n\t\t]\n\t}}\n}}\n");
        let message = deadlock_pipeline::object_cloth::rest_from(&vmdl("tail_0"), &vmdl("tail_1"), &Default::default()).unwrap_err().to_string();
        assert!(message.contains(deadlock_pipeline::object_cloth::OTHER_BONES), "{message}");
        message
    }
    /// The build's own failures for an object with cloth whose dropped game data cannot be put back,
    /// and for an object with parts kept in every level of detail whose decompile lacks what the
    /// bone marks, the bone order and the attachments prefab need, as the pipeline writes them.
    #[test]
    fn ability_object_game_data_and_level_messages_have_help() {
        use deadlock_pipeline::{object_cloth, object_levels};
        let header = "<!-- kv3 encoding:text:version{e21c7f3c-8a33-41c5-9977-a76d3a32aa0d} format:modeldoc29:version{3cec427c-1b0e-4d48-a90a-0436f33a6041} -->";
        let bone = format!("{header}\n{{\n\trootNode = \n\t{{\n\t\t_class = \"RootNode\"\n\t\tchildren = \n\t\t[\n\t\t\t{{\n\t\t\t\t_class = \"Skeleton\"\n\t\t\t\tchildren = \n\t\t\t\t[\n\t\t\t\t\t{{\n\t\t\t\t\t\t_class = \"Bone\"\n\t\t\t\t\t\tname = \"tail_0\"\n\t\t\t\t\t\torigin = [ 0.0, 0.0, 1.0 ]\n\t\t\t\t\t\tangles = [ 0.0, 0.0, 0.0 ]\n\t\t\t\t\t}},\n\t\t\t\t]\n\t\t\t}},\n\t\t]\n\t}}\n}}\n");
        let keys = "<!-- kv3 -->\n{\n\tcloth_ground_plane = \n\t{\n\t\ttrace_raise = 26.9\n\t}\n}\n";
        let odd_keys = "<!-- kv3 -->\n{\n\tcloth_ground_plane = \n\t{\n\n\t}\n}\n";
        let messages = [
            object_cloth::with_dropped_game_data(&bone, keys).unwrap_err().to_string(),
            object_cloth::with_dropped_game_data(&bone, odd_keys).unwrap_err().to_string(),
            object_levels::marked_bones(bone.clone(), &["tail_0".to_owned()].into()).unwrap_err().to_string(),
            object_levels::attachments_in_prefab(&bone, "models/sample/sample_attachments.vmdl_prefab").unwrap_err().to_string(),
        ];
        for message in &messages {
            let found = help("validation", message, "").unwrap_or_else(|| panic!("no help for {message}"));
            assert!(found.cause.contains("game data") || found.cause.contains("every level of detail"), "{message}: {found:?}");
            assert!(found.steps.iter().any(|s| s.contains("Logs")), "{message}: {found:?}");
            assert_eq!(found.action.unwrap().page, "logs", "{message}");
        }
    }
    /// The bone-constraint check's refusal for cloth from a proxy mesh when a number differs from
    /// the game's by more than `CLOTH_PROXY_CONSTRAINT_ROUNDING` (2.1e-7 here).
    fn cloth_proxy_bound_refusal() -> String {
        use deadlock_components::write_back;
        let entry = |n: &str| format!("<!-- kv3 -->\n{{\n\tBoneConstraintList = \n\t[\n\t\t{{\n\t\t\tm_qOffset = [ 0.70309854, {n} ]\n\t\t}},\n\t]\n}}\n");
        let message = write_back::constraint_rounding_for(&Default::default(), &entry("0.00008665223"), &entry("0.00008686223"), true).unwrap_err();
        assert!(message.contains("allowed for cloth made from a proxy mesh"), "{message}");
        message
    }
    /// Ability object messages, as the commands and the build write them.
    #[test]
    fn ability_object_messages_have_help() {
        use deadlock_components::ability_objects as objects;
        let target = "models/abilities/haze_dagger_model.vmdl";
        for (message, words, page) in [
            (format!("Integrity check failed: {}", objects::NEEDS_CURRENT_SDK), "Reduced CSDK 13", "settings"),
            (format!("Integrity check failed: {}. Settings → Model compiler is set to the downloaded model toolset.", objects::NEEDS_CURRENT_SDK), "Reduced CSDK 13", "settings"),
            (crate::objects::OBJECTS_NO_GAME.to_owned(), "pak01_dir.vpk", "settings"),
            (crate::objects::OBJECTS_NO_BLENDER.to_owned(), "blender.exe", "settings"),
            (format!("Integrity check failed: {}: {target}", objects::NOT_OFFERED), "Reload assets", "extras"),
            (format!("Integrity check failed: {}: C:\\files\\shape.fbx", deadlock_preparation::ability_object::NOT_PREPARED), "Prepare", "extras"),
            (format!("{}: C:\\files\\shape.pmx", deadlock_preparation::ability_object::UNSUPPORTED_FILE), "Prepare", "extras"),
            (format!("Integrity check failed: {}", objects::EMPTY_ROW), "Prepare", "extras"),
            (format!("Integrity check failed: {}: {target}_c", objects::OBJECT_CHANGED), "build again", "build"),
            ("Integrity check failed: Ability objects changed. Rebuild before installing.".to_owned(), "build again", "build"),
            (format!("Integrity check failed: {} (attachments): {target}", objects::COMPILED_DIFFERS), "Logs", "logs"),
            // The build's and the install check's words for the kinds that keep the game's own blocks.
            (format!("Integrity check failed: {} (animation decode key: the bones are listed as [\"base\", \"tip\", \"arm\"], the rebuilt skeleton as [\"base\", \"arm\", \"tip\"]): {target}", objects::KEPT_BLOCKS_REFUSED), "Logs", "logs"),
            (format!("Integrity check failed: {} (Collision is not open): {target}", objects::KEPT_BLOCKS_REFUSED), "Logs", "logs"),
            // Kept collision on a reordered skeleton, and cloth from a proxy mesh whose bones' names are given back.
            (format!("Integrity check failed: {} (physics bone tip is at position Some(2) in the game's skeleton and Some(1) in the rebuilt one): {target}", objects::KEPT_BLOCKS_REFUSED), "Logs", "logs"),
            (format!("Integrity check failed: {} (the cloth's bones: 3 of 4 names given back, status restored): {target}", objects::KEPT_BLOCKS_REFUSED), "Logs", "logs"),
            (format!("Integrity check failed: {} (ASEQ is not the game's block byte for byte): {target}_c", objects::KEPT_BLOCKS_CHANGED), "build again", "build"),
            // The words of the parts kept as the game's own and of the model data written back.
            (format!("Integrity check failed: {} (m_modelSkeleton.m_boneRotParent: bone tip differs in more than its sign: the game's [0.0, 0.0, 0.0, -1.0] / ours [0.0, 0.0, 0.1, 0.99]): {target}", objects::WRITTEN_REFUSED), "Logs", "logs"),
            (format!("Integrity check failed: {} (RestSigns is not open): {target}", objects::WRITTEN_REFUSED), "Logs", "logs"),
            // Cloth from a proxy mesh: a bone-constraint number past its own bound, as the check writes it.
            (format!("Integrity check failed: {} (m_modelInfo.m_keyValueText: {}): {target}", objects::WRITTEN_REFUSED, cloth_proxy_bound_refusal()), "Logs", "logs"),
            (format!("Integrity check failed: {} (m_meshGroups is not the value the build wrote): {target}_c", objects::WRITTEN_CHANGED), "build again", "build"),
            (format!("Integrity check failed: {} (the build kept [\"markers\"], the game's object has []): {target}_c", objects::KEPT_PARTS_CHANGED), "build again", "build"),
            // An object with cloth: the second decompile, without the cloth, as the build writes its failures.
            (format!("Integrity check failed: {}: {target}", deadlock_pipeline::object_cloth::NOT_WRITTEN), "Source 2 Viewer CLI", "settings"),
            (cloth_bones_differ(), "Logs", "logs"),
        ] {
            let found = help("validation", &message, "").unwrap_or_else(|| panic!("no help for {message}"));
            assert!(found.steps.iter().any(|s| s.contains(words)), "{message}: {found:?}");
            assert_eq!(found.action.unwrap().page, page, "{message}");
        }
    }
    /// Preparing an ability object's file: the command's message with the
    /// details as `ability_object::prepare` answered them (staging, then
    /// Blender 5.3 and ability_object.py) for files that have no mesh or cannot
    /// be read.
    #[test]
    fn ability_object_files_that_fail_to_prepare_have_help() {
        let message = "The object's file could not be prepared";
        let script = include_str!("../crates/deadlock-preparation/resources/blender/ability_object.py");
        for (details, words) in [
            ("Validation error: The file has no visible mesh object to use as the ability object.", "visible mesh"),
            ("Validation error: The file has no polygons to use as the ability object.", "visible mesh"),
            ("Validation error: The prepared object has no points.", "visible mesh"),
            (r#"Tool C:\Blender\blender.exe failed (exit code 23).
stdout:
Error: File format is not supported in file "C:\files\shape.blend"
Blender 5.3.0 (hash 02f0899d78e3 built 2026-11-10 01:33:43)"#, "export or download it again"),
            (r#"Tool C:\Blender\blender.exe failed (exit code 23).
stdout:
Error: Loading "C:\files\shape.blend" failed: Failed to read blend file 'C:\files\shape.blend': Missing DNA block"#, "export or download it again"),
            (r"Error: ASCII FBX files are not supported 'C:\\w\\object-1\\source\\shape.fbx'", "binary FBX"),
            (r"Error: Couldn't open file 'C:\\w\\object-3\\source\\shape.fbx' (failed to read complete nested block, expected 9366 bytes, but only got 334)", "export or download it again"),
            ("Unsupported or invalid GLB 2 container.", ".glb"),
            ("expected ident at line 1 column 2", ".glb"),
            ("Missing or escaping glTF dependency: shape.bin", ".bin"),
            // As Blender answers when the file reaches it (ability_object.py run on its own).
            ("Error: Bad glTF: json error: Expecting value: line 1 column 1 (char 0)", ".glb"),
            ("Error: Bad GLB: file size doesn't match", ".glb"),
        ] {
            let found = help("validation", message, details).unwrap_or_else(|| panic!("no help for {details}"));
            assert!(found.steps.iter().any(|s| s.contains(words)), "{details}: {found:?}");
            assert_eq!(found.action.unwrap().page, "extras", "{details}");
        }
        for sentence in ["The file has no visible mesh object to use as the ability object.", "The file has no polygons to use as the ability object."] {
            assert!(script.contains(sentence), "ability_object.py still says {sentence}");
        }
        // Blender itself not set: its own sentence.
        let found = help("validation", crate::objects::OBJECTS_NO_BLENDER, "No Blender is configured.").unwrap();
        assert_eq!(found.action.unwrap().page, "settings");
    }
    /// How a model follows an object with several bones: the refusals as the
    /// build, the fit view and the install check write them.
    #[test]
    fn ability_object_follow_messages_have_help() {
        use deadlock_components::object_follow as follow;
        let canister = "models/heroes_wip/sample/sample_canister.vmdl_c";
        for (message, words) in [
            (format!("Integrity check failed: {}: {canister}", follow::SAME_SKELETON_CLOSED), "In one piece or By parts"),
            (format!("Integrity check failed: {}", follow::SAME_SKELETON_CLOSED), "In one piece or By parts"),
            (format!("Integrity check failed: {}", follow::SAME_SKELETON_NONE), "name your armature's bones"),
            (format!("Integrity check failed: {} (for example ability_bone).", follow::SAME_SKELETON_NAMES), "name your armature's bones"),
            (format!("Integrity check failed: {}: body follows tail.", follow::SAME_SKELETON_OTHER_BONE), "name your armature's bones"),
            (format!("Integrity check failed: {}: body.", follow::SAME_SKELETON_UNWEIGHTED), "name your armature's bones"),
            (format!("Integrity check failed: {} (main, full_top, full_bottom): bar follows metal_mid", follow::NOT_A_CARRIER), "bones the message lists"),
            (format!("Integrity check failed: {} (main): metal_mid", follow::NOT_A_CARRIER), "bones the message lists"),
            (format!("Integrity check failed: {}", follow::NO_CARRIERS), "Remove this row"),
            (format!("Integrity check failed: {}: sweep_body follows mine_base.", deadlock_pipeline::object_states::STATE_BONES), "bones that every state"),
        ] {
            let found = help("validation", &message, "").unwrap_or_else(|| panic!("no help for {message}"));
            assert!(found.steps.iter().any(|s| s.contains(words)), "{message}: {found:?}");
            assert_eq!(found.action.unwrap().page, "extras", "{message}");
        }
        // The fit view shows the same sentences without the prefix.
        for message in [follow::SAME_SKELETON_NONE, follow::SAME_SKELETON_CLOSED, follow::NO_CARRIERS] {
            assert_eq!(help("validation", message, "").unwrap().action.unwrap().page, "extras", "{message}");
        }
    }
    #[test]
    fn the_page_knows_the_ability_object_motion_line() {
        let types = include_str!("../../src/extras/objectTypes.ts");
        assert!(types.contains(&format!("export const OBJECT_MOTION_NOT_READ = \"{}\";", deadlock_pipeline::object_motion::NOT_READ)));
    }
    /// The page compares these sentences: TypeScript holds the same text.
    #[test]
    fn the_page_knows_the_made_clip_messages() {
        use deadlock_preparation::clip_making as making;
        let types = include_str!("../../src/motion/makeTypes.ts");
        for (name, text) in [
            ("NEEDS_PREPARED", making::NEEDS_PREPARED),
            ("NO_FIT", making::NO_FIT),
            ("READY_MADE_MODEL", making::READY_MADE_MODEL),
            ("UNRIGGED_MODEL", making::UNRIGGED_MODEL),
            ("NOT_MADE", making::NOT_MADE),
            ("ONLY_ANIMATION_FILES", making::ONLY_ANIMATION_FILES),
            ("GLTF_TOO_LONG", making::GLTF_TOO_LONG),
            ("CHANGED_WHILE_MAKING", making::CHANGED_WHILE_MAKING),
            ("ADDITIVE", making::ADDITIVE),
            ("MODEL_SPACE", making::MODEL_SPACE),
            ("SINGLE_POSE", making::SINGLE_POSE),
            ("SCALED", making::SCALED),
            ("SKELETON_DIFFERS", making::SKELETON_DIFFERS),
            ("SKELETON_ADVICE", making::SKELETON_ADVICE),
            ("NO_RIG", making::NO_RIG),
            ("NO_ACTION", making::NO_ACTION),
            ("PART_TOO_LONG", making::PART_TOO_LONG),
            ("MADE_NO_BLENDER", crate::made_clips::MADE_NO_BLENDER),
            ("MADE_SUPERSEDED", crate::made_clips::MADE_SUPERSEDED),
            ("MADE_NOT_A_ROW", crate::made_clips::MADE_NOT_A_ROW),
            ("MADE_CHANGED_SINCE_BUILD", deadlock_components::made_clips::CHANGED_SINCE_BUILD),
        ] {
            assert!(types.contains(&format!("export const {name} = \"{text}\";")), "{name}");
        }
        for (name, value) in [
            ("MAX_ACTION_CHARS", deadlock_project::made_clip::MAX_ACTION_CHARS.to_string()),
            ("MAX_PART_START", deadlock_project::made_clip::MAX_PART_START.to_string()),
            ("MAX_LOOP_BLEND", deadlock_project::made_clip::MAX_LOOP_BLEND.to_string()),
        ] {
            assert!(types.contains(&format!("export const {name} = {value};")), "{name}");
        }
        let suffixes: Vec<String> = deadlock_project::made_clip::SOURCE_SUFFIXES.iter().map(|s| format!("\"{s}\"")).collect();
        assert!(types.contains(&format!("export const ANIMATION_FILE_SUFFIXES = [{}] as const;", suffixes.join(", "))), "the page takes the files the saved format takes");
    }
    /// Every message of making a clip from an animation, as Rust and Blender write it.
    #[test]
    fn made_clip_messages_have_help() {
        use deadlock_preparation::clip_making as making;
        let file = "C:\\anim\\moves.blend";
        for (message, words, page) in [
            (crate::made_clips::MADE_NO_BLENDER.to_owned(), "blender.exe", "settings"),
            (making::NEEDS_PREPARED.to_owned(), "Prepare model", "prepare"),
            (making::NO_FIT.to_owned(), "Prepare model", "prepare"),
            (making::READY_MADE_MODEL.to_owned(), "Rigged humanoid", "prepare"),
            (making::UNRIGGED_MODEL.to_owned(), "Rigged humanoid", "prepare"),
            (format!("{} (the bone spine_02 is missing). {}", making::SKELETON_DIFFERS, making::SKELETON_ADVICE), "Animate the file", "motion"),
            (format!("{} (the bone spine_02 is not where it was when you prepared). {}", making::SKELETON_DIFFERS, making::SKELETON_ADVICE), "Animate the file", "motion"),
            (making::NO_RIG.to_owned(), "Animate the file", "motion"),
            (making::ADDITIVE.to_owned(), "compiled clip file", "motion"),
            (making::MODEL_SPACE.to_owned(), "compiled clip file", "motion"),
            (making::SINGLE_POSE.to_owned(), "compiled clip file", "motion"),
            (making::SCALED.to_owned(), "compiled clip file", "motion"),
            ("The action Pose has fewer than two frames to sample.".to_owned(), "two frames", "motion"),
            ("The action Long is too long to sample (at most 20000 frames).".to_owned(), "two frames", "motion"),
            (format!("Integrity check failed: Validation error: {}: Wave ({file})", making::NOT_MADE), "make the clip again", "motion"),
            (format!("{}Wave.", making::NO_ACTION), "make the clip again", "motion"),
            (format!("{} (from 1.500 s the clip needs 1.833 s, the action has 2.833 s)", making::PART_TOO_LONG), "make the clip again", "motion"),
            (making::CHANGED_WHILE_MAKING.to_owned(), "make the clip again", "motion"),
            (format!("{} (271 characters; Blender opens paths shorter than 260), and the files beside it cannot be copied with it. Move it and the files beside it to a shorter folder, then choose it again.", making::GLTF_TOO_LONG), "shorter path", "motion"),
            (crate::made_clips::MADE_NOT_A_ROW.to_owned(), "make the clip again", "motion"),
            ("Choose the animation (action) the clip is made from: a name of 1 to 256 characters.".to_owned(), "make the clip again", "motion"),
            ("The part that plays must start from 0 to 3600 seconds into the animation.".to_owned(), "make the clip again", "motion"),
            ("The loop blend must be longer than 0 and at most 10 seconds.".to_owned(), "make the clip again", "motion"),
            (format!("{}: {file}", making::ONLY_ANIMATION_FILES), ".fbx", "motion"),
            (format!("{}: C:\\anim\\moves.vnmclip_c", making::ONLY_ANIMATION_FILES), ".gltf", "motion"),
            (deadlock_project::made_clip::NEEDS_ANIMATION_FILE.to_owned(), ".blend", "motion"),
            (format!("{} icons rows cannot.", deadlock_project::made_clip::ONLY_ANIMATIONS), ".blend", "motion"),
            (format!("Integrity check failed: {}", deadlock_components::made_clips::CHANGED_SINCE_BUILD), "build again", "build"),
            (deadlock_components::motion::UNSUPPORTED_SOURCE.to_owned(), "Make it from my animation", "motion"),
        ] {
            let found = help("validation", &message, "").unwrap_or_else(|| panic!("no help for {message}"));
            assert!(found.steps.iter().any(|s| s.contains(words)), "{message}: {found:?}");
            assert_eq!(found.action.unwrap().page, page, "{message}");
        }
    }
    /// The page compares these sentences and lists: TypeScript holds the same.
    #[test]
    fn the_page_knows_the_taken_clip_messages() {
        use deadlock_components::hero_clips as taken;
        use deadlock_project::taken_clip as saved;
        let types = include_str!("../../src/motion/takenTypes.ts");
        for (name, text) in [
            ("TAKEN_TARGET_ADDITIVE", taken::TARGET_ADDITIVE),
            ("TAKEN_TARGET_NOT_FULL", taken::TARGET_NOT_FULL),
            ("TAKEN_SOURCE_ADDITIVE", taken::SOURCE_ADDITIVE),
            ("TAKEN_SOURCE_MODEL_SPACE", taken::SOURCE_MODEL_SPACE),
            ("TAKEN_NO_SOURCE_HERO", taken::NO_SOURCE_HERO),
            ("TAKEN_NO_SOURCE_CLIP", taken::NO_SOURCE_CLIP),
            ("TAKEN_SAME_HERO", taken::SAME_HERO),
            ("TAKEN_BODY_NOT_CARRIED", taken::BODY_NOT_CARRIED),
            ("TAKEN_SOURCE_UNREADABLE", taken::SOURCE_UNREADABLE),
            ("TAKEN_PART_TOO_LONG", taken::PART_TOO_LONG),
            ("TAKEN_GAME_CHANGED", taken::GAME_CHANGED),
            ("TAKEN_CHANGED_SINCE_BUILD", taken::CHANGED_SINCE_BUILD),
            ("TAKEN_ONLY_ANIMATIONS", saved::ONLY_ANIMATIONS),
            ("TAKEN_NOT_BOTH", saved::NOT_BOTH),
            ("TAKEN_NO_FILE", saved::NO_FILE),
            ("TAKEN_BAD_SOURCE", saved::BAD_SOURCE),
            ("TAKEN_BAD_START", saved::BAD_START),
            ("TAKEN_SUPERSEDED", crate::taken_clips::TAKEN_SUPERSEDED),
            ("TAKEN_NOT_A_ROW", crate::taken_clips::TAKEN_NOT_A_ROW),
            ("TAKEN_NOT_A_CLIP", crate::taken_clips::TAKEN_NOT_A_CLIP),
        ] {
            assert!(types.contains(&format!("export const {name} = \"{text}\";")), "{name}");
        }
        assert!(types.contains(&format!("export const MAX_TAKEN_START = {};", saved::MAX_START)));
        assert!(types.contains(&format!("export const MAX_TAKEN_PATH_CHARS = {};", saved::MAX_PATH_CHARS)));
        let quoted = |names: &[&str]| names.iter().map(|s| format!("\"{s}\"")).collect::<Vec<_>>().join(", ");
        assert!(types.contains(&format!("export const TAKEN_ARM_BONES = [{}] as const;", quoted(taken::ARMS))));
        for (name, labels) in taken::SETS {
            assert!(types.contains(&format!("  {name}: [{}],", quoted(labels))), "{name}");
        }
    }
    /// Every message of taking a clip from another hero, as the build, the
    /// install check and the saved format write them.
    #[test]
    fn taken_clip_messages_have_help() {
        use deadlock_components::hero_clips as taken;
        use deadlock_project::taken_clip as saved;
        let wrap = |text: &str| format!("Integrity check failed: {text}");
        for (message, words, page) in [
            (wrap(taken::TARGET_ADDITIVE), "compiled clip file", "motion"),
            (wrap(taken::TARGET_NOT_FULL), "compiled clip file", "motion"),
            (wrap(taken::SOURCE_ADDITIVE), "another of that hero's clips", "motion"),
            (wrap(taken::SOURCE_MODEL_SPACE), "another of that hero's clips", "motion"),
            (wrap(&format!("{}: models/heroes_wip/gone/gone.vmdl", taken::NO_SOURCE_HERO)), "choose another hero", "motion"),
            (wrap(&format!("{}: models/heroes_wip/abrams/clips/gone.vnmclip_c", taken::NO_SOURCE_CLIP)), "choose another hero", "motion"),
            (wrap(taken::SAME_HERO), "choose another hero", "motion"),
            (wrap(&format!("{} (Baba has no spine_0, spine_1, neck_0, head).", taken::BODY_NOT_CARRIED)), "hero list", "motion"),
            (wrap(&format!("{} (spine_0 hangs from pelvis on Haze and from collar on Other).", taken::BODY_NOT_CARRIED)), "hero list", "motion"),
            (wrap(&format!("{}: Unsupported compressed animation layout", taken::SOURCE_UNREADABLE)), "Logs", "motion"),
            (wrap(&format!("{} (from 0.900 s this clip needs 1.033 s, the other hero's clip has 1.533 s)", taken::PART_TOO_LONG)), "stretched", "motion"),
            (wrap(taken::GAME_CHANGED), "build again", "build"),
            (wrap(taken::CHANGED_SINCE_BUILD), "build again", "build"),
            (format!("{} icons rows cannot.", saved::ONLY_ANIMATIONS), "choose the other hero", "motion"),
            (saved::NOT_BOTH.to_owned(), "choose the other hero", "motion"),
            (saved::NO_FILE.to_owned(), "choose the other hero", "motion"),
            (saved::BAD_SOURCE.to_owned(), "choose the other hero", "motion"),
            (saved::BAD_START.to_owned(), "choose the other hero", "motion"),
            (crate::taken_clips::TAKEN_NOT_A_ROW.to_owned(), "choose the other hero", "motion"),
            (format!("{}: models/heroes_wip/fencer/clips/gone.vnmclip_c", crate::taken_clips::TAKEN_NOT_A_CLIP), "choose the other hero", "motion"),
        ] {
            let found = help("validation", &message, "").unwrap_or_else(|| panic!("no help for {message}"));
            assert!(found.steps.iter().any(|s| s.to_lowercase().contains(&words.to_lowercase())), "{message}: {found:?}");
            assert_eq!(found.action.unwrap().page, page, "{message}");
        }
    }
    /// Effect edit build messages, as `effect_build.rs` and `service.rs` write them.
    #[test]
    fn effect_edit_build_messages_point_to_extras_or_a_new_build() {
        use deadlock_components::effect_build::{EFFECT_CHANGED, EFFECT_NOT_IN_GAME};
        for (message, step, page) in [
            (format!("Integrity check failed: {EFFECT_NOT_IN_GAME}: particles/abilities/ghost/ghost_blood_bomb_explode.vpcf_c"), "Reload assets", "extras"),
            (format!("Integrity check failed: {EFFECT_CHANGED}: particles/weapon_fx/ghost/ghost_tracer.vpcf_c"), "build again", "build"),
            ("Integrity check failed: Effect edits changed. Rebuild before installing.".to_owned(), "build again", "build"),
        ] {
            let found = help("validation", &message, "").unwrap_or_else(|| panic!("no help for {message}"));
            assert!(found.steps[0].contains(step), "{message}: {found:?}");
            assert_eq!(found.action.unwrap().page, page, "{message}");
        }
    }
    /// The Extras catalog and effect editor messages, as the commands write them.
    #[test]
    fn item_icon_messages_have_help() {
        use deadlock_components::item_icons;
        let target = "panorama/images/items/weapon/basic_magazine_psd.vtex_c";
        for (message, words, page) in [
            (crate::items::ITEMS_NO_GAME.to_owned(), "pak01_dir.vpk", "settings"),
            (crate::items::ITEMS_NOT_LOADED.to_owned(), "Reload assets", "extras"),
            (crate::items::ITEMS_GAME_CHANGED.to_owned(), "Reload assets", "extras"),
            (format!("Integrity check failed: {}", item_icons::ITEM_ICON_CHANGED), "Reload assets", "extras"),
            (format!("Integrity check failed: {}: {target}", item_icons::NOT_AN_ITEM_ICON), "Reload assets", "extras"),
            (format!("Integrity check failed: {}: the game has no ability definitions (scripts/abilities.vdata_c).", item_icons::ITEM_ICONS_UNREADABLE), "pak01_dir.vpk", "settings"),
            (format!("Integrity check failed: {}", item_icons::EMPTY_ROW), "Replaced only", "extras"),
            (format!("This picture is replaced both in Extras → Icons and in Extras → Items: {target}. Remove one of them."), "Replaced only", "extras"),
            (format!("Two enabled item icon replacements use the same picture: {target}"), "Replaced only", "extras"),
        ] {
            let found = help("validation", &message, "").unwrap_or_else(|| panic!("no help for {message}"));
            assert!(found.steps.iter().any(|s| s.contains(words)), "{message}: {found:?}");
            assert_eq!(found.action.unwrap().page, page, "{message}");
        }
        // The project check's own messages, as validation reports them.
        let rows = |value: serde_json::Value| serde_json::from_value::<Vec<deadlock_project::customization::ResourceReplacement>>(value).unwrap();
        for (items, icons) in [
            (rows(serde_json::json!([{"target": target}, {"target": target}])), vec![]),
            (rows(serde_json::json!([{"target": target}])), rows(serde_json::json!([{"target": target}]))),
        ] {
            let message = deadlock_project::item_icons::validate(&items, &icons).unwrap_err().to_string();
            assert!(help("validation", &message, "").is_some(), "{message}");
        }
    }
    /// Renamed items: the real messages of the build, the install check and the project check.
    #[test]
    fn item_name_messages_have_help() {
        use deadlock_components::item_names;
        use deadlock_project::item_names::{ItemNameRow, clean_name, validate_rows};
        let temp = tempfile::tempdir().unwrap();
        let row = |item: &str| ItemNameRow { item: item.into(), enabled: true, name: "New Name".into() };
        let nowhere = temp.path().join("pak01_dir.vpk");
        let missing = item_names::stage(&nowhere, &[row("upgrade_sample")], temp.path()).unwrap_err().to_string();
        let folder = item_names::folder(&nowhere);
        std::fs::create_dir_all(&folder).unwrap();
        let english = "\u{feff}\"lang\"\r\n{\r\n\t\"Tokens\"\r\n\t{\r\n\t\t\"upgrade_sample\"\t\"Sample\"\r\n\t}\r\n}\r\n";
        std::fs::write(folder.join("citadel_gc_mod_names_english.txt"), english).unwrap();
        let unknown = item_names::stage(&nowhere, &[row("upgrade_gone")], &temp.path().join("out")).unwrap_err().to_string();
        std::fs::write(folder.join("citadel_gc_mod_names_english.txt"), format!("{english}\"upgrade_sample\" \"x\"\r\n").replace("\t}\r\n}", "\"upgrade_sample\" \"twice\"\r\n\t}\r\n}")).unwrap();
        let not_carried = item_names::stage(&nowhere, &[row("upgrade_sample")], &temp.path().join("out")).unwrap_err().to_string();
        assert!(not_carried.contains(item_names::NOT_CARRIED), "{not_carried}");
        for (message, step, page) in [
            (missing, "pak01_dir.vpk", "settings"),
            (not_carried, "pak01_dir.vpk", "settings"),
            (unknown, "remove its row", "extras"),
            (format!("Integrity check failed: {}", item_names::GAME_NAMES_CHANGED), "build the mod again", "build"),
            (format!("Integrity check failed: {}", item_names::ITEM_NAMES_CHANGED), "build again", "build"),
            (format!("Validation failed: {}", item_names::ITEM_NAMES_CHANGED), "build again", "build"),
        ] {
            let found = help("validation", &message, "").unwrap_or_else(|| panic!("no help for {message}"));
            assert!(found.steps.iter().any(|s| s.contains(step)), "{message}: {found:?}");
            assert_eq!(found.action.unwrap().page, page, "{message}");
        }
        let mut project_messages: Vec<String> = ["", &"a".repeat(33), "two\nlines", "a%b", "#a"].iter().map(|n| clean_name(n).unwrap_err().to_string()).collect();
        project_messages.push(validate_rows(&[row("upgrade_a"), row("UPGRADE_A")]).unwrap_err().to_string());
        project_messages.push(validate_rows(&[row("upgrade a")]).unwrap_err().to_string());
        project_messages.push(validate_rows(&[ItemNameRow { name: " A".into(), ..row("upgrade_a") }]).unwrap_err().to_string());
        project_messages.push(validate_rows(&(0..=deadlock_project::item_names::MAX_ITEM_NAMES).map(|i| row(&format!("upgrade_{i}"))).collect::<Vec<_>>()).unwrap_err().to_string());
        for message in project_messages {
            let found = help("validation", &message, "").unwrap_or_else(|| panic!("no help for {message}"));
            assert_eq!(found.action.unwrap().page, "extras", "{message}");
        }
    }
    /// The page's row helpers hold the same limits and messages as Rust.
    #[test]
    fn the_page_knows_the_item_name_rules() {
        use deadlock_project::item_names as rules;
        let types = include_str!("../../src/extras/itemNames.ts");
        for (name, text) in [
            ("NAME_EMPTY", rules::NAME_EMPTY),
            ("NAME_TOO_LONG", rules::NAME_TOO_LONG),
            ("NAME_ONE_LINE", rules::NAME_ONE_LINE),
            ("NAME_REFUSED_CHAR", rules::NAME_REFUSED_CHAR),
            ("NAME_LEADING_HASH", rules::NAME_LEADING_HASH),
        ] {
            assert!(types.contains(&format!("export const {name} = {};", serde_json::to_string(text).unwrap())), "{name}");
        }
        for (name, value) in [("MAX_ITEM_NAMES", rules::MAX_ITEM_NAMES), ("MAX_NAME_CHARS", rules::MAX_NAME_CHARS)] {
            assert!(types.contains(&format!("export const {name} = {value};")), "{name}");
        }
        let chars: Vec<String> = rules::REFUSED_CHARS.iter().map(|c| c.to_string()).collect();
        assert!(types.contains(&format!("export const REFUSED_CHARS = {} as const;", serde_json::to_string(&chars).unwrap())));
        assert!(types.contains("Item names support at most ${MAX_ITEM_NAMES} renamed items."));
    }
    #[test]
    fn extras_and_effect_editor_messages_lead_to_reload_assets() {
        for message in [
            "Load this character’s assets first".to_owned(),
            crate::catalogs::EFFECTS_NOT_LOADED.to_owned(),
            crate::catalogs::EFFECTS_GAME_CHANGED.to_owned(),
            format!("{}: particles/abilities/ghost/ghost_tracer.vpcf_c", deadlock_components::effect_trees::TREE_UNKNOWN),
        ] {
            let found = help("validation", &message, "").unwrap_or_else(|| panic!("no help for {message}"));
            assert!(found.steps[0].contains("Reload assets"), "{message}: {found:?}");
            assert_eq!(found.action.unwrap().page, "extras", "{message}");
        }
    }
    /// A new user's empty settings, as Test my setup's first step reports
    /// them, and the model compiler messages: the fix is in Settings
    /// (Reduced CSDK 13), not on the model toolset page.
    #[test]
    fn missing_model_compiler_help_leads_to_csdk_13_in_settings() {
        let step = crate::setup_test::path_step(&deadlock_project::Settings::default());
        let error = step.error.expect("empty settings fail the path step");
        assert!(error.details.contains(deadlock_tools::toolchain::MODEL_TOOLSET_NOT_SET), "{}", error.details);
        assert_eq!(error.help.and_then(|h| h.action).map(|a| a.page).as_deref(), Some("settings"));
        let model = help("validation", deadlock_tools::toolchain::MODEL_TOOLSET_NOT_SET, "").unwrap();
        assert_eq!(model.action.as_ref().map(|a| (a.label.as_str(), a.page.as_str())), Some(("Open Settings", "settings")));
        assert!(model.steps[0].contains("Reduced CSDK 13") && model.steps.last().unwrap().starts_with("Only if you keep a model toolset"), "{model:?}");
        // The rule about an incompatible downloaded toolset keeps its page.
        assert_eq!(page("Load error: Failed to load document kv3 data: No valid format conversion from 'dmb_modeldoc_probe' to 'modeldoc41'").as_deref(), Some("setup"));
    }
    /// Hero colors refusals, as the build reports them.
    #[test]
    fn hero_color_refusals_point_to_extras() {
        for (message, cause) in [
            ("Hero colors change the game's own model. Turn off Include replacement model and textures in Extras → Build options, or recolor your replacement model in Appearance → Surface changes.", "cannot be combined"),
            ("metal is also used by 1 other game model (crate.vmdl). Turn on Recolor everywhere it is used, or choose another material.", "changes that model too"),
            ("The game material hazev2 changed since you chose it (a game update). Load character assets again, check the color, then build.", "game update"),
            ("models/other/materials/x.vmat_c is not one of this hero's materials. Load character assets again and choose a material from the list.", "game update"),
            ("odd uses the environment_blend.vfx shader. Hero colors support pbr.vfx materials only.", "does not have the color setting"),
            ("plain has no saturation or brightness setting. Use Tint instead.", "does not have the color setting"),
            ("x could not be read as a compiled material: Unsupported or malformed KV3: unconsumed KV3 values; refusing lossy conversion", "does not have the color setting"),
        ] {
            // Integrity errors reach the page as validation errors with the full text as details.
            let found = help("validation", message, &format!("Integrity check failed: {message}")).unwrap();
            assert!(found.cause.contains(cause), "{message}: {found:?}");
            assert_eq!(found.action.unwrap().page, "extras", "{message}");
        }
        for (message, cause, page) in [
            ("The color ranges of hazev2 select none of its base color. Widen a range, or pick a color from the preview.", "match none", "extras"),
            ("Color ranges cannot change hazev2: Color ranges need a BC7 base-color texture; this one is DXT5.", "cannot rewrite", "extras"),
            ("hazev2 has no base-color texture, so color ranges cannot change it.", "cannot rewrite", "extras"),
            ("The game texture of hazev2 changed (a game update). Build again.", "game update", "build"),
        ] {
            let found = help("validation", message, &format!("Integrity check failed: {message}")).unwrap();
            assert!(found.cause.contains(cause), "{message}: {found:?}");
            assert_eq!(found.action.unwrap().page, page, "{message}");
        }
        // Color ranges that give a new color or change one part: the messages as the build writes them.
        use deadlock_project::{color_parts, color_ranges};
        for (message, cause, page) in [
            (format!("hazev2: {}", color_ranges::SET_COLOUR_OFF), "switched off", "extras"),
            (format!("hazev2: {}", color_ranges::PART_ONLY_OFF), "switched off", "extras"),
            (format!("hazev2: Color range 1: {}", color_parts::PART_LOST), "not on any part", "extras"),
            (format!("hazev2: {} The hero's decompiled model could not be read.", color_parts::NO_PARTS), "could not read", "extras"),
            (format!("models/hero/materials/body.vmat {}", deadlock_materials::color_adjust::PART_NEEDS_IMAGE), "plain base color", "extras"),
            (deadlock_components::materials::PARTS_MODEL_CHANGED.to_owned(), "game update", "build"),
            (format!("Range {}", color_ranges::GIVEN_COLOUR_PAIR), "half of a new color", "extras"),
        ] {
            let found = help("validation", &message, &format!("Integrity check failed: {message}")).unwrap_or_else(|| panic!("no help for {message}"));
            assert!(found.cause.contains(cause), "{message}: {found:?}");
            assert_eq!(found.action.unwrap().page, page, "{message}");
        }
        // The replacement-model refusal keeps its plain message.
        assert!(help("validation", "Include a replacement model to use Appearance, Equipment or a static facial expression. To recolor the hero's own model instead, use Extras → Colors.", "").is_none());
    }
    /// Source 2 Viewer exiting with code 2 (a truncated hero model, November 14,
    /// 2026): the end of a hero decompile's output no longer says "decompiled".
    #[test]
    fn a_source_2_viewer_run_with_failed_files_gets_decompiler_help() {
        let details = "Tool C:\\Tools\\Source2Viewer\\Source2Viewer-CLI.exe failed (exit code 2).\nstdout:\n\t-- Block: RED2  Size: 8573   bytes [Offset: 5020432]\n\t-- Block: DATA  Size: 35106  bytes [Offset: 5029008]\n--- Wrote 0 files\n2 file(s) failed to process, see \"C:\\Users\\me\\AppData\\Local\\Temp\\exceptions.txt\".\n\nstderr:\nFound \"Deadlock\" from \"D:\\Deadlock\\game\\citadel\\gameinfo.gi\"\nFull stdout: C:\\w\\logs\\1.stdout\nFull stderr: C:\\w\\logs\\1.stderr";
        let help = help("tool", "External tool failed", details).unwrap();
        assert!(help.cause.starts_with("Source 2 Viewer could not decompile") && help.steps[1].contains("exceptions.txt"), "{help:?}");
        assert_eq!(help.action.map(|a| a.page).as_deref(), Some("settings"));
    }
    /// The hero's 3D view (Extras → Colors) and Analyze need the hero's sources:
    /// without Source 2 Viewer CLI they cannot be extracted, which is not a
    /// file that failed to read.
    #[test]
    fn hero_sources_that_cannot_be_extracted_point_to_settings() {
        // The errors as `ensure_base` and `decompile_from_archive` raise them.
        for message in ["Select Source2Viewer CLI in Setup.", "Auto-decompile is disabled and this hero has no cached original sources."] {
            let error = crate::app::AppError::from(deadlock_core::CoreError::Integrity(message.into()));
            let found = error.help.expect("help");
            assert!(found.cause.starts_with("The hero's own game files have not been extracted"), "{found:?}");
            assert_eq!(found.action.unwrap().page, "settings");
        }
    }
    /// MMD models: the messages mmd_source.py and source.rs write, with the
    /// phrases read from mmd_source.py so rewording one breaks this test.
    #[test]
    fn mmd_model_failures_get_add_on_help() {
        const SCRIPT: &str = include_str!("../crates/deadlock-preparation/resources/blender/mmd_source.py");
        let phrase = |name: &str| -> String {
            let line = SCRIPT.lines().find(|l| l.starts_with(&format!("{name} = '"))).unwrap_or_else(|| panic!("{name} in mmd_source.py"));
            line.split('\'').nth(1).unwrap().replace("{version}", "5.3")
        };
        // The add-on is missing for the Blender in Settings (the real message from a 5.3 run).
        let missing = format!("{}, the Blender set in Settings. .pmx and .pmd files are imported with this Blender add-on. In Blender 5.3, open Edit > Preferences > Get Extensions, search for MMD Tools and click Install, then prepare again. The MMD Tools page explains the installation: {}", phrase("MISSING"), phrase("PAGE"));
        assert!(missing.ends_with("https://extensions.blender.org/add-ons/mmd-tools/"), "{missing}");
        let found = help("validation", &missing, "").unwrap();
        assert!(found.cause.contains("MMD Tools add-on") && found.steps[1].contains("Get Extensions") && found.steps[2].contains("Install"), "{found:?}");
        assert!(found.steps[2].contains(&phrase("PAGE")), "{found:?}");
        assert!(found.steps[0].contains("same version"), "{found:?}");
        assert_eq!(found.action.unwrap().page, "settings");
        // Installed, but failing to load.
        let broken = format!("{} (No module named 'opencc'). Update or reinstall MMD Tools in Blender 5.3 (Edit > Preferences > Get Extensions), then prepare again.", phrase("NOT_ENABLED"));
        let found = help("validation", &broken, "").unwrap();
        assert!(found.cause.contains("failed to load"), "{found:?}");
        assert_eq!(found.action.unwrap().page, "logs");
        // A damaged model file.
        let unread = format!("{} model.pmx: the file ends early or is damaged (unpack requires a buffer of 4 bytes). Check that the file opens in MikuMikuDance or PMX Editor, or import it in Blender with MMD Tools and choose the saved .blend file instead.", phrase("NOT_READ"));
        let found = help("validation", &unread, "").unwrap();
        assert!(found.cause.contains("could not read"), "{found:?}");
        assert_eq!(found.action.unwrap().page, "project");
        // The model's own path too long for Blender (source.rs), not the copy rule.
        let long = help("validation", r"The model file D:\m\a.pmx is too long for Blender: its path is 270 characters, and Blender can only open paths shorter than 260. Move the model's folder to a shorter path, or choose a shorter Workspace folder in Settings, for example C:\DMB.", "").unwrap();
        assert!(long.cause.contains("Blender copy") && long.steps[0].contains("move its folder"), "{long:?}");
    }
    /// Preparing a model without a skeleton whose shape gives no joints: Blender
    /// stops with estimate_landmarks' message (phrases read from algorithms.py, so
    /// rewording one breaks this test). The help names a missing skeleton.
    #[test]
    fn joints_not_found_from_the_shape_point_to_a_missing_skeleton() {
        const SCRIPT: &str = include_str!("../crates/deadlock-preparation/resources/blender/algorithms.py");
        let body = SCRIPT.split("def estimate_landmarks").nth(1).expect("estimate_landmarks in algorithms.py");
        let body = body.split("\ndef ").next().unwrap();
        let messages: Vec<&str> = body.split("ValueError('").skip(1).map(|rest| rest.split('\'').next().unwrap()).collect();
        assert_eq!(messages.len(), 7, "{messages:?}");
        for message in messages {
            // As a Blender failure reaches the page: the tool error with the traceback's last line.
            let details = format!("Tool C:\\Program Files\\Blender Foundation\\Blender 5.3\\blender.exe failed (exit code 23).\nstdout:\nBlender quit\nstderr:\nTraceback (most recent call last):\n  File \"worker.py\", line 599, in prepare\nValueError: {message}\n\nFull stdout: C:\\w\\logs\\1.stdout\nFull stderr: C:\\w\\logs\\1.stderr");
            let found = help("tool", "External tool failed", &details).unwrap_or_else(|| panic!("no help for {message}"));
            assert!(found.cause.contains("without a skeleton (armature)"), "{message}: {found:?}");
            assert!(found.steps[0].contains("no skeleton") && found.steps[0].contains("FBX"), "{message}: {found:?}");
            assert_eq!(found.action.unwrap().page, "prepare", "{message}");
        }
        // The other Unsupported messages of preparation keep their own help.
        let rigs = help("tool", "External tool failed", "Tool blender.exe failed (exit code 23).\nValueError: Unsupported: multiple armatures affect the same character mesh. Body").unwrap();
        assert!(rigs.cause.starts_with("Blender stopped"), "{rigs:?}");
    }
    /// A backslash lost in editing once left a backspace byte in an example path
    /// ("Blender 5.2" + 0x08 + "lender.exe"); no help text may hold a control character.
    #[test]
    fn help_texts_hold_no_control_characters() {
        let texts = RULES.iter().flat_map(|r| {
            [r.cause].into_iter().chain(r.steps.iter().copied()).chain(r.action.into_iter().flat_map(|(label, page)| [label, page]))
        });
        for text in texts.chain(MODEL_TOOLSET_STEPS.iter().copied()).chain(GENERIC_TOOL.iter().copied()).chain(GENERIC.iter().copied()) {
            assert!(!text.chars().any(char::is_control), "control character in help text: {text:?}");
        }
        let blender = help("validation", "Blender is not configured", "").unwrap();
        assert!(blender.steps.iter().any(|s| s.contains(r"C:\Program Files\Blender Foundation\Blender 5.2\blender.exe")), "{blender:?}");
    }
}
