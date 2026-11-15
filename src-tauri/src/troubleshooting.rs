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
    Rule { any: &["not included in this build"], all: &["tangtang"],
        cause: "This copy of the app was built without the TangTang presets.",
        steps: &["Open Physics & preview → Automatic motion and choose Automatic — match movement to each part.", "Click Analyze project again. Automatic authors the movement from your model; the TangTang presets only ever applied to the accepted Haze conversion."],
        action: Some(("Open Physics & preview", "physics")) },
    Rule { any: &["another operation is running"], all: &[],
        cause: "Another task was still running when you clicked.",
        steps: &["Wait until the status bar at the bottom says Ready, then click again.", "If a step is shown as running and you do not need it, click Cancel in the status bar.", "If it keeps happening with nothing running, restart the app."],
        action: None },
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
    // Hero color ranges recolor the base-color texture.
    Rule { any: &["select none of its base color"], all: &[],
        cause: "The material's color ranges match none of its base-color pixels, so they would change nothing.",
        steps: &["Open Extras → Colors and select the material named in the message.", "Turn on Show selection and click Pick color on the preview to center a range on the part you want, or widen the hue, saturation or brightness range.", "Build again."],
        action: Some(("Open Extras", "extras")) },
    Rule { any: &["color ranges cannot change", "color ranges need a bc7"], all: &[],
        cause: "This material's base-color texture is stored in a form that color ranges cannot rewrite.",
        steps: &["Open Extras → Colors and select the material named in the message.", "Remove its color ranges and use Tint, Saturation and Brightness instead.", "Build again."],
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
    // FBX staging searches the model's folder for its textures (issue #7).
    Rule { any: &["fbx texture folder too large", "too many files to search for its textures"], all: &[],
        cause: "Your model's folder holds too many files for the app to search it for the model's textures, for example when the model sits in Downloads.",
        steps: &["Make a new folder for the model alone and move (or export) the model file and its textures into it, keeping any texture subfolders.", "In Character & model, choose the model file in its new folder.", "Prepare the model again."],
        action: Some(("Open Character & model", "project")) },
    // Issue #7: Blender cannot open a staged texture from 260 characters on.
    Rule { any: &["too long for blender"], all: &[],
        cause: "A file of your model gets a path too long for Blender once the app copies it into the workspace.",
        steps: &["Give the file named in the message a shorter name. If it is a texture, choose the renamed file again in your model's material in Blender and save the model.", r"Or open Settings and choose a shorter Workspace folder, for example C:\DMB.", "Prepare the model again."],
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
        cause: "Blender is needed to prepare Blender, FBX and glTF models, and the app cannot find it.",
        steps: &["Install Blender 5.2 LTS (Settings → Get programs has the link).", r"In Settings → Advanced configuration → Blender executable, select blender.exe, for example C:\Program Files\Blender Foundation\Blender 5.2\blender.exe.", "Click Save settings and prepare again."],
        action: Some(("Open Settings", "settings")) },
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
    // Two kept bones the model compiler writes with one name (arcaneVin's skirt
    // panels 侧裙/前裙/后裙): found at Analyze, or after compiling a model
    // prepared before the app gave such bones distinct names.
    Rule { any: &["would get the same name in the compiled model", "missing or ambiguous generated bone"], all: &[],
        cause: "Two of your model's moving-part bones get the same name when the model compiler writes them: it turns every character other than A–Z, 0–9 and _ into _, so names such as 侧裙_0_0 and 前裙_0_0 become one. The model was prepared before the app gave such bones distinct names.",
        steps: &["Open Model preparation and click Convert, then Use prepared model. The app now gives these bones distinct names; your moving-part choices stay.", "Run Analyze again, then build."],
        action: Some(("Open Model preparation", "prepare")) },
    // A game update changed the hero's model after preparation (October 20, 2026:
    // Silver, Violet and Haze among others).
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
    // Source 2 Viewer's summary when it exits with code 2; it runs in the temp
    // folder, where it leaves exceptions.txt (compiler.rs source2viewer).
    Rule { any: &["file(s) failed to process"], all: &[],
        cause: "Source 2 Viewer could not decompile one or more of the hero's game files, usually after a Deadlock update or with an older Source 2 Viewer CLI.",
        steps: &["Download the latest Source 2 Viewer CLI (Settings → Get programs) and replace the old Source2Viewer-CLI.exe.", "If it still fails, open the exceptions.txt named in the message (in your temp folder) or Logs: they name the game file that could not be read. Check for a newer version of this app.", "Analyze or build again."],
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
            // Analyze on a model with a crossing face (arcaneVin's gun before folded faces were accepted).
            ("Integrity check failed: Preview cannot triangulate a self-intersecting polygon", "prepare"),
            ("Integrity check failed: Preview cannot triangulate this DMX polygon; check for self-intersections", "prepare"),
            // arcaneVin's skirt panels, after compiling and (with the new check) at Analyze.
            ("Cloth validation: Missing or ambiguous generated bone: dmb_secondary_侧裙_0_0", "prepare"),
            ("Bones dmb_secondary_侧裙_0_0 and dmb_secondary_前裙_0_0 would get the same name in the compiled model (dmb_secondary________0_0): the model compiler writes every character other than A–Z, 0–9 and _ as _. Prepare the model again (Model preparation → Convert, then Use prepared model) so the app gives them distinct names, or rename one of them in Blender.", "prepare"),
            // janeVen after the October 20, 2026 game update.
            ("Skeleton reference belongs to different original model", "prepare"),
            ("Extended skeleton missing original identities", "prepare"),
            ("Cloth validation: Native secondary fitting is incomplete: dmb_secondary_HB_0_0", "physics"),
            ("Native float32 precision collapses or reverses a cloth proxy face", "physics"),
            (r"C:\Downloads: The system cannot find the file specified. (os error 2)", "settings"),
            (r"\\?\C:: Incorrect function. (os error 1)", "settings"),
            // Issue #7 (Russian Windows), then the staging check that replaced it.
            (r"Operation could not be completed: Publish E:\workspace\0123456789abcdef0123456789abcdef\humanoid_preparation\fedcba9876543210fedcba9876543210\source\Programs\#фон #небо #город #море #лес #поле #река #гора #сад #дом #мост #парк #луна #снег #да #wallpaper #sky #city #sea #forest #field #river #hill #garden #house #bridge #park #moon #snow #example #long #file .jpg: Системе не удается найти указанный путь. (os error 3)", "settings"),
            (r"D:\Models\Hero\Programs\#фон #небо #город.jpg is too long for Blender once copied into the workspace: its path there would be 322 characters, and Blender can only open paths shorter than 260. Give the file a shorter name (a renamed texture must then be chosen again in your model's material), or choose a shorter Workspace folder in Settings, for example C:\DMB.", "settings"),
            (r"Operation could not be completed: Publish C:\w\source\Textures\body.png: The filename or extension is too long. (os error 206)", "settings"),
            // FBX staging's two folder limits, in source.rs's words.
            ("FBX texture folder too large. Export this character into a dedicated folder.", "project"),
            (r"The folder of the model, C:\Users\me\Downloads, holds too many files to search for its textures (more than 200000). Put the model and its textures in a folder of their own.", "project"),
            ("Load error: Failed to load document kv3 data: No valid format conversion from 'dmb_modeldoc_probe' to 'modeldoc41'", "setup"),
        ] {
            assert_eq!(page(message).as_deref(), Some(expected), "{message}");
        }
        // The staging message gets its own help, not the compiler path help.
        let blender_path = help("validation", r"D:\m\Programs\a.jpg is too long for Blender once copied into the workspace: its path there would be 322 characters, and Blender can only open paths shorter than 260.", "").unwrap();
        assert!(blender_path.cause.contains("too long for Blender") && blender_path.steps[1].contains("Workspace folder"), "{blender_path:?}");
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
        // sign-in for a username (October 18, 2026) and a QR code nobody could
        // scan (issue #2).
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
        // Unknown tool failures still say where to look.
        assert_eq!(help("tool", "External tool failed", "Tool x.exe failed (exit code 9)").unwrap().action.unwrap().page, "logs");
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
