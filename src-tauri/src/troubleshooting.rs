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
    "Open Settings → Set up model compiler….",
    "If you already downloaded the toolset, click Verify existing acquisition; otherwise click Fetch manifests and plan, approve the Steam sign-in, then Download.",
    "Click Use this toolset in settings. The app saves it for you.",
    r"Check Settings → Advanced configuration: Model compiler toolset ends in \toolset and the backend is auto. The Community SDK must not be used as the model compiler.",
    "Build again.",
];

const RULES: &[Rule] = &[
    Rule { any: &["another operation is running"], all: &[],
        cause: "Another task was still running when you clicked.",
        steps: &["Wait until the status bar at the bottom says Ready, then click again.", "If a step is shown as running and you do not need it, click Cancel in the status bar.", "If it keeps happening with nothing running, restart the app."],
        action: None },
    Rule { any: &["particlefloattype_t", "schema mismatch"], all: &[],
        cause: "The character model was compiled with the Community SDK's compiler. That compiler only builds materials; hero models need the model toolset.",
        steps: MODEL_TOOLSET_STEPS, action: Some(("Set up model compiler", "setup")) },
    Rule { any: &["model compiler is not set up", "model compiler is missing", "model game configuration is missing", "model compiler executable must be"], all: &[],
        cause: "The model compiler (the toolset from Set up model compiler…) is not selected or is incomplete.",
        steps: MODEL_TOOLSET_STEPS, action: Some(("Set up model compiler", "setup")) },
    Rule { any: &["no valid format conversion"], all: &["modeldoc"],
        cause: "The selected model compiler is not the version the app needs (it cannot read ModelDoc 41).",
        steps: &["Open Settings → Set up model compiler….", "Download the pinned toolset, or click Verify existing acquisition for the one you have.", "Click Test original hero compatibility, then Use this toolset in settings.", "Build again."],
        action: Some(("Set up model compiler", "setup")) },
    Rule { any: &["logon", "login", "password", "steam guard", "rate limit", "ratelimit", "access token"], all: &["steam"],
        cause: "Steam sign-in did not finish inside DepotDownloader.",
        steps: &["Use Steam mobile QR sign-in and approve it in the Steam app on your phone.", "Open Logs while it runs to see DepotDownloader's prompts.", "Your account needs Counter-Strike 2 in its library (it is free); installing it is not needed.", "If Steam reports too many attempts, wait 30 minutes and try again."],
        action: Some(("Open Logs", "logs")) },
    Rule { any: &["choose depotdownloader.exe", "depotdownloader.exe was not found", "is the downloaded zip. extract it first (right-click → extract all), then select the depotdownloader.exe", "select the depotdownloader.exe inside the extracted"], all: &[],
        cause: "The app could not find DepotDownloader.exe at the path you gave.",
        steps: &["Download DepotDownloader-windows-x64.zip with the link on the Set up model compiler… page.", "Right-click the ZIP → Extract All.", r"In DepotDownloader executable, select the DepotDownloader.exe inside the extracted folder, for example C:\Tools\DepotDownloader\DepotDownloader.exe.", "Click Fetch manifests and plan again."],
        action: Some(("Open model tool setup", "setup")) },
    Rule { any: &["depotdownloader"], all: &["failed"],
        cause: "DepotDownloader stopped with an error while talking to Steam or downloading.",
        steps: &["Open Logs and read DepotDownloader's last lines; they say whether sign-in, the licence or the network failed.", "Use Steam mobile QR sign-in and approve it on your phone; the account needs Counter-Strike 2 in its library (free).", "Check your internet connection and free disk space, then click Fetch manifests and plan again. A download continues where it stopped."],
        action: Some(("Open Logs", "logs")) },
    Rule { any: &["nothing has been downloaded into"], all: &[],
        cause: "Verify only checks a finished download, and this folder has none yet.",
        steps: &["Click Fetch manifests and plan and approve the Steam sign-in.", "Review the plan, then click Download.", "Verify runs by itself after the download."],
        action: Some(("Open model tool setup", "setup")) },
    Rule { any: &["community sdk is incomplete", "folder around the sdk", "sdk game configuration is missing", "choose the community sdk folder"], all: &[],
        cause: "Settings → Community SDK does not point at a complete Reduced CSDK 12 folder.",
        steps: &[r"Open the Reduced_CSDK_12 folder you extracted. The right folder directly contains the folders game and content.", r"If there is a Reduced_CSDK_12 folder inside another Reduced_CSDK_12 folder, choose the inner one, or move it to C:\CSDK12.", "In Settings, click Detect tools, then Check paths; fix any field marked red.", "If game\\citadel\\gameinfo.gi is missing, extract the whole ZIP again into a new, empty folder."],
        action: Some(("Open Settings", "settings")) },
    Rule { any: &["resource compiler points outside", "select the material compiler", "select resourcecompiler.exe", "the resource compiler must belong"], all: &[],
        cause: "Settings → Resource compiler is not the Community SDK's own material compiler.",
        steps: &[r"Set Resource compiler to <Community SDK>\game\bin_cs2\win64\resourcecompiler.exe, for example C:\CSDK12\game\bin_cs2\win64\resourcecompiler.exe.", "The model toolset's compiler belongs only under Advanced configuration → Model compiler executable.", "Click Check paths, then build again."],
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
        steps: &["Close Deadlock, Blender and File Explorer windows showing the project, then try again.", r"Avoid protected folders such as C:\Program Files for the workspace, the SDK and the toolset; use for example C:\CSDK12.", "If an antivirus quarantined resourcecompiler.exe or DepotDownloader.exe, restore it and allow it."],
        action: None },
    Rule { any: &["os error 206", "filename or extension is too long", "path is too long", "too long for the compiler"], all: &[],
        cause: "A file path is longer than Windows allows.",
        steps: &[r"Move the Reduced CSDK 12 folder to a short path such as C:\CSDK12 and update Settings → Community SDK and Resource compiler.", "In Model preparation, click Prepare again so material names are shortened.", "Build again."],
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
        steps: &["Install Blender 5.2 LTS (Settings → Get programs has the link).", r"In Settings → Advanced configuration → Blender executable, select blender.exe, for example C:\Program Files\Blender Foundation\Blender 5.2lender.exe.", "Click Save settings and prepare again."],
        action: Some(("Open Settings", "settings")) },
    Rule { any: &["export_scene_ot_gltf"], all: &[],
        cause: "Blender failed while exporting the model for the preview.",
        steps: &["Open Logs and look for the Python error under EXPORT_SCENE_OT_gltf.", "Use Blender 5.2 LTS (Settings → Blender executable, then Tool diagnostics → Validate Blender).", "In Blender, apply or remove broken modifiers and save the file, then try again."],
        action: Some(("Open Logs", "logs")) },
    Rule { any: &["blender"], all: &["exit code"],
        cause: "Blender stopped while preparing your model.",
        steps: &["Open Logs and look for the lines starting with Traceback or Error just before the failure.", "Check that the file opens in Blender on its own.", "Use a supported Blender: 4.2–4.5, 5.0 or 5.2 LTS (5.2 is recommended). Set it in Settings → Blender executable and click Validate Blender.", "Save the file as a new .blend without add-on data and prepare again."],
        action: Some(("Open Logs", "logs")) },
    Rule { any: &["confirm replacement of the existing output"], all: &[],
        cause: "This VPK slot already holds a file from this project.",
        steps: &["On Build & install, confirm that the old file may be replaced, or choose another free slot.", "Close Deadlock first if it is running."],
        action: Some(("Open Build & install", "build")) },
    Rule { any: &["prepared secondary group is missing", "no preserved secondary rig", "has no moving-part bones"], all: &[],
        cause: "Moving parts are switched on, but the prepared model was made without their bones.",
        steps: &["Open Model preparation and turn on Keep existing physics bones (Moving parts).", "Click Convert, then Use prepared model.", "Run Analyze again, then build."],
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
    Rule { any: &["cloth proxy face", "fabric fold constraint", "native secondary fitting is incomplete", "cloth validation"], all: &[],
        cause: "The motion setup for one or more moving parts could not be built.",
        steps: &["Open Physics & preview and find the parts named in the message.", "Set their part type to Rigid (or switch them off), then Analyze again.", "Build again. Other moving parts keep their motion."],
        action: Some(("Open Physics & preview", "physics")) },
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
            ("Compiler startup schema mismatch (ParticleFloatType_t). This happens before VMDL loading", "setup"),
            (r"Model compiler is missing: \\?\C:\Deadlockmoddingtools\Reduced_CSDK_12\game\bin_cs2\win64\resourcecompiler.exe.", "setup"),
            (r"The Community SDK is incomplete: \\?\C:\Deadlockmoddingtools\Reduced_CSDK_12\game\citadel\gameinfo.gi is missing.", "settings"),
            ("Choose DepotDownloader.exe. Download DepotDownloader-windows-x64.zip", "setup"),
            (r"Nothing has been downloaded into C:\Tools\DMBTools yet.", "setup"),
            (r"Copy staged archive: There is not enough space on the disk. (os error 112)", "settings"),
            (r"Confirm replacement of the existing output before building: C:\w\output\pak18_dir.vpk", "build"),
            ("Prepared secondary group is missing: Bone_Breast_L_00. Prepare with Keep existing physics bones.", "prepare"),
            ("Model fitting changed. Convert the original source and choose Use prepared model", "prepare"),
            ("Extended skeleton missing original identities", "prepare"),
            ("Cloth validation: Native secondary fitting is incomplete: dmb_secondary_HB_0_0", "physics"),
            ("Native float32 precision collapses or reverses a cloth proxy face", "physics"),
            (r"C:\Downloads: The system cannot find the file specified. (os error 2)", "settings"),
            (r"\\?\C:: Incorrect function. (os error 1)", "settings"),
            ("Load error: Failed to load document kv3 data: No valid format conversion from 'dmb_modeldoc_probe' to 'modeldoc41'", "setup"),
        ] {
            assert_eq!(page(message).as_deref(), Some(expected), "{message}");
        }
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
        // Unknown tool failures still say where to look.
        assert_eq!(help("tool", "External tool failed", "Tool x.exe failed (exit code 9)").unwrap().action.unwrap().page, "logs");
    }
}
