//! Reproducible release measurements; no fabricated compiler/game timings.
//! cargo run --release -p deadlock-pipeline --example benchmark -- --v22 /path/to/v22_validated_export --output /path/to/evidence
use deadlock_core::{
    process::{Runner, RunnerOptions},
    safety, CancellationToken, TimingRecorder, TimingStatus,
};
use deadlock_dmx::{split, Dmx};
use deadlock_vpk::{Hashes, SnapshotOptions, Vpk};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
type Error = Box<dyn std::error::Error>;
type Result<T> = std::result::Result<T, Error>;
struct Bench {
    clock: TimingRecorder,
    kinds: BTreeMap<String, String>,
    outputs: BTreeMap<String, Value>,
}
impl Bench {
    fn stage<T>(&mut self, name: &str, kind: &str, work: impl FnOnce() -> Result<T>) -> Result<T> {
        let span = self.clock.start_step(name, None)?;
        self.kinds.insert(span.id().into(), kind.into());
        match work() {
            Ok(value) => {
                span.success()?;
                Ok(value)
            }
            Err(error) => {
                span.fail(error.to_string())?;
                Err(error)
            }
        }
    }
    fn output(&mut self, name: &str, value: impl serde::Serialize) -> Result<()> {
        self.outputs
            .insert(name.into(), serde_json::to_value(value)?);
        Ok(())
    }
}
fn required(args: &BTreeMap<String, String>, key: &str) -> Result<PathBuf> {
    args.get(key)
        .map(PathBuf::from)
        .ok_or_else(|| format!("Missing {key}; no substitute fixture is silently used.").into())
}
fn input(path: &Path) -> Result<Value> {
    Ok(json!({"path":path,"bytes":fs::metadata(path)?.len(),"sha256":safety::digest(path)?}))
}
fn run(bench: &mut Bench, repo: &Path, v22: &Path, output: &Path) -> Result<Value> {
    let cancel = CancellationToken::new();
    let source = v22.join("Collection.dmx");
    // The accepted Vindicta reference is a private asset (docs/DEVELOPMENT.md, Private assets).
    let cloth_path = std::env::var_os("DMB_PRIVATE_ASSETS")
        .map(PathBuf::from)
        .unwrap_or_else(|| repo.join("private-assets"))
        .join("physics-references/Vindicta-R12-Balanced/cloth_supported_hair_balanced.vmdl_c");
    if !cloth_path.is_file() {
        return Err(format!("private asset missing: {}", cloth_path.display()).into());
    }
    let oracle: Value = serde_json::from_slice(&fs::read(v22.join("rust-skeleton-oracle.json"))?)?;
    let reference: deadlock_skeleton::source_bones::SkeletonReference =
        serde_json::from_value(oracle["reference"].clone())?;
    let required = serde_json::from_value(oracle["required"].clone())?;
    let mut inputs = vec![];
    for path in [&source, &cloth_path] {
        inputs.push(input(path)?);
    }
    let bytes = bench.stage("Read real V22 DMX from filesystem", "io", || {
        Ok(fs::read(&source)?)
    })?;
    let bytes: Arc<[u8]> = bench.stage("Create shared DMX byte storage", "cpu_memory", || {
        Ok(Arc::from(bytes))
    })?;
    let dmx = bench.stage("Parse real V22 DMX", "cpu", || {
        Ok(Dmx::parse_with_cancel(bytes.clone(), &cancel)?)
    })?;
    let summary = bench.stage("Summarize real V22 meshes/materials/bones", "cpu", || {
        Ok(dmx.summary_with_cancel(&cancel)?)
    })?;
    let geometry = bench.stage("Count real V22 geometry", "cpu", || {
        Ok(dmx.geometry_counts_with_cancel(&cancel)?)
    })?;
    bench.output("dmx_summary", &summary)?;
    bench.output("geometry", geometry)?;
    let skin = bench.stage("Validate real V22 skin weights and palettes", "cpu", || {
        Ok(deadlock_skeleton::skinning::skinning(&dmx)?)
    })?;
    bench.output("skinning", skin)?;
    let bones = bench.stage(
        "Validate real V22 source bone identities and binds",
        "cpu",
        || {
            Ok(deadlock_skeleton::source_bones::source_bones(
                &dmx,
                &required,
                Some(&reference),
                Some(&reference.sha256),
            )?)
        },
    )?;
    bench.output("source_bones", bones)?;
    let transforms = bench.stage("Prepare target skeleton axes and scales", "cpu", || {
        Ok(deadlock_skeleton::transforms::prepare_target(
            &reference.skeleton,
        )?)
    })?;
    bench.output("target_transforms", transforms)?;
    let temporary = tempfile::tempdir()?;
    let pieces = bench.stage(
        "Split real V22 body at 400000 vertices and validate exact pieces",
        "cpu_io",
        || {
            Ok(split::split_file(
                &source,
                &["body".into()],
                &temporary.path().join("split"),
                400000,
                &cancel,
            )?)
        },
    )?;
    let piece_hashes: Hashes = pieces
        .groups
        .iter()
        .flat_map(|g| g.pieces.iter())
        .map(|piece| {
            let path = pieces.directory.as_ref().unwrap().join(&piece.file);
            Ok((piece.file.clone(), safety::digest(&path)?))
        })
        .collect::<Result<_>>()?;
    bench.output("split", &pieces)?;
    bench.output("split_piece_hashes", piece_hashes)?;
    let materials = bench.stage(
        "Plan real V22 materials and texture candidates",
        "cpu_io",
        || {
            Ok(deadlock_materials::plan(
                &summary.materials,
                &v22.join("source_materials"),
                &v22.join("source_materials"),
                &BTreeMap::new(),
                &cancel,
            )?)
        },
    )?;
    bench.output("material_plan", materials)?;
    let raw = bench.stage("Read packaged R12 cloth reference", "io", || {
        Ok(fs::read(&cloth_path)?)
    })?;
    let runtime = bench.stage(
        "Decode and audit native cloth runtime structure",
        "cpu",
        || Ok(deadlock_cloth::audit_resource(&raw)?),
    )?;
    bench.output("cloth_runtime", runtime)?;
    let qualified = bench.stage(
        "Validate tested R12 cloth bindings capacity and constraints",
        "cpu",
        || Ok(deadlock_cloth::qualify_tested(&raw)?),
    )?;
    bench.output("cloth_qualification", qualified)?;
    let identity = bench.stage(
        "Verify packaged independent cloth reader hashes",
        "cpu_io",
        || {
            Ok(deadlock_tools::cloth_reader::identity(
                &repo.join("resources/native/cloth-reader"),
                &cancel,
            )?)
        },
    )?;
    bench.output("independent_reader_identity", &identity)?;
    let runner = Runner::new(
        output.join("native-logs"),
        cancel.clone(),
        RunnerOptions {
            timeout: Duration::from_secs(120),
            ..Default::default()
        },
    )?;
    let layout = deadlock_vpk::resource_blocks(&raw)?;
    for block in ["PHYS", "DATA"] {
        let result = bench.stage(
            &format!("Run full-precision independent reader {block}"),
            "external_process",
            || Ok(runner.run(&identity.command(&cloth_path, block)?)?),
        )?;
        let text = result.read_stdout_utf8(512 * 1024 * 1024)?;
        let proof = bench.stage(
            &format!("Parse and exactly verify independent {block} output"),
            "cpu",
            || {
                let documents = deadlock_resource::independent::parse_cli_blocks(&text)?;
                if documents.len() != 1 {
                    return Err("Independent reader returned ambiguous blocks".into());
                }
                let binary = layout
                    .blocks
                    .iter()
                    .find(|b| b.name == block)
                    .ok_or("Missing original block")?;
                let decoded = deadlock_resource::binary::decode(
                    &raw[binary.offset..binary.offset + binary.size],
                )?;
                Ok(deadlock_resource::independent::verify(
                    &documents[0].root,
                    &decoded.root,
                )?)
            },
        )?;
        bench.output(&format!("independent_{block}_process"), result)?;
        bench.output(&format!("independent_{block}_proof"), proof)?;
    }
    let game = temporary.path().join("game");
    fs::create_dir(&game)?;
    let package_target = "models/heroes_staging/hornet_v3/hornet.vmdl_c".to_string();
    bench.stage(
        "Stage package input with atomic copy and SHA256 verification",
        "cpu_io",
        || {
            Ok(safety::atomic_copy(
                &cloth_path,
                &safety::inside(&game, &package_target)?,
                &cancel,
            )?)
        },
    )?;
    let mut options = SnapshotOptions::default();
    options.required = [package_target.clone()].into();
    let stage = temporary.path().join("frozen");
    let snapshot = bench.stage(
        "Freeze package snapshot with stable inventory and copied-byte verification",
        "cpu_io_settle",
        || {
            Ok(deadlock_vpk::create_snapshot(
                &game,
                &stage,
                &options,
                &mut || Ok(()),
                &mut |_| {},
            )?)
        },
    )?;
    bench.output("snapshot", &snapshot)?;
    let package = temporary.path().join("pak18_dir.vpk");
    let receipt = bench.stage(
        "Write built-in VPK v2 from exact allowlist",
        "cpu_io",
        || {
            Ok(deadlock_vpk::write_vpk(
                &stage,
                &snapshot.files.keys().cloned().collect::<Vec<_>>(),
                &package,
                Some(&snapshot.files),
                2,
                &mut || Ok(()),
            )?)
        },
    )?;
    bench.output("vpk_write", receipt)?;
    let archive = bench.stage("Open and parse built-in VPK directory", "cpu_io", || {
        Ok(Vpk::open(&package)?)
    })?;
    let hashes = bench.stage(
        "Read every VPK entry and verify CRC and SHA256",
        "cpu_io",
        || Ok(archive.verify_files(Some(&snapshot.files), &mut || Ok(()))?),
    )?;
    bench.output("vpk_verified_hashes", hashes)?;
    bench.stage(
        "Read back representative compiled VPK resource",
        "cpu_io",
        || {
            let bytes = archive.read(&package_target)?;
            if bytes != raw {
                return Err("VPK payload differs from input".into());
            }
            Ok(())
        },
    )?;
    bench.stage("Clean owned benchmark snapshot", "io", || {
        Ok(deadlock_vpk::remove_snapshot(&stage)?)
    })?;
    bench.output("workload",json!({"dmx_bytes":bytes.len(),"mesh_count":summary.meshes.len(),"material_count":summary.materials.len(),"bone_count":summary.bones.len(),"split_runs":1,"cloth_reference_bytes":raw.len(),"package_files":snapshot.files.len(),"snapshot_settle_ms":options.settle_time.as_millis(),"native_valve_compiler_measured":false,"live_game_measured":false,"performance_superiority_claimed":false}))?;
    let stages: BTreeSet<_> = bench.kinds.values().cloned().collect();
    Ok(json!({"inputs":inputs,"classes":stages}))
}
fn main() -> Result<()> {
    if cfg!(debug_assertions) {
        return Err(
            "Use cargo run --release; debug timings are not accepted benchmark evidence.".into(),
        );
    }
    let values: Vec<_> = std::env::args().skip(1).collect();
    if values.len() % 2 != 0 {
        return Err("Arguments must be --v22 DIR --output DIR [--repo DIR]".into());
    }
    let args: BTreeMap<_, _> = values
        .chunks(2)
        .map(|p| (p[0].clone(), p[1].clone()))
        .collect();
    if args
        .keys()
        .any(|k| !["--repo", "--v22", "--output"].contains(&k.as_str()))
    {
        return Err("Unknown benchmark argument".into());
    }
    let repo = args
        .get("--repo")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.."));
    let repo = repo.canonicalize()?;
    let v22 = required(&args, "--v22")?.canonicalize()?;
    let output = required(&args, "--output")?;
    fs::create_dir_all(&output)?;
    let mut bench = Bench {
        clock: TimingRecorder::new("Native release workload benchmark"),
        kinds: BTreeMap::new(),
        outputs: BTreeMap::new(),
    };
    bench.clock.enable_journal(&output.join(format!(
        "timing-events-{}.jsonl",
        uuid::Uuid::new_v4().simple()
    )))?;
    let outcome = run(&mut bench, &repo, &v22, &output);
    let status = if outcome.is_ok() {
        TimingStatus::Succeeded
    } else {
        TimingStatus::Failed
    };
    let error = outcome.as_ref().err().map(ToString::to_string);
    let report = bench.clock.finish(status, error.clone())?;
    safety::write_json(&output.join("timings.json"), &report)?;
    let document = json!({"schema":1,"profile":"release","host_os":std::env::consts::OS,"host_arch":std::env::consts::ARCH,"logical_cpus":std::thread::available_parallelism().map(|n|n.get()).ok(),"methodology":"One measured execution of each stage, including one full V22 split. Input hashes and prior workflow use can warm the OS file cache; these are not cold-disk or statistical-distribution results. Whole-run total includes evidence hashing and benchmark setup outside individual stages. No Python speedup comparison is claimed.","status":status,"error":error,"timing":report,"stage_classes":bench.kinds,"observations":bench.outputs,"input_context":outcome.as_ref().ok(),"unmeasured":["Valve resourcecompiler native model/material/image/animation compilation on Windows","Steam network tool acquisition","live Deadlock rendering, animation and cloth behavior","installation into a running game"],"source_command":"cargo run --release -p deadlock-pipeline --example benchmark -- --v22 V22_DIRECTORY --output EVIDENCE_DIRECTORY"});
    safety::write_json(&output.join("benchmark.json"), &document)?;
    for step in &report.steps {
        println!(
            "{:.6}\t{}\t{}",
            step.inclusive_ns as f64 / 1e9,
            bench
                .kinds
                .get(&step.id)
                .map(String::as_str)
                .unwrap_or("unclassified"),
            step.name
        );
    }
    println!(
        "Measured operation total: {:.6} seconds; report {}",
        report.total_ns as f64 / 1e9,
        output.join("benchmark.json").display()
    );
    outcome.map(|_| ())
}
