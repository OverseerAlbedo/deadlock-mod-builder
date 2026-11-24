//! Shell-free Blender process boundary with embedded, byte-identical bridge scripts.
//! Includes the qualified rest-skeleton recovery planner.
use crate::source::{BlendStager, PackedImage};
use crate::{Result, invalid, path_text, write_json};
use deadlock_core::process::{CommandSpec, ToolExecutor};
use deadlock_project::asset_paths;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};
pub use deadlock_core::blender_version::SUPPORTED_DESCRIPTION;
pub fn supported_version(version: [u32; 3]) -> bool {
    deadlock_core::blender_version::supported(version)
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlenderValidation {
    pub path: String,
    pub version: String,
    pub launch_verified: bool,
    pub conversion_certified: bool,
}
pub fn version_report(executable: &Path, text: &str) -> Result<BlenderValidation> {
    let regex = regex::Regex::new(r"Blender (\d+)\.(\d+)\.(\d+)").unwrap();
    let found = regex
        .captures(text)
        .ok_or_else(|| invalid("The selected executable did not report a Blender version."))?;
    let mut version = [0; 3];
    for i in 0..3 {
        version[i] = found[i + 1]
            .parse()
            .map_err(|_| invalid("Invalid Blender version."))?;
    }
    if !supported_version(version) {
        return Err(invalid(format!(
            "Blender {}.{}.{} was found, but this bridge supports {SUPPORTED_DESCRIPTION}. Newer minor versions require bridge validation.",
            version[0], version[1], version[2]
        )));
    }
    Ok(BlenderValidation {
        path: path_text(&asset_paths::resolve(executable)?)?,
        version: format!("{}.{}.{}", version[0], version[1], version[2]),
        launch_verified: true,
        conversion_certified: false,
    })
}
pub fn check_executable(executable: &Path) -> Result<()> {
    if executable.as_os_str().is_empty()
        || executable
            .to_str()
            .is_some_and(|value| value.trim().is_empty())
    {
        return Err(crate::Error::BlenderNotConfigured);
    }
    if !executable.is_file() {
        return Err(crate::Error::BlenderNotFound(
            executable.display().to_string(),
        ));
    }
    Ok(())
}
pub fn validate(executable: &Path, executor: &dyn ToolExecutor) -> Result<BlenderValidation> {
    check_executable(executable)?;
    let output = executor.run(
        &CommandSpec::new(executable)
            .arg("--version")
            .timeout(std::time::Duration::from_secs(20)),
    )?;
    version_report(executable, &output.read_stdout_utf8(16 * 1024 * 1024)?)
}
pub fn detect() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&path) {
            let file = directory.join(if cfg!(windows) {
                "blender.exe"
            } else {
                "blender"
            });
            if file.is_file() {
                return Some(file);
            }
        }
    }
    let root = PathBuf::from(
        std::env::var_os("PROGRAMFILES").unwrap_or_else(|| "C:/Program Files".into()),
    )
    .join("Blender Foundation");
    let candidates: Vec<_> = fs::read_dir(root)
        .ok()?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|s| s.starts_with("Blender "))
        })
        .map(|entry| entry.path().join("blender.exe"))
        .filter(|path| path.is_file())
        .collect();
    // By version number, highest supported first (not by folder name order).
    deadlock_core::blender_version::choose_install(candidates)
}
const SCRIPTS: [(&str, &[u8]); 30] = [
    ("mmd_source.py", include_bytes!("../resources/blender/mmd_source.py")),
    // The fit a clip made from an animation is retargeted with (worker.py writes it at Convert).
    ("animation_fit.py", include_bytes!("../resources/blender/animation_fit.py")),
    // Clips made from an animation (its own --python entry, `execute_script`).
    ("animation_export.py", include_bytes!("../resources/blender/animation_export.py")),
    // Ability objects (its own --python entry, `execute_script`).
    ("ability_object.py", include_bytes!("../resources/blender/ability_object.py")),
    ("staged_images.py", include_bytes!("../resources/blender/staged_images.py")),
    ("source_springs.py", include_bytes!("../resources/blender/source_springs.py")),
    ("readable_names.py", include_bytes!("../resources/blender/readable_names.py")),
    // Mandarin readings for readable material names (Unicode Unihan data).
    ("cjk_readings.txt", include_bytes!("../resources/blender/cjk_readings.txt")),
    ("mesh_export.py", include_bytes!("../resources/blender/mesh_export.py")),
    ("breast_bones.py", include_bytes!("../resources/blender/breast_bones.py")),
    ("scene_isolation.py", include_bytes!("../resources/blender/scene_isolation.py")),
    ("weapon_fit.py", include_bytes!("../resources/blender/weapon_fit.py")),
    ("weapon_source.py", include_bytes!("../resources/blender/weapon_source.py")),
    ("source_compatibility.py", include_bytes!("../resources/blender/source_compatibility.py")),
    ("source_rigs.py", include_bytes!("../resources/blender/source_rigs.py")),
    ("arm_skinning.py", include_bytes!("../resources/blender/arm_skinning.py")),
    ("arm_helpers.py", include_bytes!("../resources/blender/arm_helpers.py")),
    ("secondary_bones.py", include_bytes!("../resources/blender/secondary_bones.py")),
    ("leg_chains.py", include_bytes!("../resources/blender/leg_chains.py")),
    (
        "material_textures.py",
        include_bytes!("../resources/blender/material_textures.py"),
    ),
    (
        "source_helpers.py",
        include_bytes!("../resources/blender/source_helpers.py"),
    ),
    (
        "__init__.py",
        include_bytes!("../resources/blender/__init__.py"),
    ),
    (
        "algorithms.py",
        include_bytes!("../resources/blender/algorithms.py"),
    ),
    (
        "blend_source.py",
        include_bytes!("../resources/blender/blend_source.py"),
    ),
    (
        "clothing.py",
        include_bytes!("../resources/blender/clothing.py"),
    ),
    (
        "exact_bind.py",
        include_bytes!("../resources/blender/exact_bind.py"),
    ),
    (
        "retarget.py",
        include_bytes!("../resources/blender/retarget.py"),
    ),
    (
        "rigid_props.py",
        include_bytes!("../resources/blender/rigid_props.py"),
    ),
    (
        "static_face.py",
        include_bytes!("../resources/blender/static_face.py"),
    ),
    (
        "worker.py",
        include_bytes!("../resources/blender/worker.py"),
    ),
];
pub fn install_scripts(directory: &Path) -> Result<PathBuf> {
    fs::create_dir_all(directory)?;
    let mut manifest = BTreeMap::new();
    for (name, data) in SCRIPTS {
        let path = directory.join(name);
        if !path.is_file() || fs::read(&path)? != data {
            deadlock_core::safety::atomic_write(&path, data)?;
        }
        manifest.insert(name, format!("{:x}", Sha256::digest(data)));
    }
    write_json(&directory.join("bridge-manifest.json"), &manifest)?;
    Ok(directory.join("worker.py"))
}
#[derive(Clone)]
pub struct BlenderBridge {
    pub executable: PathBuf,
    pub executor: Arc<dyn ToolExecutor>,
}
#[derive(Serialize)]
struct Job<'a, T> {
    #[serde(flatten)]
    request: &'a T,
    result: String,
    workspace: String,
}
#[derive(Deserialize)]
struct Status {
    status: String,
    #[serde(default)]
    error: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum Request {
    RecoverMaterials {
        source: String,
        hero_root: String,
    },
    Inspect {
        source: String,
        #[serde(default)]
        rotation: [f64; 3],
        #[serde(skip_serializing_if = "Option::is_none")]
        garment_source: Option<String>,
        /// Explicit scene-object choices (include/exclude/character rig).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scene: Option<serde_json::Value>,
        /// Add breast bones where the chest has breasts (breast motion).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        breast_bones: Option<serde_json::Value>,
        /// Add the gun file's meshes (the right hand holds them).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        weapon_source: Option<crate::weapon::GunFile>,
        /// Weapon-model projects: the source may be a gun-only file.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mode: Option<deadlock_project::ModelInputMode>,
    },
    StageBlend {
        source: String,
        destination: String,
    },
    /// Import an MMD model with MMD Tools and save it as a packed Blender file.
    StageMmd {
        source: String,
        destination: String,
    },
    Prepare {
        source: String,
        mode: deadlock_project::ModelInputMode,
        target: Vec<deadlock_skeleton::transforms::PreparedBone>,
        hero_root: String,
        options: deadlock_project::PreparationOptions,
        rotation: [f64; 3],
        correction: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        garment_source: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        face: Option<deadlock_project::customization::Face>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        props: Vec<deadlock_project::customization::Prop>,
        /// Fitted gun meshes: source objects, source->target transform and skinning.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        weapon: Option<crate::weapon::Plan>,
        /// Bones of the offered gun candidates (bone -> candidate ids), as the
        /// inspection marked them; charms hanging from the gun are found by them.
        #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
        gun_bones: std::collections::BTreeMap<String, Vec<String>>,
        /// The breast-bone request the inspection used, repeated here.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        breast_bones: Option<serde_json::Value>,
        /// The gun-file request the inspection used, repeated here.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        weapon_source: Option<crate::weapon::GunFile>,
    },
}
/// What an inspection repeats besides the source and its rotation.
#[derive(Debug, Clone, Default)]
pub struct InspectInputs {
    pub garment: Option<PathBuf>,
    pub scene: Option<serde_json::Value>,
    pub breast_bones: Option<serde_json::Value>,
    pub weapon_source: Option<crate::weapon::GunFile>,
    pub weapon_model: bool,
}
impl BlenderBridge {
    /// Runs one of the bridge's own scripts other than `worker.py` (its own
    /// `--python` entry) with `request` as its job. A script failure is
    /// answered with the script's own error words.
    pub fn execute_script<T: Serialize, R: DeserializeOwned>(
        &self,
        script: &str,
        request: &T,
        destination: &Path,
    ) -> Result<R> {
        if script == "worker.py" || !SCRIPTS.iter().any(|(name, _)| *name == script) {
            return Err(invalid("Unknown Blender bridge script."));
        }
        let destination = asset_paths::resolve(destination)?;
        fs::create_dir_all(&destination)?;
        let result = destination.join("blender-result.json");
        if result.exists() {
            fs::remove_file(&result)?;
        }
        let folder = destination.join("bridge");
        install_scripts(&folder)?;
        let job = destination.join("job.json");
        write_json(
            &job,
            &Job {
                request,
                result: path_text(&result)?,
                workspace: path_text(&destination)?,
            },
        )?;
        let script_error = || {
            fs::read(&result)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Status>(&bytes).ok())
                .filter(|status| status.status == "error")
                .and_then(|status| status.error)
        };
        if let Err(error) = self.executor.run(
            &CommandSpec::new(&self.executable)
                .args([
                    "--background",
                    "--factory-startup",
                    "--disable-autoexec",
                    "--python-exit-code",
                    "23",
                    "--python",
                ])
                .arg(folder.join(script))
                .arg("--")
                .arg(job)
                .current_dir(&destination),
        ) {
            return Err(script_error().map_or(error.into(), invalid));
        }
        if !result.is_file() {
            return Err(invalid("Blender returned without a result. See the full Blender log."));
        }
        let bytes = fs::read(&result)?;
        let status: Status = serde_json::from_slice(&bytes)?;
        if status.status != "ok" {
            return Err(invalid(status.error.unwrap_or_else(|| "Blender stopped without saying why.".into())));
        }
        Ok(serde_json::from_slice(&bytes)?)
    }
    pub fn execute<T: Serialize, R: DeserializeOwned>(
        &self,
        request: &T,
        destination: &Path,
    ) -> Result<R> {
        let destination = asset_paths::resolve(destination)?;
        fs::create_dir_all(&destination)?;
        let result = destination.join("blender-result.json");
        if result.exists() {
            fs::remove_file(&result)?;
        }
        let worker = install_scripts(&destination.join("bridge"))?;
        let job = destination.join("job.json");
        write_json(
            &job,
            &Job {
                request,
                result: path_text(&result)?,
                workspace: path_text(&destination)?,
            },
        )?;
        self.executor.run(
            &CommandSpec::new(&self.executable)
                .args([
                    "--background",
                    "--factory-startup",
                    "--disable-autoexec",
                    "--python-exit-code",
                    "23",
                    "--python",
                ])
                .arg(worker)
                .arg("--")
                .arg(job)
                .current_dir(&destination),
        )?;
        if !result.is_file() {
            return Err(invalid(
                "Blender returned without a preparation receipt. See the full Blender log.",
            ));
        }
        let bytes = fs::read(&result)?;
        let status: Status = serde_json::from_slice(&bytes)?;
        if status.status != "ok" {
            return Err(invalid(
                status
                    .error
                    .unwrap_or_else(|| "Blender preparation failed.".into()),
            ));
        }
        Ok(serde_json::from_slice(&bytes)?)
    }
    pub fn inspect<R: DeserializeOwned>(
        &self,
        source: &Path,
        destination: &Path,
        rotation: [f64; 3],
        garment: Option<&Path>,
        scene: Option<serde_json::Value>,
        breast_bones: Option<&serde_json::Value>,
    ) -> Result<R> {
        let inputs = InspectInputs {
            garment: garment.map(Path::to_path_buf),
            scene,
            breast_bones: breast_bones.cloned(),
            ..Default::default()
        };
        self.inspect_with(source, destination, rotation, &inputs)
    }
    pub fn inspect_with<R: DeserializeOwned>(
        &self,
        source: &Path,
        destination: &Path,
        rotation: [f64; 3],
        inputs: &InspectInputs,
    ) -> Result<R> {
        if !rotation.iter().all(|v| v.is_finite()) {
            return Err(invalid("Source rotation must contain finite values."));
        }
        self.execute(
            &Request::Inspect {
                source: path_text(source)?,
                rotation,
                garment_source: inputs.garment.as_deref().map(path_text).transpose()?,
                scene: inputs.scene.clone(),
                breast_bones: inputs.breast_bones.clone(),
                weapon_source: inputs.weapon_source.clone(),
                mode: inputs.weapon_model.then_some(deadlock_project::ModelInputMode::Weapon),
            },
            destination,
        )
    }
}
impl BlendStager for BlenderBridge {
    fn stage_blend(&self, source: &Path, destination: &Path) -> Result<Vec<PackedImage>> {
        #[derive(Deserialize)]
        struct Reply {
            #[serde(default)]
            packed_images: Vec<PackedImage>,
        }
        let destination = asset_paths::resolve(destination)?;
        let response: Reply = self.execute(
            &Request::StageBlend {
                source: path_text(source)?,
                destination: path_text(&destination)?,
            },
            &destination.parent().unwrap().join("blend-import"),
        )?;
        Ok(response.packed_images)
    }
    fn stage_mmd(&self, source: &Path, destination: &Path) -> Result<Vec<PackedImage>> {
        #[derive(Deserialize)]
        struct Reply {
            #[serde(default)]
            packed_images: Vec<PackedImage>,
        }
        let destination = asset_paths::resolve(destination)?;
        let job = destination.parent().unwrap().join("mmd-import");
        let response: Reply = self
            .execute(
                &Request::StageMmd {
                    source: path_text(source)?,
                    destination: path_text(&destination)?,
                },
                &job,
            )
            .map_err(|error| {
                // The worker's own words (MMD Tools missing for this Blender,
                // a file it cannot read) instead of "External tool failed";
                // the full Blender output stays in the logs.
                fs::read(job.join("blender-result.json"))
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<Status>(&bytes).ok())
                    .filter(|status| status.status == "error")
                    .and_then(|status| status.error)
                    .map_or(error, invalid)
            })?;
        Ok(response.packed_images)
    }
}
pub fn correction_command(executable: &Path, path: &Path, workspace: &Path) -> Result<CommandSpec> {
    let path = asset_paths::resolve(path)?;
    let workspace = asset_paths::resolve(workspace)?;
    if !path.starts_with(&workspace) || crate::source::suffix(&path) != "blend" || !path.is_file() {
        return Err(invalid(
            "Only a workspace prepared.blend may be opened for correction.",
        ));
    }
    check_executable(executable)?;
    Ok(CommandSpec::new(executable)
        .arg("--disable-autoexec")
        .arg(path))
}
