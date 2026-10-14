//! Test my setup: checks every path in Settings, compiles one small material
//! with the Community SDK and asks the model compiler which ModelDoc format it
//! reads, then removes everything it made. Catches crossed or missing tools
//! before the first real build. It does not compile a hero.
use crate::app::{AppError, Application, Result};
use deadlock_core::{safety, CancellationToken, TimingRecorder};
use deadlock_project::Settings;
use deadlock_tools::setup_checks::{field_checks, State};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// One part of the setup test.
#[derive(Debug, Clone, Serialize)]
pub struct SetupTestStep {
    pub name: String,
    pub passed: bool,
    /// What was checked, or what was found.
    pub detail: String,
    /// Why it failed, with the same help as other errors.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<AppError>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SetupTest {
    pub passed: bool,
    pub steps: Vec<SetupTestStep>,
}

/// The material the test compiles: a plain grey pbr.vfx material like the
/// ones preparation writes, without textures.
const TEST_MATERIAL: &str = "// Deadlock Mod Builder setup test; deleted after the test.\nLayer0\n{\n\tshader \"pbr.vfx\"\n\tF_USE_NPR_LIGHTING 1\n\tTextureColor1 \"[1.000000 1.000000 1.000000 0.000000]\"\n\tg_vColorTint1 \"[0.500000 0.500000 0.500000 0.000000]\"\n}\n";

const FIELD_NAMES: [(&str, &str); 11] = [
    ("workspace", "Workspace folder"),
    ("deadlock", "Deadlock game folder"),
    ("pak", "Deadlock pak01_dir.vpk"),
    ("source2viewer", "Source2Viewer CLI"),
    ("resourcecompiler", "Resource compiler"),
    ("csdk", "Community SDK"),
    ("cswin", "Model toolset"),
    ("cswin_resourcecompiler", "Model resource compiler"),
    ("model_game", "Model game folder"),
    ("vpk_tool", "VPK tool"),
    ("blender", "Blender"),
];

fn step(name: &str, outcome: Result<String>) -> SetupTestStep {
    match outcome {
        Ok(detail) => SetupTestStep { name: name.into(), passed: true, detail, error: None },
        Err(error) => SetupTestStep { name: name.into(), passed: false, detail: error.message.clone(), error: Some(error) },
    }
}

/// Removes the folders the test created (never one that existed before).
struct Created(Vec<PathBuf>);
impl Created {
    fn folder(&mut self, path: PathBuf) -> PathBuf {
        if !path.exists() {
            self.0.push(path.clone());
        }
        path
    }
}
impl Drop for Created {
    fn drop(&mut self) {
        for path in &self.0 {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

fn random_hex() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// Settings paths that stop a build (empty optional fields are fine).
pub fn path_step(settings: &Settings) -> SetupTestStep {
    let problems: Vec<String> = field_checks(settings)
        .into_iter()
        .filter(|c| c.state == State::Problem)
        .map(|c| {
            let name = FIELD_NAMES.iter().find(|(key, _)| *key == c.field).map_or(c.field.as_str(), |(_, name)| name);
            format!("{name}: {}", c.message)
        })
        .collect();
    let outcome = if problems.is_empty() {
        Ok("Every path in Settings points at the right file or folder.".to_string())
    } else {
        Err(AppError::validation("Settings has a wrong path. Fix the paths marked in red in Settings (Fix all marked paths can help).", problems.join("\n")))
    };
    step("Paths in Settings", outcome)
}

impl Application {
    pub fn test_setup(&self, settings: Settings) -> Result<SetupTest> {
        self.operation("Test my setup", |clock, token| {
            let mut steps = vec![path_step(&settings)];
            let (runner, _logs) = self.create_runner(clock, token)?;
            steps.push(step("Material compiler (Community SDK)", self.test_material(&settings, &runner, clock, token)));
            steps.push(step("Model compiler (model toolset)", self.test_model_compiler(&settings, &runner, clock, token)));
            Ok(SetupTest { passed: steps.iter().all(|s| s.passed), steps })
        })
        .map(|(result, _)| result)
    }

    fn test_material(&self, settings: &Settings, runner: &deadlock_core::process::Runner, clock: &TimingRecorder, token: &CancellationToken) -> Result<String> {
        let target_game = deadlock_tools::toolchain::sdk_game(settings)?;
        let csdk = safety::resolve_path(Path::new(&settings.csdk))?;
        let addon = format!("dmb_setup_test_{}", &random_hex()[..8]);
        let mut created = Created(vec![]);
        let content = created.folder(safety::inside(&csdk, &format!("content/citadel_addons/{addon}"))?);
        let game = created.folder(safety::inside(&csdk, &format!("game/citadel_addons/{addon}"))?);
        let source = safety::inside(&content, "materials/dmb_setup_test.vmat")?;
        let output = safety::inside(&game, "materials/dmb_setup_test.vmat_c")?;
        safety::atomic_write(&source, TEST_MATERIAL.as_bytes())?;
        let compiler = Path::new(&settings.resourcecompiler);
        deadlock_tools::compiler::compile_resource(runner, compiler, &source, &output, Some(&target_game), clock, None, token)?;
        Ok(format!("Compiled a test material with {}.", compiler.display()))
    }

    fn test_model_compiler(&self, settings: &Settings, runner: &deadlock_core::process::Runner, clock: &TimingRecorder, token: &CancellationToken) -> Result<String> {
        let toolset = deadlock_tools::toolchain::select(settings, true)?;
        let paths = toolset.paths(&random_hex())?;
        let mut created = Created(vec![]);
        let content = created.folder(paths.content);
        let game = created.folder(paths.game);
        std::fs::create_dir_all(&content).map_err(|e| AppError::io("Could not create the test folder", e))?;
        std::fs::create_dir_all(&game).map_err(|e| AppError::io("Could not create the test folder", e))?;
        let report = created.folder(std::env::temp_dir().join(format!("dmb_setup_test_{}", &random_hex()[..8])));
        std::fs::create_dir_all(&report).map_err(|e| AppError::io("Could not create the test folder", e))?;
        let probe = deadlock_tools::compiler::detect_target(&toolset, runner, &content, &game, &report.join("format.json"), "auto", clock, None, token)?;
        let format = probe.target.map_or("an unknown format".to_string(), |t| format!("ModelDoc format {t}"));
        Ok(format!("The {} model compiler answers and reads {format}. Compile your hero once to be sure it builds.", toolset.backend))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn an_empty_setup_fails_every_step_with_help_and_leaves_nothing_behind() {
        let temp = tempfile::tempdir().unwrap();
        let app = Application::new(temp.path().join("state")).unwrap();
        let result = app.test_setup(Settings::default()).unwrap();
        assert!(!result.passed);
        assert_eq!(result.steps.len(), 3);
        for s in &result.steps {
            assert!(!s.passed, "{s:?}");
            assert!(s.error.is_some(), "{s:?}");
        }
        assert!(result.steps[1].detail.contains("Community SDK"), "{:?}", result.steps[1]);
        // A Community SDK without its compiler fails before anything is written.
        let csdk = temp.path().join("csdk");
        std::fs::create_dir_all(csdk.join("game/citadel")).unwrap();
        std::fs::write(csdk.join("game/citadel/gameinfo.gi"), b"gameinfo").unwrap();
        let settings = Settings { csdk: csdk.to_string_lossy().into_owned(), ..Settings::default() };
        let result = app.test_setup(settings).unwrap();
        assert!(!result.steps[1].passed);
        assert!(!csdk.join("content").exists() && !csdk.join("game/citadel_addons").exists());
    }
}

#[cfg(test)]
mod real_tests {
    /// Runs the setup test with the settings file in DMB_SETTINGS (run with --ignored).
    #[test]
    #[ignore = "needs the real tools"]
    fn real_setup() {
        let Ok(path) = std::env::var("DMB_SETTINGS") else { return };
        let settings: deadlock_project::Settings = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let temp = tempfile::tempdir().unwrap();
        let app = crate::app::Application::new(temp.path().join("state")).unwrap();
        let result = app.test_setup(settings.clone()).unwrap();
        for s in &result.steps {
            println!("{}: {} — {}{}", s.name, s.passed, s.detail, s.error.as_ref().map(|e| format!(" [{}]", e.details)).unwrap_or_default());
        }
        assert!(result.passed);
        let addons = std::path::Path::new(&settings.csdk).join("content/citadel_addons");
        assert!(!std::fs::read_dir(addons).unwrap().any(|e| e.unwrap().file_name().to_string_lossy().starts_with("dmb_setup_test")));
    }
}
