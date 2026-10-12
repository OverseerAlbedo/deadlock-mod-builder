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
pub const SUPPORTED_DESCRIPTION: &str = "Blender 4.2–4.5, 5.0 or 5.2 LTS";
pub fn supported_version(version: [u32; 3]) -> bool {
    matches!((version[0], version[1]), (4, 2..=5) | (5, 0 | 2))
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
    let mut candidates: Vec<_> = fs::read_dir(root)
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
    candidates.sort();
    candidates.pop()
}
const SCRIPTS: [(&str, &[u8]); 20] = [
    ("breast_bones.py", include_bytes!("../resources/blender/breast_bones.py")),
    ("scene_isolation.py", include_bytes!("../resources/blender/scene_isolation.py")),
    ("weapon_fit.py", include_bytes!("../resources/blender/weapon_fit.py")),
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
    },
    StageBlend {
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
        /// The breast-bone request the inspection used, repeated here.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        breast_bones: Option<serde_json::Value>,
    },
}
impl BlenderBridge {
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
        if !rotation.iter().all(|v| v.is_finite()) {
            return Err(invalid("Source rotation must contain finite values."));
        }
        self.execute(
            &Request::Inspect {
                source: path_text(source)?,
                rotation,
                garment_source: garment.map(path_text).transpose()?,
                scene,
                breast_bones: breast_bones.cloned(),
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
