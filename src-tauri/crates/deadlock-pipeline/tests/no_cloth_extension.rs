//! Fresh source discovery + compiled validation, no simulated compiler acceptance.
#![cfg(all(
    target_arch = "x86_64",
    any(target_os = "linux", target_os = "windows")
))]
use deadlock_core::{
    process::{Runner, RunnerOptions},
    CancellationToken, TimingRecorder,
};
use deadlock_pipeline::{
    inspect::{ModelPipeline, SourceBoneInspection},
    motion::ValidatedMotion,
};
use deadlock_project::{MeshMapping, Project, Settings};
use deadlock_resource::{binary, container, Kind, Value};
use deadlock_skeleton::{
    export::{export_dmx, PreparedReport},
    Skeleton,
};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
fn fixture(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/no-cloth-extension")
            .join(name),
    )
    .unwrap()
}
fn write(path: &Path, data: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, data).unwrap()
}
fn edit(raw: &[u8], block: &str, f: impl FnOnce(&mut Value)) -> Vec<u8> {
    let layout = container::layout(raw).unwrap();
    let (ordinal, block) = layout
        .blocks
        .iter()
        .enumerate()
        .find(|(_, b)| b.name == block)
        .unwrap();
    let mut data = binary::decode(container::payload(raw, block).unwrap()).unwrap();
    f(&mut data.root);
    container::rebuild_ordinals(
        raw,
        &BTreeMap::from([(
            ordinal,
            binary::encode(&data.root, data.format_guid).unwrap(),
        )]),
    )
    .unwrap()
}

// Preparation deliberately keeps the compile template/reference hero-only.
// Its DMX also contains weighted secondary bones, before any mesh is assigned.
struct PreparedExtension {
    temp: tempfile::TempDir,
    project: Project,
    settings: Settings,
    original: Skeleton,
    combined: Skeleton,
    prepared: PreparedReport,
}
impl PreparedExtension {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let mut project = Project::default();
        project.original_model = "models/test/model.vmdl_c".into();
        project.imported_source = temp.path().join("source").display().to_string();
        project.original_compiled = temp.path().join("original.vmdl_c").display().to_string();
        project.dmx = temp.path().join("prepared.dmx").display().to_string();
        project.mesh_template = temp.path().join("mesh_mapping.vmdl").display().to_string();
        project.skeleton_reference = temp
            .path()
            .join("skeleton_reference.json")
            .display()
            .to_string();
        project.generate_materials = false;
        // Match the reported project: auto-map is on, but Tangtang_mesh does
        // not match the hero's body/cape names, so the first selection is empty.
        assert!(project.auto_map && project.meshes.is_empty());
        let source = format!("{}\n{{ rootNode={{_class=\"RootNode\" children=[{{_class=\"RenderMeshList\" children=[{{_class=\"RenderMeshFile\" name=\"body\" filename=\"models/test/body.dmx\"}},{{_class=\"RenderMeshFile\" name=\"cape\" filename=\"models/test/cape.dmx\"}}]}}]}} }}", deadlock_model::modeldoc::header(28).unwrap());
        write(
            &Path::new(&project.imported_source).join(project.source_model()),
            source,
        );
        write(
            &Path::new(&project.imported_source).join("models/test/cloth.vmat"),
            b"Layer0 { shader=\"citadel_character.vfx\" }",
        );
        let raw = fixture("empty-original.vmdl_c");
        write(Path::new(&project.original_compiled), &raw);
        let read_skeleton = |raw: &[u8]| {
            let data = container::read_blocks(raw, "DATA").unwrap().remove(0);
            Skeleton::from_value(data.root.get("m_modelSkeleton").unwrap()).unwrap()
        };
        let original = read_skeleton(&raw);
        let combined = read_skeleton(&fixture("empty-candidate.vmdl_c"));
        let mesh = |name: &str, bone: &str| serde_json::json!({"name":name,"positions":[[0.,0.,0.],[1.,0.,0.],[0.,1.,0.]],"normals":[[0.,0.,1.],[0.,0.,1.],[0.,0.,1.]],"uvs":[[[0.,0.],[1.,0.],[0.,1.]]],"colors":[],"weights":[[[bone,1.]],[[bone,1.]],[[bone,1.]]],"faces":[{"vertices":[0,1,2],"corners":[0,1,2],"material":"models/test/cloth.vmat"}]});
        let prepared: PreparedReport = serde_json::from_value(serde_json::json!({"meshes":[mesh("Tangtang_mesh", "custom_skirt_tip"), mesh("ordinary_mesh", "root")]})).unwrap();
        export_dmx(&prepared, &combined, Path::new(&project.dmx)).unwrap();
        let (target, _) = deadlock_skeleton::transforms::prepare_target(&original).unwrap();
        write(
            Path::new(&project.mesh_template),
            deadlock_preparation::service::mesh_template(&target, &prepared.meshes).unwrap(),
        );
        let reference = serde_json::json!({"sha256":deadlock_core::safety::digest(Path::new(&project.original_compiled)).unwrap(),"skeleton":original,"scale_policy":deadlock_skeleton::transforms::SCALE_POLICY});
        write(
            Path::new(&project.skeleton_reference),
            serde_json::to_vec(&reference).unwrap(),
        );
        let mut settings = Settings::default();
        settings.workspace = temp.path().join("work").display().to_string();
        Self {
            temp,
            project,
            settings,
            original,
            combined,
            prepared,
        }
    }
    fn enable_automatic_motion(&mut self) {
        // Use the preparation export naming contract while keeping this fixture's
        // unfamiliar mesh names and initially empty mesh selection.
        let rename: BTreeMap<_, _> = self.combined.names.iter()
            .filter(|n| !self.original.names.contains(n))
            .map(|n| (n.clone(), format!("dmb_secondary_{}", n.strip_prefix("custom_").unwrap())))
            .collect();
        for name in &mut self.combined.names {
            if let Some(next) = rename.get(name) { *name = next.clone(); }
        }
        for mesh in &mut self.prepared.meshes {
            for (name, _) in mesh.weights.iter_mut().flatten() {
                if let Some(next) = rename.get(name) { *name = next.clone(); }
            }
        }
        export_dmx(&self.prepared, &self.combined, Path::new(&self.project.dmx)).unwrap();
        let roots: Vec<_> = self.combined.names.iter().enumerate()
            .filter(|(i, n)| n.starts_with("dmb_secondary_")
                && self.combined.parent_name(*i).is_some_and(|p| self.original.names.iter().any(|n| n == p)))
            .map(|(_, n)| n.strip_prefix("dmb_secondary_").unwrap().to_string()).collect();
        assert!(!roots.is_empty());
        self.project.preparation_options.values.extend([
            ("preserve_physics_bones".into(), true.into()),
            ("automatic_secondary_motion".into(), true.into()),
            ("physics_roots".into(), serde_json::json!(roots)),
        ]);
        self.project.customization.enabled = Some(true);
        self.project.customization.secondary_motion = Some(deadlock_project::customization::SecondaryMotion {
            enabled: Some(true), preset: Some("automatic".into()), ..Default::default()
        });
    }
    fn inspect(
        &mut self,
        check: impl FnOnce(
            &ModelPipeline<'_>,
            &mut Project,
            deadlock_pipeline::inspect::InspectionReport,
        ),
    ) {
        let cancel = CancellationToken::new();
        let clock = TimingRecorder::new("prepared-extension-template");
        let runner = Runner::new(
            self.temp.path().join("logs"),
            cancel.clone(),
            RunnerOptions::default(),
        )
        .unwrap();
        let reader = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../resources/native/cloth-reader")
            .canonicalize()
            .unwrap();
        let pipeline = ModelPipeline {
            settings: &self.settings,
            runner: &runner,
            cancel: &cancel,
            clock: &clock,
            log: &|_| {},
            reader_root: &reader,
        };
        let report = pipeline.inspect(&mut self.project).unwrap();
        check(&pipeline, &mut self.project, report);
    }
    fn inspection_error(&mut self) -> String {
        let cancel = CancellationToken::new();
        let clock = TimingRecorder::new("invalid-prepared-extension-template");
        let runner = Runner::new(
            self.temp.path().join("logs"),
            cancel.clone(),
            RunnerOptions::default(),
        )
        .unwrap();
        let reader = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../resources/native/cloth-reader")
            .canonicalize()
            .unwrap();
        let pipeline = ModelPipeline {
            settings: &self.settings,
            runner: &runner,
            cancel: &cancel,
            clock: &clock,
            log: &|_| {},
            reader_root: &reader,
        };
        pipeline.inspect(&mut self.project).unwrap_err().to_string()
    }
}

#[test]
fn automatic_motion_waits_for_mesh_mapping_then_checks_the_selected_rig() {
    let mut f = PreparedExtension::new();
    f.enable_automatic_motion();
    let before = fs::read(&f.project.dmx).unwrap();
    let options = f.project.preparation_options.clone();
    f.inspect(|pipeline, project, report| {
        assert!(report.needs_review);
        assert_eq!(report.unresolved_meshes, ["body", "cape"]);
        assert!(report.source_binding().is_none());
        assert!(report.secondary_motion.is_none());
        assert!(report.warnings.iter().any(|w| w.contains("Complete Mesh mapping")));
        assert!(matches!(report.source_bones, Some(SourceBoneInspection::Extension(_))));
        assert!(pipeline.prepare(project, &report, true).unwrap_err().to_string().contains("Review mesh mappings"));
        project.meshes.insert("body".into(), MeshMapping::One("Tangtang_mesh".into()));
        project.meshes.insert("cape".into(), MeshMapping::One("@keep".into()));
        assert!(pipeline.prepare(project, &report, true).unwrap_err().to_string().contains("Project choices changed"));
        let selected = pipeline.inspect(project).unwrap();
        assert!(!selected.needs_review);
        assert!(selected.source_binding().unwrap().required_extra_names.contains(&"dmb_secondary_skirt_tip".into()));
        let setup = selected.secondary_motion.as_ref().unwrap();
        assert_eq!(setup.preset, "automatic");
        assert_eq!(setup.groups.len(), 1);
        assert!(!setup.proxy.is_empty());
        assert!(!selected.warnings.iter().any(|w| w.contains("Complete Mesh mapping")));
    });
    assert_eq!(fs::read(&f.project.dmx).unwrap(), before);
    assert_eq!(f.project.preparation_options, options);
}

#[test]
fn automatic_motion_cannot_use_whole_source_authority_when_only_body_meshes_are_selected() {
    let mut f = PreparedExtension::new();
    f.enable_automatic_motion();
    f.project.meshes.insert("body".into(), MeshMapping::One("ordinary_mesh".into()));
    // A partial mapping can still reach review without claiming rig authority.
    f.inspect(|_, _, report| {
        assert!(report.needs_review);
        assert!(report.source_binding().is_none());
        assert!(report.secondary_motion.is_none());
    });
    f.project.meshes.insert("cape".into(), MeshMapping::One("@keep".into()));
    let error = f.inspection_error();
    assert!(error.contains("selected replacement meshes"), "{error}");
    assert!(error.contains("Mesh mapping"), "{error}");
    // Completing mapping still runs the existing group and ancestry checks.
    f.project.meshes.insert("body".into(), MeshMapping::One("Tangtang_mesh".into()));
    f.project.preparation_options.values.insert("physics_roots".into(), serde_json::json!(["missing_group"]));
    assert!(f.inspection_error().contains("Prepared secondary group is missing"));
}

#[test]
fn unmapped_prepared_secondary_bones_reach_mesh_review_then_require_fresh_selection() {
    let mut f = PreparedExtension::new();
    let before = fs::read(&f.project.dmx).unwrap();
    f.inspect(|pipeline, project, report| {
        assert!(report.needs_review);
        assert_eq!(report.unresolved_meshes, ["body", "cape"]);
        assert!(project.meshes.is_empty());
        assert!(report.source_identity.selected_meshes.is_empty());
        assert!(report.source_binding().is_none());
        let Some(SourceBoneInspection::Extension(audit)) = &report.source_bones else {
            panic!("the entire source template must be independently audited")
        };
        assert!(audit.template_original_only);
        assert!(audit.target_bind_audit.is_some());
        assert!(!audit.source_modified && !audit.weights_modified);
        assert!(pipeline
            .prepare(project, &report, true)
            .unwrap_err()
            .to_string()
            .contains("Review mesh mappings"));
        project
            .meshes
            .insert("body".into(), MeshMapping::One("Tangtang_mesh".into()));
        project
            .meshes
            .insert("cape".into(), MeshMapping::One("@keep".into()));
        assert!(pipeline
            .prepare(project, &report, true)
            .unwrap_err()
            .to_string()
            .contains("Project choices changed"));
        let selected = pipeline.inspect(project).unwrap();
        assert!(!selected.needs_review);
        assert_eq!(selected.source_identity.selected_meshes, ["Tangtang_mesh"]);
        assert!(selected
            .source_binding()
            .unwrap()
            .required_extra_names
            .contains(&"custom_skirt_tip".into()));
        let output = Path::new(&project.dmx).with_file_name("candidate.vmdl_c");
        write(&output, fixture("empty-candidate.vmdl_c"));
        let validated = pipeline
            .validate_compiled(project, &output, &selected)
            .unwrap();
        assert!(validated.skeleton_extension.unwrap().source_binding_checked);
        assert!(validated.structure.compiled_bindings.passed);
    });
    assert_eq!(fs::read(&f.project.dmx).unwrap(), before);
}

#[test]
fn whole_source_template_audit_does_not_authorize_unselected_secondary_meshes() {
    let mut f = PreparedExtension::new();
    f.project
        .meshes
        .insert("body".into(), MeshMapping::One("ordinary_mesh".into()));
    f.project
        .meshes
        .insert("cape".into(), MeshMapping::One("@keep".into()));
    f.inspect(|_, _, report| {
        assert!(!report.needs_review);
        assert_eq!(report.source_identity.selected_meshes, ["ordinary_mesh"]);
        assert!(report.source_binding().is_none());
        assert!(report.source_identity.weighted_extra_bones.is_empty());
        assert!(matches!(
            report.source_bones,
            Some(SourceBoneInspection::Extension(_))
        ));
    });
}

#[test]
fn unmapped_secondary_template_still_rejects_changed_hero_binds_parents_and_reference() {
    for variant in ["bind", "parent", "reference", "template"] {
        let mut f = PreparedExtension::new();
        let expected = match variant {
            "bind" => {
                f.combined.positions[0][0] += 0.25;
                export_dmx(&f.prepared, &f.combined, Path::new(&f.project.dmx)).unwrap();
                "bind transform changed"
            }
            "parent" => {
                let child = f.original.parents.iter().position(|p| *p >= 0).unwrap();
                f.combined.parents[child] = -1;
                export_dmx(&f.prepared, &f.combined, Path::new(&f.project.dmx)).unwrap();
                "original parent"
            }
            "reference" => {
                let mut value: serde_json::Value =
                    serde_json::from_slice(&fs::read(&f.project.skeleton_reference).unwrap())
                        .unwrap();
                value["sha256"] = serde_json::json!("f".repeat(64));
                write(
                    Path::new(&f.project.skeleton_reference),
                    serde_json::to_vec(&value).unwrap(),
                );
                "different original model"
            }
            _ => {
                let (target, _) =
                    deadlock_skeleton::transforms::prepare_target(&f.combined).unwrap();
                write(
                    Path::new(&f.project.mesh_template),
                    deadlock_preparation::service::mesh_template(&target, &f.prepared.meshes)
                        .unwrap(),
                );
                "exactly original hero bones"
            }
        };
        let error = f.inspection_error();
        assert!(error.contains(expected), "{variant}: {error}");
    }
}
#[test]
fn fresh_inspection_authorizes_only_source_bound_no_cloth_extensions() {
    for absent in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let prefix = if absent { "absent" } else { "empty" };
        let original = fixture(&format!("{prefix}-original.vmdl_c"));
        let candidate = fixture(&format!("{prefix}-candidate.vmdl_c"));
        let mut project = Project::default();
        project.original_model = "models/test/model.vmdl_c".into();
        project.imported_source = temp.path().join("source").display().to_string();
        project.original_compiled = temp.path().join("original.vmdl_c").display().to_string();
        project.dmx = temp.path().join("replacement.dmx").display().to_string();
        project.generate_materials = false;
        project.meshes = BTreeMap::from([
            ("body".into(), MeshMapping::One("body".into())),
            ("cape".into(), MeshMapping::One("cape".into())),
        ]);
        let source=format!("{}\n{{ rootNode={{_class=\"RootNode\" children=[{{_class=\"RenderMeshList\" children=[{{_class=\"RenderMeshFile\" name=\"body\" filename=\"models/test/body.dmx\"}},{{_class=\"RenderMeshFile\" name=\"cape\" filename=\"models/test/cape.dmx\"}}]}}]}} }}",deadlock_model::modeldoc::header(28).unwrap());
        write(
            &Path::new(&project.imported_source).join(project.source_model()),
            source,
        );
        write(
            &Path::new(&project.imported_source).join("models/test/cloth.vmat"),
            b"Layer0 { shader=\"citadel_character.vfx\" }",
        );
        write(Path::new(&project.original_compiled), &original);
        let data = container::read_blocks(&candidate, "DATA")
            .unwrap()
            .remove(0);
        let skeleton = Skeleton::from_value(data.root.get("m_modelSkeleton").unwrap()).unwrap();
        let mesh = |name: &str, bone: &str| serde_json::json!({"name":name,"positions":[[0.,0.,0.],[1.,0.,0.],[0.,1.,0.]],"normals":[[0.,0.,1.],[0.,0.,1.],[0.,0.,1.]],"uvs":[[[0.,0.],[1.,0.],[0.,1.]]],"colors":[],"weights":[[[bone,1.]],[[bone,1.]],[[bone,1.]]],"faces":[{"vertices":[0,1,2],"corners":[0,1,2],"material":"models/test/cloth.vmat"}]});
        let prepared: PreparedReport = serde_json::from_value(
            serde_json::json!({"meshes":[mesh("body","custom_skirt_tip"),mesh("cape","root")]}),
        )
        .unwrap();
        export_dmx(&prepared, &skeleton, Path::new(&project.dmx)).unwrap();
        let mut settings = Settings::default();
        settings.workspace = temp.path().join("work").display().to_string();
        let cancel = CancellationToken::new();
        let clock = TimingRecorder::new("no-cloth-source-extension");
        let runner = Runner::new(
            temp.path().join("logs"),
            cancel.clone(),
            RunnerOptions::default(),
        )
        .unwrap();
        let reader = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../resources/native/cloth-reader")
            .canonicalize()
            .unwrap();
        let pipeline = ModelPipeline {
            settings: &settings,
            runner: &runner,
            cancel: &cancel,
            clock: &clock,
            log: &|_| {},
            reader_root: &reader,
        };
        let report = pipeline.inspect(&mut project).unwrap();
        assert!(!report.needs_review);
        let source = report
            .source_binding()
            .expect("fresh source discovery must authorize extra bones without cloth");
        assert!(source
            .required_extra_names
            .contains(&"custom_skirt_tip".into()));
        let output = temp.path().join("candidate.vmdl_c");
        write(&output, &candidate);
        let audit = pipeline
            .validate_compiled(&project, &output, &report)
            .unwrap();
        assert!(
            audit
                .skeleton_extension
                .as_ref()
                .unwrap()
                .source_binding_checked
        );
        assert!(audit.structure.compiled_bindings.passed);
        let ValidatedMotion::Preserve { audit: motion, .. } = audit.runtime_motion else {
            panic!()
        };
        assert!(motion.passed && !motion.required);
        let bad = edit(&candidate, "MDAT", |v| {
            let Kind::Array(bones) = &mut v
                .get_mut("m_skeleton")
                .unwrap()
                .get_mut("m_bones")
                .unwrap()
                .kind
            else {
                panic!()
            };
            let Kind::Array(values) = &mut bones
                .last_mut()
                .unwrap()
                .get_mut("m_invBindPose")
                .unwrap()
                .kind
            else {
                panic!()
            };
            values[0].kind = Kind::F64(99.);
        });
        write(&output, bad);
        assert!(pipeline
            .validate_compiled(&project, &output, &report)
            .is_err());
        write(&output, &original);
        assert!(pipeline
            .validate_compiled(&project, &output, &report)
            .is_err());
        write(&output, &candidate);
        write(Path::new(&project.dmx), b"changed source authority");
        assert!(pipeline
            .validate_compiled(&project, &output, &report)
            .is_err());
    }
}
