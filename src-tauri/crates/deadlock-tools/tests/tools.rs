use deadlock_core::{
    process::{CommandSpec, ProcessResult, ToolExecutor},
    safety, CancellationToken, Result, TimingRecorder,
};
use deadlock_project::Settings;
use deadlock_tools::{acquisition as a, cloth_reader, compiler, toolchain};
use sha1::{Digest, Sha1};
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};
fn settings(root: &Path) -> Settings {
    let mut s = Settings::default();
    s.csdk = root.to_string_lossy().into_owned();
    s.model_backend = "csdk".into();
    s
}
fn setup(root: &Path) {
    safety::atomic_write(
        &root.join("game/bin_cs2/win64/resourcecompiler.exe"),
        b"fixture executable",
    )
    .unwrap();
    safety::atomic_write(&root.join("game/citadel/gameinfo.gi"), b"GameInfo{}").unwrap();
}
#[test]
fn tool_selection_requires_coherent_root_and_configuration() {
    let dir = tempfile::tempdir().unwrap();
    setup(dir.path());
    let mut s = settings(dir.path());
    let tool = toolchain::select(&s, true).unwrap();
    let paths = tool.paths("0123456789abcdef0123456789abcdef").unwrap();
    assert!(paths
        .content
        .ends_with("citadel_addons/dmb_0123456789abcdef_model"));
    s.model_game = dir.path().join("other/game/citadel").display().to_string();
    assert!(toolchain::select(&s, false).is_err());
    s.model_game.clear();
    s.deadlock = s.csdk.clone();
    assert!(toolchain::select(&s, false).is_err());
    s.deadlock.clear();
    fs::remove_file(dir.path().join("game/citadel/gameinfo.gi")).unwrap();
    assert!(toolchain::select(&s, true).is_err());
}
#[test]
fn tool_identity_changes_with_dll_and_config() {
    let dir = tempfile::tempdir().unwrap();
    setup(dir.path());
    let tool = toolchain::select(&settings(dir.path()), true).unwrap();
    let cancel = CancellationToken::new();
    let before = toolchain::identity(&tool, &cancel).unwrap();
    fs::write(tool.compiler.with_file_name("tier0.dll"), b"dependency").unwrap();
    let after = toolchain::identity(&tool, &cancel).unwrap();
    assert_ne!(before.fingerprint, after.fingerprint);
    fs::write(tool.game.join("gameinfo.gi"), b"changed").unwrap();
    assert_ne!(
        after.fingerprint,
        toolchain::identity(&tool, &cancel).unwrap().fingerprint
    );
}
#[test]
fn full_precision_reader_verifies_every_manifest_file_and_hash() {
    let dir = tempfile::tempdir().unwrap();
    safety::atomic_write(&dir.path().join("linux-x64/reader"), b"reader").unwrap();
    let hash = safety::digest(&dir.path().join("linux-x64/reader")).unwrap();
    let manifest = serde_json::json!({"format":"round-trip-v1","version":"fixture","platforms":{"linux-x64":{"entrypoint":"linux-x64/reader","files":{"linux-x64/reader":hash}}}});
    safety::write_json(&dir.path().join("manifest.json"), &manifest).unwrap();
    let identity =
        cloth_reader::identity_for(dir.path(), "Linux", "x86_64", &CancellationToken::new())
            .unwrap();
    assert!(identity.command(Path::new("x.vmdl_c"), "PHYS").is_ok());
    assert!(identity.command(Path::new("x.vmdl_c"), "NTRO").is_err());
    assert!(
        cloth_reader::identity(dir.path(), &CancellationToken::new()).is_err(),
        "A rewritten local manifest cannot replace the production trust pin"
    );
    fs::write(&identity.executable, b"modified").unwrap();
    assert!(
        cloth_reader::identity_for(dir.path(), "Linux", "x86_64", &CancellationToken::new())
            .is_err()
    );
}
#[test]
fn packaged_reader_matches_production_trust_pins() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../resources/native/cloth-reader");
    let identity = cloth_reader::identity(&root, &CancellationToken::new()).unwrap();
    assert_eq!(identity.format, "round-trip-v1");
}
#[test]
fn windows_reader_package_does_not_require_a_linux_executable() {
    let root=Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../resources/native/cloth-reader");
    let temp=tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("win-x64")).unwrap();
    for path in ["manifest.json","win-x64/Dmb.ClothReader.exe"] {fs::copy(root.join(path),temp.path().join(path)).unwrap();}
    let expected=cloth_reader::identity_for(&root,"Windows","x86_64",&CancellationToken::new()).unwrap();
    let actual=cloth_reader::identity_for(temp.path(),"Windows","x86_64",&CancellationToken::new()).unwrap();
    assert_eq!(actual.manifest_sha256,expected.manifest_sha256);
    assert_eq!(actual.executable_sha256,expected.executable_sha256);
    assert!(!temp.path().join("linux-x64").exists());
}
#[cfg(unix)]
#[test]
fn reader_rejects_linked_ancestors_even_with_valid_file_hashes() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("reader"), b"reader").unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("linux-x64")).unwrap();
    let hash = safety::digest(&outside.path().join("reader")).unwrap();
    safety::write_json(&dir.path().join("manifest.json"), &serde_json::json!({"format":"round-trip-v1","version":"fixture","platforms":{"linux-x64":{"entrypoint":"linux-x64/reader","files":{"linux-x64/reader":hash}}}})).unwrap();
    assert!(
        cloth_reader::identity_for(dir.path(), "Linux", "x86_64", &CancellationToken::new())
            .is_err()
    );
}
#[test]
fn compiler_commands_preserve_flags_and_path_arguments() {
    let dir = tempfile::tempdir().unwrap();
    let compiler = dir.path().join("tool with spaces.exe");
    fs::write(&compiler, b"fixture").unwrap();
    let source = Path::new("model with spaces.vmdl");
    let material = compiler::compile_command(&compiler, source, None).unwrap();
    assert_eq!(
        material
            .args
            .iter()
            .map(|s| s.to_string_lossy())
            .collect::<Vec<_>>(),
        ["-i", "model with spaces.vmdl", "-nop4"]
    );
    let model = compiler::compile_command(&compiler, source, Some(Path::new("game/csgo"))).unwrap();
    assert_eq!(
        model
            .args
            .iter()
            .map(|s| s.to_string_lossy())
            .collect::<Vec<_>>(),
        [
            "-f",
            "-i",
            "model with spaces.vmdl",
            "-game",
            "game/csgo",
            "-nop4"
        ]
    );
}
struct Fake {
    directory: PathBuf,
    output: PathBuf,
    stdout: String,
    stderr: String,
    code: i32,
    create_output: bool,
    calls: Mutex<usize>,
}
impl ToolExecutor for Fake {
    fn run(&self, _: &CommandSpec) -> Result<ProcessResult> {
        let mut calls = self.calls.lock().unwrap();
        *calls += 1;
        let out = self.directory.join(format!("stdout{calls}.txt"));
        let err = self.directory.join(format!("stderr{calls}.txt"));
        fs::write(&out, &self.stdout).unwrap();
        fs::write(&err, &self.stderr).unwrap();
        if self.create_output {
            safety::atomic_write(&self.output, include_bytes!("fixtures/model-resource.bin"))
                .unwrap();
        }
        Ok(ProcessResult {
            command_id: "fixture".into(),
            code: Some(self.code),
            success: self.code == 0,
            stdout_path: out,
            stderr_path: err,
            receipt_path: self.directory.join("receipt.json"),
            elapsed_ns: 1,
            stdout_bytes: self.stdout.len() as u64,
            stderr_bytes: self.stderr.len() as u64,
            dropped_live_events: 0,
        })
    }
}
fn fake(dir: &Path) -> Fake {
    Fake {
        directory: dir.into(),
        output: dir.join("model.vmdl_c"),
        stdout: "1 compiled, 0 failed".into(),
        stderr: String::new(),
        code: 0,
        create_output: true,
        calls: Mutex::new(0),
    }
}
#[test]
fn stale_output_cannot_pass_and_stderr_failure_is_not_ignored() {
    let dir = tempfile::tempdir().unwrap();
    setup(dir.path());
    let tool = toolchain::select(&settings(dir.path()), true).unwrap();
    let mut f = fake(dir.path());
    let timing = TimingRecorder::new("build");
    let cancel = CancellationToken::new();
    let source = dir.path().join("model.vmdl");
    safety::atomic_write(&f.output, include_bytes!("fixtures/model-resource.bin")).unwrap();
    f.create_output = false;
    assert!(compiler::compile_resource(
        &f,
        &tool.compiler,
        &source,
        &f.output,
        None,
        &timing,
        None,
        &cancel
    )
    .is_err());
    assert!(!f.output.exists());
    f.create_output = true;
    f.stderr = "error: fatal tool diagnostic".into();
    assert!(compiler::compile_resource(
        &f,
        &tool.compiler,
        &source,
        &f.output,
        None,
        &timing,
        None,
        &cancel
    )
    .is_err());
}
#[test]
fn valid_compile_keeps_native_evidence_unverified_for_fixture() {
    let dir = tempfile::tempdir().unwrap();
    setup(dir.path());
    let tool = toolchain::select(&settings(dir.path()), true).unwrap();
    let f = fake(dir.path());
    let timing = TimingRecorder::new("build");
    let result = compiler::compile_resource(
        &f,
        &tool.compiler,
        &dir.path().join("model.vmdl"),
        &f.output,
        None,
        &timing,
        None,
        &CancellationToken::new(),
    )
    .unwrap();
    assert!(result.resource_references.is_empty());
    assert_eq!(timing.report().steps.len(), 3);
    assert!(!f.native_execution());
}
#[test]
fn probe_accepts_only_exact_loader_mismatch_and_cleans_up() {
    let dir = tempfile::tempdir().unwrap();
    setup(dir.path());
    let tool = toolchain::select(&settings(dir.path()), true).unwrap();
    let mut f = fake(dir.path());
    f.create_output = false;
    f.code = 1;
    f.stdout = "No valid format conversion from 'dmb_modeldoc_probe' to 'modeldoc41'".into();
    let content = dir.path().join("content");
    let report = dir.path().join("format.json");
    let value = compiler::detect_target(
        &tool,
        &f,
        &content,
        &tool.game,
        &report,
        "auto",
        &TimingRecorder::new("probe"),
        None,
        &CancellationToken::new(),
    )
    .unwrap();
    assert_eq!(value.target, Some(41));
    assert!(!value.native);
    assert_eq!(fs::read_dir(&content).unwrap().count(), 0);
    f.stdout = "compile error unrelated".into();
    assert!(compiler::detect_target(
        &tool,
        &f,
        &content,
        &tool.game,
        &report,
        "auto",
        &TimingRecorder::new("probe"),
        None,
        &CancellationToken::new()
    )
    .is_err());
    assert!(
        serde_json::from_slice::<serde_json::Value>(&fs::read(report).unwrap()).unwrap()["error"]
            .is_string()
    );
}
fn manifest(depot: &str, id: &str, items: &[(String, Vec<u8>, u32)]) -> String {
    let mut text = format!(
        "Content Manifest for Depot {depot}\nManifest ID / date : {id} / date\nTotal number of files : {}\nTotal bytes on disk : {}\n",
        items.len(),
        items.iter().map(|r| r.1.len()).sum::<usize>()
    );
    for (name, data, flags) in items {
        text.push_str(&format!(
            "{} 1 {:x} {flags:x} {name}\n",
            data.len(),
            Sha1::digest(data)
        ));
    }
    text
}
fn plan_fixture(root: &Path) -> (a::Recipe, BTreeMap<String, Vec<(String, Vec<u8>, u32)>>) {
    let items = fixture_items();
    (fixture_recipe(root, items.iter()), items)
}
fn fixture_items() -> BTreeMap<String, Vec<(String, Vec<u8>, u32)>> {
    let mut items: BTreeMap<String, Vec<(String, Vec<u8>, u32)>> = BTreeMap::new();
    items.insert(
        "2347770".into(),
        a::REQUIRED
            .iter()
            .filter(|p| p.ends_with(".gi"))
            .map(|p| (p.to_string(), b"config".to_vec(), 0))
            .chain([(String::from("game/csgo/pak01_000.vpk"), b"bulk".to_vec(), 0)])
            .collect::<Vec<_>>(),
    );
    items.insert(
        "2347771".into(),
        vec![
            (
                "game/bin/win64/resourcecompiler.dll".into(),
                b"rc".to_vec(),
                0,
            ),
            ("game/bin/win64/shared.dll".into(), b"base".to_vec(), 0),
        ],
    );
    items.insert(
        "2347779".into(),
        vec![
            (
                "game/bin/win64/resourcecompiler.exe".into(),
                b"exe".to_vec(),
                0,
            ),
            (
                "game/bin/win64/modeldoc_utils.dll".into(),
                b"utils".to_vec(),
                0,
            ),
            ("game/bin/win64/shared.dll".into(), b"tools".to_vec(), 0),
            ("content/core/default.tga".into(), b"default".to_vec(), 0),
        ],
    );
    items
}
/// Writes the manifests of `items` into `root`; the recipe lists the depots in
/// the order given.
fn fixture_recipe<'a>(
    root: &Path,
    items: impl IntoIterator<Item = (&'a String, &'a Vec<(String, Vec<u8>, u32)>)>,
) -> a::Recipe {
    let mut depots = vec![];
    for (depot, rows) in items {
        let id = format!("{depot}01");
        fs::write(
            root.join(format!("manifest_{depot}_{id}.txt")),
            manifest(depot, &id, rows),
        )
        .unwrap();
        depots.push(a::Depot {
            depot: depot.clone(),
            manifest: id,
            whole_depot_bytes: rows.iter().map(|r| r.1.len() as u64).sum(),
            extra: BTreeMap::new(),
        });
    }
    a::Recipe {
        app: "730".into(),
        build: "fixture".into(),
        queried_utc: "fixture".into(),
        depots,
        extra: BTreeMap::new(),
    }
}
#[test]
fn plan_preserves_tools_overrides_and_excludes_bulk_game() {
    let dir = tempfile::tempdir().unwrap();
    let (recipe, items) = plan_fixture(dir.path());
    let cancel = CancellationToken::new();
    let plan = a::plan(&recipe, dir.path(), &cancel).unwrap();
    assert!(!plan.files.iter().any(|f| f.path.ends_with("pak01_000.vpk")));
    assert_eq!(
        plan.files
            .iter()
            .find(|f| f.path.ends_with("shared.dll"))
            .unwrap()
            .depot
            .as_deref(),
        Some("2347779")
    );
    assert_eq!(plan.transfer_uncompressed_bytes - plan.installed_bytes, 4);
    let tools = dir.path().join("tools");
    for file in &plan.files {
        let data = &items[file.depot.as_ref().unwrap()]
            .iter()
            .find(|r| r.0 == file.path)
            .unwrap()
            .1;
        safety::atomic_write(&tools.join(&file.path), data).unwrap();
    }
    assert!(
        a::validate_files(&plan, &tools, &cancel)
            .unwrap()
            .manifest_integrity_passed
    );
    fs::write(tools.join("game/bin/win64/OLD.dll"), b"old").unwrap();
    assert!(a::validate_files(&plan, &tools, &cancel).is_err());
}
fn texts(spec: &CommandSpec) -> Vec<String> {
    spec.args.iter().map(|s| s.to_string_lossy().into_owned()).collect()
}
/// With QR sign-in every DepotDownloader run needs its own scan, so all three
/// depots download in one run when that writes exactly the planned files.
#[test]
fn every_depot_downloads_in_one_run_when_it_writes_exactly_the_plan() {
    let dir = tempfile::tempdir().unwrap();
    let mut items = fixture_items();
    // Outside the base filter, so the plan takes it from TOOLS; one run keeps
    // it from TOOLS too, the last depot listed.
    items.get_mut("2347770").unwrap().push((
        "content/core/default.tga".into(),
        b"base default".to_vec(),
        0,
    ));
    let recipe = fixture_recipe(dir.path(), items.iter());
    let cancel = CancellationToken::new();
    let plan = a::plan(&recipe, dir.path(), &cancel).unwrap();
    let lists = dir.path().join("filelists");
    let toolset = dir.path().join("toolset");
    let dd = dir.path().join("DepotDownloader.exe");
    let commands = a::download_commands(
        &dd, &recipe, &plan, dir.path(), &lists, &toolset, &a::Auth::Qr, &cancel,
    )
    .unwrap();
    assert_eq!(commands.len(), 1);
    let (step, command) = &commands[0];
    assert_eq!(step, "Download depots 2347770, 2347771, 2347779");
    let filelist = lists.join("all-depots.txt");
    let mut expected: Vec<String> = [
        "-app", "730", "-depot", "2347770", "2347771", "2347779", "-manifest", "234777001",
        "234777101", "234777901", "-os", "windows", "-osarch", "64", "-filelist",
    ]
    .map(String::from)
    .to_vec();
    expected.push(filelist.to_string_lossy().into_owned());
    expected.extend(["-dir".into(), toolset.to_string_lossy().into_owned()]);
    expected.extend(["-validate", "-qr", "-loginid", "17843291"].map(String::from));
    assert_eq!(texts(command), expected);
    let names: Vec<String> = plan.files.iter().map(|f| f.path.clone()).collect();
    assert_eq!(
        fs::read_to_string(&filelist).unwrap(),
        format!("{}\n", names.join("\n"))
    );
    // DepotDownloader 3.4: the filelist matches every depot ignoring case, and
    // depots are written in order, so the last depot keeps a name.
    let wanted: HashSet<String> = names.iter().map(|n| n.to_lowercase()).collect();
    for depot in ["2347770", "2347771", "2347779"] {
        for (name, data, _) in &items[depot] {
            if wanted.contains(&name.to_lowercase()) {
                safety::atomic_write(&toolset.join(name), data).unwrap();
            }
        }
    }
    assert!(a::validate_files(&plan, &toolset, &cancel).is_ok());
    // The decision is made on the manifests the plan approved.
    let changed = dir.path().join("manifest_2347771_234777101.txt");
    let text = fs::read_to_string(&changed).unwrap();
    fs::write(&changed, text + "\n").unwrap();
    assert!(a::single_run_filelist(&plan, dir.path(), &cancel).is_err());
}
#[test]
fn depots_download_one_by_one_when_one_run_would_differ_from_the_plan() {
    let base_first = ["2347770", "2347771", "2347779"];
    let tools_first = ["2347779", "2347770", "2347771"];
    for (case, row, order) in [
        // The filelist matches it ignoring case, but DepotDownloader keeps
        // names by exact spelling, so the base file would be written too.
        (
            "other spelling",
            ("content/core/Default.TGA", b"base".to_vec(), 0),
            base_first,
        ),
        // Listed after TOOLS, the base depot would keep TOOLS' file.
        (
            "later depot",
            ("content/core/default.tga", b"base".to_vec(), 0),
            tools_first,
        ),
        // A folder under a planned file's name.
        ("directory", ("content/core/default.tga", vec![], 0x40), base_first),
        // .NET may match a dotless i to I.
        (
            "non-ASCII",
            ("game/b\u{131}n/win64/modeldoc_utils.dll", b"base".to_vec(), 0),
            base_first,
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let mut items = fixture_items();
        items
            .get_mut("2347770")
            .unwrap()
            .push((row.0.into(), row.1, row.2));
        let recipe = fixture_recipe(
            dir.path(),
            order.iter().map(|d| items.get_key_value(*d).unwrap()),
        );
        let cancel = CancellationToken::new();
        let toolset = dir.path().join("toolset");
        let prepared = a::prepared_plan(&recipe, dir.path(), &toolset, &cancel).unwrap();
        assert_eq!(
            a::single_run_filelist(&prepared.plan, dir.path(), &cancel).unwrap(),
            None,
            "{case}"
        );
        let lists = a::write_plan_files(dir.path(), &prepared).unwrap();
        let commands = a::download_commands(
            &dir.path().join("DepotDownloader.exe"),
            &recipe,
            &prepared.plan,
            dir.path(),
            &lists,
            &toolset,
            &a::Auth::Qr,
            &cancel,
        )
        .unwrap();
        assert_eq!(commands.len(), 3, "{case}");
        for ((step, command), depot) in commands.iter().zip(order) {
            assert_eq!(step, &format!("Download depot {depot}"), "{case}");
            let args = texts(command);
            let at = args.iter().position(|a| a == "-depot").unwrap();
            assert_eq!(args[at + 1..at + 3], [depot, "-manifest"], "{case}");
            let list = lists.join(format!("{depot}.txt"));
            assert!(args.contains(&list.to_string_lossy().into_owned()), "{case}");
        }
        assert!(!lists.join("all-depots.txt").exists(), "{case}");
    }
}
#[test]
fn downloads_without_a_filelist_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let recipe = a::recipe().unwrap();
    let dd = dir.path().join("DepotDownloader.exe");
    let list = dir.path().join("list.txt");
    for content in [None, Some("")] {
        if let Some(content) = content {
            fs::write(&list, content).unwrap();
        }
        assert!(
            a::depots_download_command(&dd, &recipe, &recipe.depots, &list, dir.path(), &a::Auth::Qr)
                .is_err()
        );
        assert!(a::download_command(
            &dd,
            &recipe,
            &recipe.depots[0],
            &list,
            dir.path(),
            &a::Auth::Qr
        )
        .is_err());
    }
    fs::write(&list, "game/csgo/gameinfo.gi\n").unwrap();
    assert!(a::depots_download_command(&dd, &recipe, &[], &list, dir.path(), &a::Auth::Qr).is_err());
    // One depot: the command it always was.
    let one = a::download_command(
        &dd,
        &recipe,
        &recipe.depots[0],
        &list,
        dir.path(),
        &a::Auth::Username("myuser".into()),
    )
    .unwrap();
    let mut expected: Vec<String> = [
        "-app", "730", "-depot", "2347770", "-manifest", "3469898947734168900", "-os", "windows",
        "-osarch", "64", "-filelist",
    ]
    .map(String::from)
    .to_vec();
    expected.push(list.to_string_lossy().into_owned());
    expected.extend(["-dir".into(), dir.path().to_string_lossy().into_owned()]);
    expected.extend(
        ["-validate", "-username", "myuser", "-remember-password", "-loginid", "17843291"]
            .map(String::from),
    );
    assert_eq!(texts(&one), expected);
}
/// The pinned Steam manifests download in one run. Needs the manifests from
/// Fetch manifests and plan: set DMB_TOOL_MANIFESTS to an acquisition folder's
/// manifests folder and run with --ignored.
#[test]
#[ignore]
fn pinned_manifests_download_in_one_run() {
    let Some(manifests) = std::env::var_os("DMB_TOOL_MANIFESTS") else {
        eprintln!("DMB_TOOL_MANIFESTS is not set; skipped");
        return;
    };
    let manifests = Path::new(&manifests);
    let cancel = CancellationToken::new();
    let plan = a::plan(&a::recipe().unwrap(), manifests, &cancel).unwrap();
    let names = a::single_run_filelist(&plan, manifests, &cancel)
        .unwrap()
        .expect("one run writes exactly the planned files");
    assert_eq!(names.len(), plan.files.len());
}
#[test]
fn malformed_manifest_paths_rows_links_and_duplicates_fail() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["../bad", "game/CON.dll", "game/test.", "game/bad\tname"] {
        assert!(a::safe_path(name).is_err());
    }
    for (rows, flags) in [
        (vec!["game/a.dll", "game/A.dll"], 0),
        (vec!["game/link.dll"], 512),
    ] {
        let items: Vec<_> = rows
            .into_iter()
            .map(|p| (p.into(), b"x".to_vec(), flags))
            .collect();
        let path = dir.path().join("manifest.txt");
        fs::write(&path, manifest("1", "2", &items)).unwrap();
        assert!(a::parse_manifest(&path, "1", "2", &CancellationToken::new()).is_err());
    }
}
#[test]
fn acquisition_ownership_and_auth_are_explicit() {
    let dir = tempfile::tempdir().unwrap();
    let recipe = a::recipe().unwrap();
    fs::write(dir.path().join("untouched"), b"keep").unwrap();
    assert!(a::owned(dir.path(), &recipe, true).is_err());
    let own = a::owned(&dir.path().join("new"), &recipe, true).unwrap();
    assert_eq!(a::owned(&own, &recipe, false).unwrap(), own);
    let qr = a::Auth::Qr.args().unwrap();
    assert_eq!(qr, ["-qr"]);
    let user = a::Auth::Username("myuser".into()).args().unwrap();
    assert!(!user.iter().any(|s| s == "-qr" || s == "-password"));
    assert!(a::Auth::Username("-bad".into()).args().is_err());
}

#[test]
fn pe_static_and_delay_imports_match_python_goldens() {
    for name in ["imports", "delay"] {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let actual = deadlock_tools::pe::imports(&root.join(format!("{name}.bin"))).unwrap();
        let expected: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join(format!("{name}.json"))).unwrap()).unwrap();
        assert_eq!(serde_json::to_value(actual).unwrap(), expected);
    }
}
#[test]
fn static_class_literals_and_rtti_do_not_claim_native_registration() {
    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join("compiler.dll");
    let mut bytes = include_bytes!("fixtures/imports.bin").to_vec();
    bytes.extend_from_slice(
        b"NmSkeletonList\0CAnimGraph2Instance\0.?AVCModelDocAnimGraph2Reference@@\0modeldoc41\0",
    );
    fs::write(&binary, bytes).unwrap();
    let report =
        deadlock_tools::binary_audit::scan_tree(dir.path(), &CancellationToken::new(), |_| {})
            .unwrap();
    assert_eq!(report["native_tools_executed"], false);
    assert_eq!(
        report["class_evidence_by_module"]["NmSkeletonList"],
        serde_json::json!(["compiler.dll"])
    );
    assert_eq!(
        report["class_evidence_by_module"]["AnimGraph2"],
        serde_json::json!(["compiler.dll"])
    );
    assert_eq!(
        report["class_evidence_by_module"]["DefaultAnimGraph2"],
        serde_json::json!([])
    );
}

#[test]
fn manual_commands_quote_apostrophes_and_never_execute() {
    let options = deadlock_tools::manual_setup::Options {
        dd: PathBuf::from("C:/Tools/O'Neil/DepotDownloader.exe"),
        toolroot: PathBuf::from("C:/DMBTools/tools"),
        manifests: PathBuf::from("C:/DMBTools/manifests"),
    };
    let value = deadlock_tools::manual_setup::preview(&options, &a::recipe().unwrap()).unwrap();
    assert!(!value.downloaded && !value.compatible);
    assert!(value.argv.values().all(|c| c.downloads.is_empty()));
    assert!(value.commands.contains("O''Neil"));
    assert!(deadlock_tools::manual_setup::powershell(&["bad\ncommand".into()]).is_err());
    // The guide reads as steps for a person: what to run, where to click, what failures mean.
    for part in ["YOU NEED", "Counter-Strike 2 in its library", "STEP 2. GET THE FILE LISTS", "Sign in with a QR code", "Export exact file plan", "are not here yet", "IF SOMETHING GOES WRONG"] {
        assert!(value.guide.contains(part), "{part}");
    }
    // Flags, numbers and plain paths stay bare; the program and odd values are quoted.
    let qr = &value.argv["qr"].metadata_command;
    assert!(qr.starts_with("& 'C:/Tools/O''Neil/DepotDownloader.exe' -app 730 -depot 2347770"), "{qr}");
    assert!(qr.contains("-dir C:/DMBTools/manifests -qr"), "{qr}");
    assert_eq!(deadlock_tools::manual_setup::powershell(&["dd.exe".into(), "-dir".into(), r"C:\My Tools".into()]).unwrap(), r"& 'dd.exe' -dir 'C:\My Tools'");
    if std::env::var_os("DMB_PRINT_GUIDE").is_some() {
        println!("{}", value.guide);
    }
}
#[test]
fn manual_export_writes_exact_review_files_outside_inputs() {
    let root = tempfile::tempdir().unwrap();
    let manifests = root.path().join("manifests");
    fs::create_dir(&manifests).unwrap();
    let (recipe, _) = plan_fixture(&manifests);
    let options = deadlock_tools::manual_setup::Options {
        dd: PathBuf::from("DepotDownloader.exe"),
        toolroot: root.path().join("tools"),
        manifests: manifests.clone(),
    };
    assert!(deadlock_tools::manual_setup::export_plan(
        &options,
        &manifests,
        &[],
        &recipe,
        &CancellationToken::new()
    )
    .is_err());
    let result = deadlock_tools::manual_setup::export_plan(
        &options,
        &root.path().join("exports"),
        &[],
        &recipe,
        &CancellationToken::new(),
    )
    .unwrap();
    let export = result.export_dir.unwrap();
    assert!(export.join("expected-files.tsv").is_file());
    // The same download as the app's own: here one run for all three depots
    // (one sign-in), filtered by the combined file list.
    assert_eq!(result.argv["qr"].downloads.len(), 1);
    assert!(result.argv["qr"]
        .downloads
        .iter()
        .all(|a| a.iter().any(|v| v == "-filelist")));
    assert!(export.join("filelists").join("all-depots.txt").is_file());
    assert!(result.guide.contains("One command for depots"), "{}", result.guide);
    assert!(!options.toolroot.exists());
}
#[test]
fn local_detection_preserves_explicit_compiler_and_finds_optional_steam_tools() {
    let root = tempfile::tempdir().unwrap();
    let steam = root.path().join("Steam");
    let full = steam.join("steamapps/common/Counter-Strike Global Offensive");
    safety::atomic_write(
        &full.join("game/bin/win64/resourcecompiler.exe"),
        b"fixture",
    )
    .unwrap();
    safety::atomic_write(&full.join("game/csgo/gameinfo.gi"), b"GameInfo{}").unwrap();
    let mut settings = Settings::default();
    settings.steam = steam.display().to_string();
    let found =
        deadlock_tools::detection::detect(&settings, &[], &CancellationToken::new()).unwrap();
    assert_eq!(found.optional_cs2, full.display().to_string());
    assert_eq!(found.model_backend, "cswin");
    settings.cswin_resourcecompiler = "explicit-choice.exe".into();
    let found =
        deadlock_tools::detection::detect(&settings, &[], &CancellationToken::new()).unwrap();
    assert_eq!(found.cswin_resourcecompiler, "explicit-choice.exe");
}

#[test]
fn sdk_game_is_explicit_and_independent_of_the_model_game() {
    let dir = tempfile::tempdir().unwrap();
    setup(dir.path());
    let mut settings = settings(dir.path());
    settings.resourcecompiler = dir
        .path()
        .join("game/bin_cs2/win64/resourcecompiler.exe")
        .display()
        .to_string();
    settings.model_game = dir
        .path()
        .join("separate-model-tools/game/csgo")
        .display()
        .to_string();
    let game = toolchain::sdk_game(&settings).unwrap();
    assert_eq!(
        game,
        safety::resolve_path(&dir.path().join("game/citadel")).unwrap()
    );
    let source = dir
        .path()
        .join("content/citadel_addons/Test Skin/rg_cloth.vmat");
    let command =
        compiler::compile_command(Path::new(&settings.resourcecompiler), &source, Some(&game))
            .unwrap();
    let selected = command.args.windows(2).find(|a| a[0] == "-game").unwrap();
    assert_eq!(safety::resolve_path(Path::new(&selected[1])).unwrap(), game);
    fs::remove_file(game.join("gameinfo.gi")).unwrap();
    let error = toolchain::sdk_game(&settings).unwrap_err().to_string();
    assert!(error.contains("The Community SDK is incomplete"));
    assert!(error.contains("gameinfo.gi"));
}

#[test]
fn supplied_stdout_only_compiler_failure_is_visible_and_never_accepted() {
    let dir = tempfile::tempdir().unwrap();
    setup(dir.path());
    let tool = toolchain::select(&settings(dir.path()), true).unwrap();
    let mut process = fake(dir.path());
    process.stdout = include_str!("fixtures/missing-gameinfo-stdout.bin").into();
    process.stderr.clear();
    process.code = 1;
    process.create_output = false;
    let error = compiler::compile_resource(
        &process,
        &tool.compiler,
        &dir.path().join("rg_cloth.vmat"),
        &process.output,
        Some(&tool.game),
        &TimingRecorder::new("failure"),
        None,
        &CancellationToken::new(),
    )
    .unwrap_err();
    let text = error.to_string();
    assert!(text.contains("Can't find 'citadel\\gameinfo.gi'"), "{text}");
    assert!(text.contains("exit code 1"), "{text}");
    assert!(
        text.contains("stderr:\n(no output on this stream)"),
        "{text}"
    );
    assert!(text.contains("Full stdout:"));
    assert!(!process.output.exists());
    assert_eq!(
        fs::read(dir.path().join("stdout1.txt")).unwrap(),
        include_bytes!("fixtures/missing-gameinfo-stdout.bin")
    );
}
