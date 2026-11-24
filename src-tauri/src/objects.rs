//! Extras → Objects (ability objects): the commands a page uses.
//!
//! * `ability_object_catalog`: the hero's ability objects
//!   (`deadlock_components::ability_objects::catalog`). Read-only and
//!   lease-free; kept per game archive (path, size, modification time) and
//!   hero, so opening the page again is instant. Nothing about objects runs at
//!   project open or in `discover_extras`.
//! * `prepare_ability_object`: stages the user's file and runs Blender
//!   (`deadlock_preparation::ability_object::prepare`) under the operation
//!   lease; a current preparation is reused without Blender.
//! * `ability_object_fit`: the fit view's data in one response: the game
//!   object's meshes and the user's prepared preview mesh, both as preview
//!   packets in the game object's frame, and the fit's transform. The game's
//!   object is decompiled once (under the operation lease); after that the
//!   command takes no lease, so moving a fit slider never waits for a build.
use crate::app::{AppError, Application, Result};
use deadlock_components::ability_objects::{self as objects, ObjectCatalog};
use deadlock_core::CancellationToken;
use deadlock_pipeline::ability_objects as build;
use deadlock_preparation::ability_object as preparation;
use deadlock_project::{ability_objects::ObjectFit, Project};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// Messages of the Objects commands (matched by `troubleshooting.rs`; the page compares them).
pub const OBJECTS_NO_GAME: &str = "Choose the current Deadlock pak01_dir.vpk in Setup to see the hero's ability objects";
pub const OBJECTS_NO_BLENDER: &str = "Ability objects are prepared with Blender. Choose blender.exe in Settings";

type Stamp = (u64, u128);
fn stamp(path: &Path) -> Option<Stamp> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_nanos()).unwrap_or(0);
    Some((meta.len(), modified))
}
struct Kept {
    pak: String,
    stamp: Stamp,
    archive: Arc<deadlock_vpk::Vpk>,
    /// Casefolded hero model → its catalog.
    catalogs: BTreeMap<String, Arc<ObjectCatalog>>,
}
fn kept() -> &'static Mutex<Option<Kept>> {
    static KEPT: OnceLock<Mutex<Option<Kept>>> = OnceLock::new();
    KEPT.get_or_init(|| Mutex::new(None))
}
/// Whether any object catalog is kept (tests: nothing about objects runs at project open).
pub fn loaded() -> bool {
    kept().lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|k| !k.catalogs.is_empty())
}
/// What `prepare_ability_object` answers.
#[derive(Debug, Clone, Serialize)]
pub struct PreparedObject {
    /// The game model (`.vmdl`).
    pub target: String,
    pub source: String,
    /// The object's own folder in the game (its materials' prefix).
    pub root: String,
    pub prepared: preparation::Prepared,
}
/// The fit view's details (the JSON header of `ability_object_fit`'s answer).
#[derive(Debug, Clone, Serialize)]
pub struct FitView {
    pub target: String,
    /// Each preview packet in the answer's data: `game` (the game object's
    /// meshes, one packet per mesh file) or `yours` (the user's preview mesh,
    /// already through the fit), with its byte range.
    pub packets: Vec<FitPacket>,
    /// The fit's map from the prepared file's coordinates into the game
    /// object's frame, row-major (`p' = M·[p, 1]`).
    pub transform: [[f64; 4]; 4],
    /// The game object's bounds (model space, inches).
    pub game_bounds: objects::Bounds,
    /// The user's object's bounds after the fit (none before a preparation).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yours_bounds: Option<objects::Bounds>,
    pub fit: ObjectFit,
    /// The game DMX's axis system (upAxis, forwardParity, coordSys), copied to the user's mesh.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub axis: Option<(i32, i32, i32)>,
    /// The game object's collision shapes (model space, inches), for an object
    /// with collision a switch opened (absent otherwise).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collision: Option<Vec<objects::CollisionShape>>,
    /// How far the user's fitted object sticks out of that collision, with the page's line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collision_fit: Option<objects::CollisionFit>,
    /// An object with several bones a switch opened: what the user's model can follow
    /// and what each part follows (absent otherwise).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub moving: Option<MovingView>,
}
/// The fit view's part for an object with several bones (`object_follow`).
#[derive(Debug, Clone, Serialize)]
pub struct MovingView {
    pub carriers: Vec<deadlock_components::object_follow::Carrier>,
    pub default_bone: String,
    /// The file's separate objects, in file order.
    pub parts: Vec<String>,
    pub armature: Vec<String>,
    pub way: deadlock_project::ability_objects::FollowWay,
    pub bound: BTreeMap<String, String>,
    pub suggested: BTreeMap<String, String>,
    /// Why the row's way cannot be built (the engine's sentence).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refused: Option<String>,
    pub same_skeleton_open: bool,
    /// Your model is drawn without its binding (its lighter preview copy does not
    /// pair with its meshes), so the player shows it at rest.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub still: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct FitPacket {
    pub role: String,
    pub name: String,
    pub offset: usize,
    pub length: usize,
}

impl Application {
    fn object_archive(&self) -> Result<(String, Stamp, Arc<deadlock_vpk::Vpk>)> {
        let settings = self.settings()?;
        let current = stamp(Path::new(&settings.pak)).filter(|_| !settings.pak.is_empty()).ok_or_else(|| AppError::validation(OBJECTS_NO_GAME, "No game archive is configured."))?;
        let mut kept = kept().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(k) = kept.as_ref().filter(|k| k.pak == settings.pak && k.stamp == current) {
            return Ok((k.pak.clone(), k.stamp, k.archive.clone()));
        }
        let archive = Arc::new(deadlock_vpk::Vpk::open(Path::new(&settings.pak)).map_err(|e| AppError::validation("Could not read the game archive", e))?);
        *kept = Some(Kept { pak: settings.pak.clone(), stamp: current, archive: archive.clone(), catalogs: BTreeMap::new() });
        Ok((settings.pak, current, archive))
    }

    /// The hero's ability objects (lease-free, kept per game archive and hero).
    pub fn ability_object_catalog(&self, project: Project, reload: bool) -> Result<ObjectCatalog> {
        let hero = deadlock_core::text::casefold(&project.original_model);
        let (pak, current, archive) = self.object_archive()?;
        let held = || {
            kept().lock().unwrap_or_else(|e| e.into_inner()).as_ref().filter(|k| k.pak == pak && k.stamp == current).and_then(|k| k.catalogs.get(&hero).cloned())
        };
        if !reload {
            if let Some(catalog) = held() {
                return Ok((*catalog).clone());
            }
        }
        static READING: Mutex<()> = Mutex::new(());
        let _reading = READING.lock().unwrap_or_else(|e| e.into_inner());
        if !reload {
            if let Some(catalog) = held() {
                return Ok((*catalog).clone());
            }
        }
        let started = std::time::Instant::now();
        let catalog = Arc::new(objects::catalog(&archive, &project.original_model, &CancellationToken::new())?);
        let _ = self.append_log(
            "info",
            &format!("Ability objects: {} listed, {} offered ({} generic shapes left out) in {:.0} ms", catalog.counts.listed, catalog.counts.offered, catalog.counts.shared_shapes, started.elapsed().as_secs_f64() * 1000.),
        );
        if let Some(k) = kept().lock().unwrap_or_else(|e| e.into_inner()).as_mut().filter(|k| k.pak == pak && k.stamp == current) {
            k.catalogs.insert(hero, catalog.clone());
        }
        Ok((*catalog).clone())
    }

    fn offered_target(&self, project: &Project, target: &str) -> Result<String> {
        let target = deadlock_project::ability_objects::target_path(target).map_err(|e| AppError::validation("Choose one of the hero's ability objects", e))?;
        let catalog = self.ability_object_catalog(project.clone(), false)?;
        if !objects::allowed_in(&catalog).contains(&deadlock_core::text::casefold(&target)) {
            return Err(AppError::validation(objects::NOT_OFFERED, &target));
        }
        Ok(target)
    }

    /// Prepares the user's file for one ability object (Blender, under the operation lease).
    pub fn prepare_ability_object(&self, project: Project, target: String, source: String) -> Result<PreparedObject> {
        project.validate_action().map_err(|e| AppError::validation("Select a valid character project", e))?;
        let target = self.offered_target(&project, &target)?;
        let (prepared, _) = self.operation("Prepare an ability object", |clock, cancel| {
            let settings = self.settings()?;
            if settings.blender.is_empty() || !Path::new(&settings.blender).is_file() {
                return Err(AppError::validation(OBJECTS_NO_BLENDER, "No Blender is configured."));
            }
            let path = PathBuf::from(&source);
            deadlock_vpk::check_unlinked_path(&path, false).map_err(|e| AppError::validation("Unsupported object file", e))?;
            preparation::check_source(&path).map_err(|e| AppError::validation(preparation::UNSUPPORTED_FILE, e))?;
            let (runner, _observer) = self.create_runner(clock, cancel)?;
            let bridge = deadlock_preparation::blender::BlenderBridge { executable: PathBuf::from(&settings.blender), executor: Arc::new(runner) };
            let root = deadlock_pipeline::workspace::root(&settings, &project)?;
            let folder = preparation::folder(&root, &target);
            let object_root = objects::object_root(&target);
            let prepared = self.step(clock, "Prepare the object's file with Blender", || {
                preparation::prepare(&bridge, &path, &folder, &object_root, cancel).map_err(|e| AppError::validation("The object's file could not be prepared", e))
            })?;
            for warning in &prepared.warnings {
                self.append_log("warning", warning)?;
            }
            Ok(PreparedObject { target: target.clone(), source: prepared.source.clone(), root: object_root, prepared })
        })?;
        Ok(prepared)
    }

    /// The fit view's data: see the module header. `source` empty: the game object alone.
    pub fn ability_object_fit(&self, project: Project, target: String, source: Option<String>, fit: ObjectFit) -> Result<Vec<u8>> {
        project.validate_action().map_err(|e| AppError::validation("Select a valid character project", e))?;
        fit.validate().map_err(|e| AppError::validation("Invalid ability object fit", e))?;
        let target = self.offered_target(&project, &target)?;
        let settings = self.settings()?;
        let (_, _, archive) = self.object_archive()?;
        let original = match build::cached_original(&settings, &archive, &target)? {
            Some(original) => original,
            None => {
                self.operation("Read the game's ability object", |clock, cancel| {
                    let (runner, _observer) = self.create_runner(clock, cancel)?;
                    Ok(self.step(clock, "Decompile the game's ability object", || Ok(build::original(&settings, &runner, &archive, &target, cancel)?))?)
                })?
                .0
            }
        };
        let cancel = CancellationToken::new();
        let carried = build::carried(&original);
        let game_bounds = build::fit_bounds(&original)?;
        let mut data = vec![];
        let mut packets = vec![];
        let mut push = |role: &str, name: &str, bytes: Vec<u8>, packets: &mut Vec<FitPacket>| {
            packets.push(FitPacket { role: role.into(), name: name.into(), offset: data.len(), length: bytes.len() });
            data.extend(bytes);
        };
        // A carried object with levels of detail shows its first level only; parts kept as the
        // game's own (`kept_parts`) are not what the user's object stands in for and are not shown.
        let mut shown = build::replaced_mesh_files(&original)?;
        if carried {
            let text = deadlock_pipeline::sources::read_text(&original.vmdl_path()?)?;
            if let Some(first) = build::lod_groups(&text)?.into_iter().next() {
                let nodes = build::mesh_nodes(&text)?;
                shown = nodes.into_iter().filter(|n| first.contains(&n.name)).map(|n| n.filename).collect();
            }
            // Body-group states: the object as it normally shows, its default state (the one
            // the fit is made to and the one drawn bound below).
            if let Some(file) = deadlock_pipeline::object_states::default_state_file(&original)? {
                shown = vec![file];
            }
        }
        let collision = if carried {
            let raw = archive.read(&format!("{}_c", deadlock_core::text::casefold(&target))).map_err(|e| AppError::validation("Could not read the game's ability object", e))?;
            Some(objects::collision_shapes(&raw)).filter(|s| !s.is_empty())
        } else {
            None
        };
        let shell = original.facts.shell_material();
        let mut game_points = vec![];
        for mesh in &shown {
            let path = deadlock_core::safety::inside(&original.folder, mesh)?;
            let dmx = deadlock_dmx::Dmx::load(&path).map_err(|e| AppError::validation("The game's ability object mesh could not be read", e))?;
            if collision.is_some() && shell.as_ref().is_none_or(|m| build::dmx_materials(&dmx) != std::collections::BTreeSet::from([m.clone()])) {
                game_points.extend(build::dmx_points(&dmx)?);
            }
            let preview = deadlock_preview::from_dmx(&dmx, mesh.clone(), &cancel)?;
            push("game", mesh, preview.packet(&cancel)?, &mut packets);
        }
        // Several bones: the bones carrying the game's mesh, and the row's binding.
        let several = carried && original.facts.several_bones();
        let game_raw = if several { Some(archive.read(&format!("{}_c", deadlock_core::text::casefold(&target))).map_err(|e| AppError::validation("Could not read the game's ability object", e))?) } else { None };
        let mut moving = match &game_raw {
            Some(raw) => {
                let skeleton = build::game_skeleton(raw)?;
                let carriers = build::game_carriers(&original, &skeleton.names)?;
                let default_bone = deadlock_components::object_follow::default_bone(&carriers).unwrap_or_default().to_owned();
                Some(MovingView { carriers, default_bone, parts: vec![], armature: vec![], way: deadlock_project::ability_objects::FollowWay::OnePiece, bound: BTreeMap::new(), suggested: BTreeMap::new(), refused: None, same_skeleton_open: objects::switches().same_skeleton, still: false })
            }
            None => None,
        };
        let follow = project.customization.ability_objects.iter().flatten().find(|r| deadlock_core::text::casefold(&r.target) == deadlock_core::text::casefold(&target)).and_then(|r| r.follow.clone());
        let mut transform = [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.], [0., 0., 0., 1.]];
        let mut yours_bounds = None;
        let mut collision_fit = None;
        if let Some(source) = source.filter(|s| !s.trim().is_empty()) {
            let root = deadlock_pipeline::workspace::root(&settings, &project)?;
            let (_, packet) = preparation::load(&preparation::folder(&root, &target), Path::new(&source), &objects::object_root(&target), &cancel)
                .map_err(|e| AppError::validation(preparation::NOT_PREPARED, e))?;
            let (meshes, affine) = build::fitted_meshes(&packet, &fit, &game_bounds, packet.preview_meshes())?;
            transform = affine.matrix();
            yours_bounds = deadlock_components::object_fit::bounds_of(meshes.iter().flat_map(|m| m.positions.iter().copied()));
            if let Some(shapes) = &collision {
                // Measured on the full mesh, as the build writes it (the preview may be lighter).
                let (full, _) = build::fitted_meshes(&packet, &fit, &game_bounds, &packet.meshes)?;
                let game_own = objects::sticks_out(game_points.iter().copied(), shapes).unwrap_or(0.);
                collision_fit = objects::sticks_out(full.iter().flat_map(|m| m.positions.iter().copied()), shapes).map(|mine| objects::collision_fit(mine, game_own));
            }
            // Several bones: the model bound as the build binds it (so the player can move it).
            let mut bound = None;
            if let (Some(view), Some(raw)) = (moving.as_mut(), &game_raw) {
                view.parts = packet.meshes.iter().map(|m| build::part_name(m).to_owned()).collect();
                view.armature = packet.armature.iter().map(|b| b.name.clone()).collect();
                let (full, _) = build::fitted_meshes(&packet, &fit, &game_bounds, &packet.meshes)?;
                // Body-group states: every state bound as the build binds it (so a refusal is the
                // build's), the default state drawn, with that state's own carriers.
                let bound_model = match deadlock_pipeline::object_states::bound_states(&packet, &original, raw, follow.as_ref(), &full) {
                    Ok(Some(states)) => match states.into_iter().find(|s| s.state.default) {
                        Some(deadlock_pipeline::object_states::BoundState { skeleton: Some(skeleton), meshes, carriers, binding: Some(binding), .. }) => {
                            view.default_bone = deadlock_components::object_follow::default_bone(&carriers).unwrap_or_default().to_owned();
                            view.carriers = carriers;
                            Ok((skeleton, meshes, binding))
                        }
                        _ => Err(deadlock_core::CoreError::Integrity("The default state of the game's object follows no bone of its own.".into())),
                    },
                    Ok(None) => build::moving_meshes(&packet, follow.as_ref(), &original, raw, &full),
                    Err(error) => Err(error),
                };
                match bound_model {
                    Ok((skeleton, bound_meshes, binding)) => {
                        view.way = binding.way;
                        view.bound = binding.parts.clone();
                        // The lighter preview copy (a large file) is bound like the full meshes.
                        let drawn = if packet.preview.is_empty() { Some(bound_meshes) } else { build::bound_preview(&packet, &binding, &bound_meshes, &meshes) };
                        view.suggested = binding.suggested;
                        if let Some(drawn) = drawn {
                            let report = deadlock_skeleton::export::PreparedReport { meshes: drawn, extra: BTreeMap::new() };
                            bound = Some(deadlock_skeleton::export::export_bytes_with_axis(&report, &skeleton, original.axis).map_err(|e| AppError::validation("Your object could not be drawn", e))?.0);
                        } else {
                            view.still = true;
                        }
                    }
                    Err(error) => {
                        view.way = follow.as_ref().map_or_else(|| deadlock_components::object_follow::default_way(view.parts.len()), |f| f.way);
                        view.refused = Some(error.to_string().trim_start_matches("Integrity check failed: ").to_owned());
                    }
                }
            }
            let raw = match bound {
                Some(bytes) => bytes,
                None => deadlock_skeleton::export::export_rigid_bytes(&meshes, original.axis).map_err(|e| AppError::validation("Your object could not be drawn", e))?,
            };
            let dmx = deadlock_dmx::Dmx::parse(raw).map_err(|e| AppError::validation("Your object could not be drawn", e))?;
            let preview = deadlock_preview::from_dmx(&dmx, "yours".into(), &cancel)?;
            push("yours", "yours", preview.packet(&cancel)?, &mut packets);
        }
        let details = FitView { target, packets, transform, game_bounds, yours_bounds, fit, axis: original.axis, collision, collision_fit, moving };
        crate::catalogs::media("application/x-dmb-object-fit", serde_json::to_value(details).map_err(|e| AppError::new("internal", "Could not encode the fit view", e))?, data)
    }
}

impl Application {
    /// The game's own animations of an object with several bones, for the fit view's
    /// player (`deadlock_pipeline::object_motion`): lease-free; the page keeps only the
    /// newest answer.
    pub fn ability_object_motion(&self, project: Project, target: String, clip: Option<String>) -> Result<Vec<u8>> {
        project.validate_action().map_err(|e| AppError::validation("Select a valid character project", e))?;
        let target = self.offered_target(&project, &target)?;
        let (_, _, archive) = self.object_archive()?;
        let raw = archive.read(&format!("{}_c", deadlock_core::text::casefold(&target))).map_err(|e| AppError::validation("Could not read the game's ability object", e))?;
        let (header, data) = deadlock_pipeline::object_motion::motion(&archive, &target, &raw, clip.as_deref().filter(|c| !c.is_empty()))?;
        crate::catalogs::media("application/x-dmb-object-motion", serde_json::to_value(header).map_err(|e| AppError::new("internal", "Could not encode the animation", e))?, data)
    }
}

#[cfg(feature = "desktop")]
pub mod commands {
    use super::*;
    type State<'a> = tauri::State<'a, Arc<Application>>;
    async fn background<T: Send + 'static>(work: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
        tauri::async_runtime::spawn_blocking(work).await.map_err(|e| AppError::new("internal", "The ability object worker stopped", e))?
    }
    #[tauri::command]
    pub async fn ability_object_catalog(state: State<'_>, project: Project, reload: Option<bool>) -> Result<ObjectCatalog> {
        let app = state.inner().clone();
        background(move || app.ability_object_catalog(project, reload.unwrap_or(false))).await
    }
    #[tauri::command]
    pub async fn prepare_ability_object(state: State<'_>, project: Project, target: String, source: String) -> Result<PreparedObject> {
        let app = state.inner().clone();
        background(move || app.prepare_ability_object(project, target, source)).await
    }
    #[tauri::command]
    pub async fn ability_object_motion(state: State<'_>, project: Project, target: String, clip: Option<String>) -> Result<tauri::ipc::Response> {
        let app = state.inner().clone();
        background(move || app.ability_object_motion(project, target, clip)).await.map(tauri::ipc::Response::new)
    }
    #[tauri::command]
    pub async fn ability_object_fit(state: State<'_>, project: Project, target: String, source: Option<String>, fit: Option<ObjectFit>) -> Result<tauri::ipc::Response> {
        let app = state.inner().clone();
        background(move || app.ability_object_fit(project, target, source, fit.unwrap_or_default())).await.map(tauri::ipc::Response::new)
    }
}
