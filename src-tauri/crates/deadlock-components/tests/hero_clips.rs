//! A clip taken from another hero is authored at build from the game's files
//! only, goes through the same checks as a compiled clip, and its receipt
//! lets the install check see a changed game file or a changed row. The
//! archive is made up: one humanoid skeleton shared by a test hero and a
//! roster hero's model path, rest-pose clips, and a clip of the other hero
//! that turns the left upper arm.
use deadlock_components::{hero_clips, motion, CancellationToken};
use deadlock_preparation::clip_decode::{normalize, product, Clip, Local, Skeleton};
use deadlock_project::customization::ResourceReplacement;
use deadlock_project::taken_clip::{TakenArms, TakenClip};
use deadlock_resource::{binary, container, Kind, Value};
use deadlock_vpk::Vpk;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

const MODEL: &str = "models/heroes_wip/test/test.vmdl_c";
const TARGET: &str = "models/heroes_wip/test/idle.vnmclip_c";
/// A roster hero's model path (the hero itself is made up here).
const OTHER: &str = "models/heroes_staging/hornet_v3/hornet.vmdl";
const OTHER_CLIP: &str = "models/heroes_staging/hornet_v3/wave.vnmclip_c";
const OTHER_LAYER: &str = "models/heroes_staging/hornet_v3/lean.vnmclip_c";

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}
fn turn(axis: usize, angle: f64) -> [f64; 4] {
    let mut q = [0., 0., 0., (angle / 2.).cos()];
    q[axis] = (angle / 2.).sin();
    q
}
const HALF: f64 = std::f64::consts::FRAC_PI_2;

/// The made-up humanoid: (name, parent, position, rotation) in parent space.
fn bones() -> Vec<(&'static str, &'static str, [f64; 3], [f64; 4])> {
    let i = [0., 0., 0., 1.];
    vec![
        ("root_motion", "", [0., 0., 0.], i),
        ("pelvis", "root_motion", [0., 0., 40.], i),
        ("spine_0", "pelvis", [5., 0., 0.], turn(1, -HALF)),
        ("spine_1", "spine_0", [5., 0., 0.], i),
        ("spine_2", "spine_1", [5., 0., 0.], i),
        ("spine_3", "spine_2", [5., 0., 0.], i),
        ("neck_0", "spine_3", [5., 0., 0.], i),
        ("head", "neck_0", [4., 0., 0.], i),
        ("clavicle_L", "spine_3", [3., 2., 0.], turn(2, 1.2)),
        ("arm_upper_L", "clavicle_L", [4., 0., 0.], i),
        ("arm_lower_L", "arm_upper_L", [10., 0., 0.], i),
        ("hand_L", "arm_lower_L", [10., 0., 0.], i),
        ("finger_middle_0_L", "hand_L", [3., 0., 0.], i),
        ("clavicle_R", "spine_3", [3., -2., 0.], turn(2, -1.2)),
        ("arm_upper_R", "clavicle_R", [-4., 0., 0.], i),
        ("arm_lower_R", "arm_upper_R", [-10., 0., 0.], i),
        ("hand_R", "arm_lower_R", [-10., 0., 0.], i),
        ("leg_upper_L", "pelvis", [0., 4., -2.], turn(1, HALF)),
        ("leg_lower_L", "leg_upper_L", [18., 0., 0.], i),
        ("ankle_L", "leg_lower_L", [18., 0., 0.], i),
        ("ball_L", "ankle_L", [3., 0., 3.], i),
        ("leg_upper_R", "pelvis", [0., -4., -2.], turn(1, HALF)),
        ("leg_lower_R", "leg_upper_R", [-18., 0., 0.], i),
        ("ankle_R", "leg_lower_R", [-18., 0., 0.], i),
        ("ball_R", "ankle_R", [-3., 0., 3.], i),
    ]
}
fn rest() -> Vec<Local> {
    bones().iter().map(|b| Local { position: b.2, rotation: b.3, scale: 1. }).collect()
}
fn arm() -> usize {
    bones().iter().position(|b| b.0 == "arm_upper_L").unwrap()
}

/// The fixture's clip re-encoded with one track per bone and `frames`
/// (per frame, per bone), additive when asked.
fn clip_bytes(one: &[u8], frames: &[Vec<Local>], additive: bool) -> Vec<u8> {
    let layout = container::layout(one).unwrap();
    let block = container::unique(&layout.blocks, "DATA").unwrap();
    let mut doc = binary::decode(container::payload(one, block).unwrap()).unwrap();
    let guid = doc.format_guid;
    {
        let Kind::Object(rows) = &mut doc.root.kind else { panic!() };
        let (_, settings) = rows.iter_mut().find(|(k, _)| k == "m_trackCompressionSettings").unwrap();
        let Kind::Array(tracks) = &mut settings.kind else { panic!() };
        let first = tracks[0].clone();
        tracks.truncate(1);
        while tracks.len() < frames[0].len() {
            tracks.push(first.clone());
        }
        let (_, flag) = rows.iter_mut().find(|(k, _)| k == "m_bIsAdditive").unwrap();
        *flag = Value::new(Kind::Bool(additive));
    }
    deadlock_preparation::clip_encode::encode_with(&mut doc.root, frames, deadlock_preparation::clip_encode::Ranges::Fresh, deadlock_preparation::clip_encode::Statics::Rewrite).unwrap();
    container::rebuild(one, &BTreeMap::from([("DATA".to_string(), binary::encode(&doc.root, guid).unwrap())])).unwrap()
}
fn skeleton_bytes(one: &[u8], id: &str, reach: f64) -> Vec<u8> {
    let layout = container::layout(one).unwrap();
    let guid = binary::decode(container::payload(one, container::unique(&layout.blocks, "DATA").unwrap()).unwrap()).unwrap().format_guid;
    let s = |t: &str| Value::new(Kind::String(t.into()));
    let list = bones();
    let row = |b: &(&str, &str, [f64; 3], [f64; 4])| {
        let p = if b.0 == "arm_lower_L" { [b.2[0] * reach, b.2[1], b.2[2]] } else { b.2 };
        let values = [p[0], p[1], p[2], 1., b.3[0], b.3[1], b.3[2], b.3[3]];
        Value::new(Kind::Array(values.iter().map(|x| Value::new(Kind::F32(*x as f32))).collect()))
    };
    let parent = |b: &(&str, &str, [f64; 3], [f64; 4])| list.iter().position(|o| o.0 == b.1).map_or(-1, |p| p as i64);
    let root = Value::new(Kind::Object(vec![
        ("m_ID".into(), s(id)),
        ("m_boneIDs".into(), Value::new(Kind::Array(list.iter().map(|b| s(b.0)).collect()))),
        ("m_parentIndices".into(), Value::new(Kind::Array(list.iter().map(|b| Value::new(Kind::I64(parent(b)))).collect()))),
        ("m_parentSpaceReferencePose".into(), Value::new(Kind::Array(list.iter().map(row).collect()))),
    ]));
    container::rebuild(one, &BTreeMap::from([("DATA".to_string(), binary::encode(&root, guid).unwrap())])).unwrap()
}

/// The archive: the test hero and the other hero (the test hero's model
/// under the roster path), the skeleton, rest clips of the test hero, the
/// other hero's waving clip (its left upper arm turns a quarter turn about
/// z, by `wave`) and an additive one. `reach` scales one bone of the
/// skeleton (a game update that changes the skeleton).
fn archive(dir: &Path, wave: f64, reach: f64) -> Vpk {
    let fixture_archive = Vpk::open(fixture("components_dir.vpk")).unwrap();
    let one = fixture_archive.read(TARGET).unwrap();
    let count = Clip::decode(&one).unwrap().frames.len().max(2);
    let skeleton_id = Clip::decode(&one).unwrap().skeleton;
    let still: Vec<Vec<Local>> = vec![rest(); count];
    let waving: Vec<Vec<Local>> = (0..count)
        .map(|k| {
            let mut f = rest();
            f[arm()].rotation = normalize(product(f[arm()].rotation, turn(2, wave * k as f64 / (count - 1) as f64)));
            f
        })
        .collect();
    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for entry in fixture_archive.entries.values() {
        files.insert(entry.path.clone(), fixture_archive.read(&entry.path).unwrap());
    }
    files.insert(TARGET.into(), clip_bytes(&one, &still, false));
    files.insert("models/heroes_wip/test/layer.vnmclip_c".into(), clip_bytes(&one, &still, true));
    files.insert(format!("{OTHER}_c"), fixture_archive.read(MODEL).unwrap());
    files.insert(OTHER_CLIP.into(), clip_bytes(&one, &waving, false));
    files.insert(OTHER_LAYER.into(), clip_bytes(&one, &still, true));
    files.insert(format!("{}_c", skeleton_id.trim_end_matches("_c")), skeleton_bytes(&one, &skeleton_id, reach));
    let folder = dir.join("archive");
    for (path, bytes) in &files {
        let place = folder.join(path);
        fs::create_dir_all(place.parent().unwrap()).unwrap();
        fs::write(&place, bytes).unwrap();
    }
    let destination = dir.join("pak02_dir.vpk");
    deadlock_vpk::write_vpk(&folder, &files.keys().cloned().collect::<Vec<_>>(), &destination, None, 2, &mut || Ok(())).unwrap();
    Vpk::open(destination).unwrap()
}

fn row(taken: TakenClip) -> ResourceReplacement {
    ResourceReplacement { enabled: Some(true), target: Some(TARGET.into()), allow_shared: Some(false), taken: Some(taken), ..Default::default() }
}
/// Model-space rotation of bone `i` in frame `f` of `clip`.
fn world(clip: &Clip, skeleton: &Skeleton, f: usize, i: usize) -> [f64; 4] {
    deadlock_preparation::clip_author::worlds(skeleton, &clip.frames[f]).unwrap()[i].0
}

#[test]
fn a_taken_row_is_built_from_the_games_files_and_checked_like_a_compiled_clip() {
    let dir = tempfile::tempdir().unwrap();
    let archive = archive(dir.path(), HALF, 1.);
    let cancel = CancellationToken::new();
    let taken = TakenClip::new(OTHER, OTHER_CLIP);
    let before = hero_clips::other_hero_reads();
    let (content, game) = (dir.path().join("content"), dir.path().join("game"));
    let receipt = motion::stage_with(&archive, MODEL, &[row(taken.clone())], &content, &game, &cancel, false, None).unwrap();
    assert!(hero_clips::other_hero_reads() > before, "the other hero's files are read");
    assert_eq!(receipt.files, vec![TARGET.to_string()]);
    let entry = &receipt.replacements[0];
    assert_eq!(entry.status, "verified");
    assert!(entry.made.is_none());
    let kept = entry.taken.as_ref().unwrap();
    assert_eq!((&kept.taken, kept.switches, kept.policy.as_str()), (&taken, deadlock_preparation::clip_making::SWITCHES, hero_clips::POLICY));
    assert_eq!(entry.source, PathBuf::from(OTHER_CLIP));
    assert_eq!(entry.source_sha256, kept.source_clip_sha256);
    assert!(kept.source_skeleton.ends_with(".vnmskel_c") && kept.source_skeleton_sha256.len() == 64 && kept.target_skeleton_sha256.len() == 64);
    // The same checks as a compiled clip, and the staged clip turns the arm as the other hero's does.
    assert_eq!(motion::verify_compiled(&archive, &receipt, &game, &cancel).unwrap().len(), 1);
    let staged = Clip::decode(&fs::read(game.join(TARGET)).unwrap()).unwrap();
    let source = Clip::decode(&archive.read(OTHER_CLIP).unwrap()).unwrap();
    let skeleton = deadlock_components::made_clips::clip_skeleton(&archive, &archive.read(TARGET).unwrap()).unwrap();
    let last = staged.frames.len() - 1;
    let a = world(&staged, &skeleton, last, arm());
    let b = world(&source, &skeleton, source.frames.len() - 1, arm());
    assert!(deadlock_preparation::clip_author::degrees(a, b) < 0.1, "{}", deadlock_preparation::clip_author::degrees(a, b));
    // The receipt says it was taken; rows and entries must agree.
    assert!(serde_json::to_string(&receipt).unwrap().contains("\"taken\""));
    assert!(hero_clips::same_rows(&[row(taken.clone())], &receipt.replacements));
    let mut own = taken.clone();
    own.arms = TakenArms::Own;
    assert!(!hero_clips::same_rows(&[row(own)], &receipt.replacements), "a changed choice");
    let plain = ResourceReplacement { source: Some("a.vnmclip_c".into()), target: Some(TARGET.into()), ..Default::default() };
    assert!(!hero_clips::same_rows(&[plain.clone()], &receipt.replacements), "a compiled file for a taken clip");
    assert!(deadlock_components::made_clips::same_rows(&[plain], &[]), "made rows are not the taken check's business");
    // Nothing changed in the game: the install check passes.
    hero_clips::verify_entries(&archive, &receipt.replacements, &cancel).unwrap();
}

#[test]
fn own_arms_keep_this_heros_arm_and_the_whole_body_follows_by_default() {
    let dir = tempfile::tempdir().unwrap();
    let archive = archive(dir.path(), HALF, 1.);
    let cancel = CancellationToken::new();
    let mut taken = TakenClip::new(OTHER, OTHER_CLIP);
    let whole = hero_clips::author_row(&archive, MODEL, TARGET, &taken, &cancel).unwrap();
    assert!(whole.driven.iter().any(|b| b == "arm_upper_L") && whole.own.is_empty());
    taken.arms = TakenArms::Own;
    let own = hero_clips::author_row(&archive, MODEL, TARGET, &taken, &cancel).unwrap();
    assert!(!own.driven.iter().any(|b| hero_clips::ARMS.contains(&b.as_str())));
    assert_eq!(own.own.len(), hero_clips::ARMS.len());
    let skeleton = deadlock_components::made_clips::clip_skeleton(&archive, &own.original).unwrap();
    let (game, made) = (Clip::decode(&own.original).unwrap(), Clip::decode(&own.bytes).unwrap());
    let last = made.frames.len() - 1;
    // The arm keeps the game's clip (at rest here), not the other hero's wave.
    let d = deadlock_preparation::clip_author::degrees(world(&made, &skeleton, last, arm()), world(&game, &skeleton, last, arm()));
    assert!(d < 0.1, "{d}");
    let whole_clip = Clip::decode(&whole.bytes).unwrap();
    let d = deadlock_preparation::clip_author::degrees(world(&whole_clip, &skeleton, last, arm()), world(&game, &skeleton, last, arm()));
    assert!(d > 80., "{d}");
    // Own speed: the part from the start plays at its own speed; here both clips are as
    // long, so a later start leaves too little of the other clip.
    taken.arms = TakenArms::Source;
    taken.timing = deadlock_project::taken_clip::TakenTiming::OwnSpeed;
    let part = hero_clips::author_row(&archive, MODEL, TARGET, &taken, &cancel).unwrap();
    assert!((part.report.stretch - 1.).abs() < 1e-9 && part.report.plays_from == 0., "{:?}", part.report);
    taken.start_seconds = Some(0.5 * part.report.clip_seconds);
    let late = hero_clips::author_row(&archive, MODEL, TARGET, &taken, &cancel).err().unwrap().to_string();
    assert!(late.contains(hero_clips::PART_TOO_LONG), "{late}");
}

#[test]
fn layers_other_heroes_and_missing_clips_are_refused_with_their_sentences() {
    let dir = tempfile::tempdir().unwrap();
    let archive = archive(dir.path(), HALF, 1.);
    let cancel = CancellationToken::new();
    let error = |taken: TakenClip, target: &str| hero_clips::author_row(&archive, MODEL, target, &taken, &cancel).err().unwrap().to_string();
    assert!(error(TakenClip::new(OTHER, OTHER_LAYER), TARGET).contains(hero_clips::SOURCE_ADDITIVE));
    assert!(error(TakenClip::new(OTHER, OTHER_CLIP), "models/heroes_wip/test/layer.vnmclip_c").contains(hero_clips::TARGET_ADDITIVE));
    assert!(error(TakenClip::new("models/heroes_wip/test/test.vmdl", TARGET), TARGET).contains(hero_clips::SAME_HERO));
    let gone = error(TakenClip::new("models/heroes_wip/nobody/nobody.vmdl", OTHER_CLIP), TARGET);
    assert!(gone.contains(&format!("{}: models/heroes_wip/nobody/nobody.vmdl", hero_clips::NO_SOURCE_HERO)), "{gone}");
    let missing = error(TakenClip::new(OTHER, "models/heroes_staging/hornet_v3/gone.vnmclip_c"), TARGET);
    assert!(missing.contains(hero_clips::NO_SOURCE_CLIP), "{missing}");
    // A roster hero that is not in this game.
    let absent = error(TakenClip::new("models/heroes_wip/fencer/fencer.vmdl", OTHER_CLIP), TARGET);
    assert!(absent.contains(hero_clips::NO_SOURCE_HERO), "{absent}");
    // A clip of the test hero is not a clip of the other hero.
    assert!(error(TakenClip::new(OTHER, TARGET), TARGET).contains(hero_clips::NO_SOURCE_CLIP));
}

#[test]
fn the_page_learns_before_any_row_whether_a_clip_can_be_taken() {
    let dir = tempfile::tempdir().unwrap();
    let archive = archive(dir.path(), HALF, 1.);
    assert_eq!(hero_clips::target_refusal(&archive, TARGET).unwrap(), None);
    assert_eq!(hero_clips::target_refusal(&archive, "models/heroes_wip/test/layer.vnmclip_c").unwrap(), Some(hero_clips::TARGET_ADDITIVE));
}

#[test]
fn the_install_check_refuses_a_changed_clip_or_skeleton_of_the_game() {
    let dir = tempfile::tempdir().unwrap();
    let archive = archive(dir.path(), HALF, 1.);
    let cancel = CancellationToken::new();
    let (content, game) = (dir.path().join("content"), dir.path().join("game"));
    let receipt = motion::stage_with(&archive, MODEL, &[row(TakenClip::new(OTHER, OTHER_CLIP))], &content, &game, &cancel, true, None).unwrap();
    for (name, wave, reach) in [("clip", HALF / 2., 1.), ("skeleton", HALF, 1.25)] {
        let other = dir.path().join(name);
        let updated = self::archive(&other, wave, reach);
        let error = hero_clips::verify_entries(&updated, &receipt.replacements, &cancel).unwrap_err().to_string();
        assert!(error.contains(hero_clips::GAME_CHANGED), "{name}: {error}");
    }
    // Entries without `taken` read nothing.
    let plain: Vec<motion::AnimationEntry> = receipt.replacements.iter().cloned().map(|mut e| { e.taken = None; e }).collect();
    hero_clips::verify_entries(&archive, &plain, &cancel).unwrap();
}

#[test]
fn the_page_learns_the_heroes_and_the_report_of_a_pair() {
    let dir = tempfile::tempdir().unwrap();
    let archive = archive(dir.path(), HALF, 1.);
    let cancel = CancellationToken::new();
    let heroes = hero_clips::source_heroes(&archive, MODEL, &cancel).unwrap();
    assert_eq!(heroes.len(), deadlock_components::heroes::roster().unwrap().len(), "every roster hero (the test hero is not one)");
    let other = heroes.iter().find(|h| h.hero == OTHER).unwrap();
    assert!(other.usable && other.reason.is_none(), "{other:?}");
    // Heroes this game does not have are listed with the reason.
    let absent = heroes.iter().find(|h| h.hero != OTHER).unwrap();
    assert!(!absent.usable && absent.reason.as_deref().unwrap().starts_with(hero_clips::NO_SOURCE_HERO), "{absent:?}");
    // From the other hero itself, the test hero is listed and this hero is not.
    let back = hero_clips::source_heroes(&archive, OTHER, &cancel).unwrap();
    assert!(!back.iter().any(|h| h.hero == OTHER));
    // The report of a pair: what follows, what stays, the speed and the lift.
    let taken = TakenClip::new(OTHER, OTHER_CLIP);
    let made = hero_clips::author_row(&archive, MODEL, TARGET, &taken, &cancel).unwrap();
    let report = hero_clips::report(&archive, &taken, &made).unwrap();
    assert_eq!(report.driven.len(), hero_clips::BODY.len());
    assert!(report.own.is_empty() && report.not_matched.is_empty(), "{report:?}");
    assert_eq!(report.speed_percent, 0., "both clips are as long");
    assert_eq!(report.made.follow, made.report.follow);
    assert_eq!(report.floor_lift_inches, made.report.floor_lift_inches);
    let json = serde_json::to_value(&report).unwrap();
    assert!(json.get("made").is_some() && json.get("source_name").is_some());
}

/// The build's own path (Analyze, build, the frozen-build check and install)
/// with a taken row: a row without a file of its own is accepted, and the
/// install check refuses a changed row, a changed clip of the other hero and
/// a changed skeleton, each with the sentence the user sees.
#[test]
fn a_taken_row_goes_through_the_component_service_and_its_install_check() {
    use deadlock_components::service::ComponentService;
    use deadlock_core::{process::{Runner, RunnerOptions}, TimingRecorder};
    let dir = tempfile::tempdir().unwrap();
    let archive = archive(dir.path(), HALF, 1.);
    let settings = deadlock_project::Settings { workspace: dir.path().join("workspace").to_string_lossy().into(), pak: archive.path.to_string_lossy().into(), ..Default::default() };
    let cancel = CancellationToken::new();
    let runner = Runner::new(dir.path().join("logs"), cancel.clone(), RunnerOptions::default()).unwrap();
    let clock = TimingRecorder::new("taken clips");
    let log = |_: &str| {};
    let service = ComponentService { settings: &settings, runner: &runner, cancel: &cancel, clock: &clock, log: &log };
    let mut project = deadlock_project::Project::default();
    project.original_model = MODEL.trim_end_matches("_c").into();
    project.customization.enabled = Some(true);
    project.customization.model_enabled = Some(false);
    project.customization.animations = Some(vec![row(TakenClip::new(OTHER, OTHER_CLIP))]);
    let inspection = service.inspect(&project).unwrap();
    assert!(inspection.animations[0].taken.is_some());
    assert!(!inspection.native_compile_required, "a taken clip needs no compiler");
    let output = dir.path().join("output");
    let receipt = service.compile(&project, &output).unwrap().unwrap();
    assert_eq!(receipt.files.keys().collect::<Vec<_>>(), [TARGET]);
    service.verify_snapshot(&project, &receipt, &output).unwrap();
    let package = dir.path().join("pak18_dir.vpk");
    deadlock_vpk::write_vpk(&output, &receipt.files.keys().cloned().collect::<Vec<_>>(), &package, Some(&receipt.files), 2, &mut || Ok(())).unwrap();
    let candidate = Vpk::open(&package).unwrap();
    service.verify_install(&project, &receipt, &candidate, &archive).unwrap();
    // A changed row.
    let mut changed = project.clone();
    changed.customization.animations.as_mut().unwrap()[0].taken.as_mut().unwrap().arms = TakenArms::Own;
    let error = service.verify_install(&changed, &receipt, &candidate, &archive).unwrap_err().to_string();
    assert_eq!(error, format!("Integrity check failed: {}", hero_clips::CHANGED_SINCE_BUILD));
    // The other hero's clip changed in the game: GAME_CHANGED.
    let updated = self::archive(&dir.path().join("clip"), HALF / 2., 1.);
    let error = service.verify_install(&project, &receipt, &candidate, &updated).unwrap_err().to_string();
    assert_eq!(error, format!("Integrity check failed: {}", hero_clips::GAME_CHANGED));
    // The skeleton changed: here both heroes share it, and this hero's clip names
    // it, so the check of the replaced clip's own dependencies refuses first.
    let updated = self::archive(&dir.path().join("skeleton"), HALF, 1.25);
    let error = service.verify_install(&project, &receipt, &candidate, &updated).unwrap_err().to_string();
    assert!(error.contains("Original character component dependencies changed"), "{error}");
}
