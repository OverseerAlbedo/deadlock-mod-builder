//! A clip made from an animation is authored from its kept packet at build,
//! with no Blender, and goes through the same checks as a compiled clip.
use deadlock_components::{made_clips, motion};
use deadlock_core::CancellationToken;
use deadlock_preparation::clip_author::{BoneRow, Export, Sample, SampleBone, POLICY};
use deadlock_preparation::clip_decode::{Clip, Skeleton};
use deadlock_preparation::clip_making::{self, Switches, SWITCHES};
use deadlock_project::customization::ResourceReplacement;
use deadlock_project::made_clip::MadeClip;
use deadlock_vpk::Vpk;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

const MODEL: &str = "models/heroes_wip/test/test.vmdl_c";
const TARGET: &str = "models/heroes_wip/test/idle.vnmclip_c";

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}
fn turn(z: f64) -> [f64; 4] {
    [0., 0., (z / 2.).sin(), (z / 2.).cos()]
}

/// The fixture archive with its clip given a second (child) track and a
/// readable skeleton for it (a chain of bones at rest, one per track).
fn with_skeleton(dir: &Path) -> Vpk {
    with_clip(dir, false)
}
/// `with_skeleton`, with the clip marked additive when `additive`. The child
/// track's rotation is stored as a fixed value (a still track).
fn with_clip(dir: &Path, additive: bool) -> Vpk {
    use deadlock_resource::{binary, container, Kind, Value};
    let fixture_archive = Vpk::open(fixture("components_dir.vpk")).unwrap();
    let one = fixture_archive.read(TARGET).unwrap();
    let layout = container::layout(&one).unwrap();
    let block = container::unique(&layout.blocks, "DATA").unwrap();
    let mut doc = binary::decode(container::payload(&one, block).unwrap()).unwrap();
    let guid = doc.format_guid;
    let first = Clip::from_value(&doc.root).unwrap();
    {
        let Kind::Object(rows) = &mut doc.root.kind else { panic!() };
        let (_, settings) = rows.iter_mut().find(|(k, _)| k == "m_trackCompressionSettings").unwrap();
        let Kind::Array(tracks) = &mut settings.kind else { panic!() };
        let mut copy = tracks[0].clone();
        let Kind::Object(fields) = &mut copy.kind else { panic!() };
        fields.iter_mut().find(|(k, _)| k == "m_bIsRotationStatic").unwrap().1 = Value::new(Kind::Bool(true));
        tracks.push(copy);
        let (_, flag) = rows.iter_mut().find(|(k, _)| k == "m_bIsAdditive").unwrap();
        *flag = Value::new(Kind::Bool(additive));
    }
    let child = deadlock_preparation::clip_decode::Local { position: [0., 0., 10.], rotation: [0., 0., 0., 1.], scale: 1. };
    let frames: Vec<Vec<_>> = first.frames.iter().map(|f| vec![deadlock_preparation::clip_decode::Local { scale: 1., ..f[0] }, child]).collect();
    deadlock_preparation::clip_encode::encode_with(&mut doc.root, &frames, deadlock_preparation::clip_encode::Ranges::Fresh, deadlock_preparation::clip_encode::Statics::Rewrite).unwrap();
    let raw = container::rebuild(&one, &BTreeMap::from([("DATA".to_string(), binary::encode(&doc.root, guid).unwrap())])).unwrap();
    let clip = Clip::decode(&raw).unwrap();
    let tracks = clip.frames[0].len();
    assert_eq!(tracks, 2);
    let s = |t: &str| Value::new(Kind::String(t.into()));
    let row = Value::new(Kind::Array([0., 0., 0., 1., 0., 0., 0., 1.].map(|x| Value::new(Kind::F32(x))).to_vec()));
    let root = Value::new(Kind::Object(vec![
        ("m_ID".into(), s(&clip.skeleton)),
        ("m_boneIDs".into(), Value::new(Kind::Array((0..tracks).map(|i| s(&format!("bone_{i}"))).collect()))),
        ("m_parentIndices".into(), Value::new(Kind::Array((0..tracks).map(|i| Value::new(Kind::I64(i as i64 - 1))).collect()))),
        ("m_parentSpaceReferencePose".into(), Value::new(Kind::Array(vec![row; tracks]))),
    ]));
    let skeleton = container::rebuild(&raw, &BTreeMap::from([("DATA".to_string(), binary::encode(&root, guid).unwrap())])).unwrap();
    let folder = dir.join("archive");
    let mut files = vec![];
    for entry in fixture_archive.entries.values() {
        let place = folder.join(&entry.path);
        fs::create_dir_all(place.parent().unwrap()).unwrap();
        let bytes = if entry.path.eq_ignore_ascii_case(TARGET) { raw.clone() } else { fixture_archive.read(&entry.path).unwrap() };
        fs::write(&place, bytes).unwrap();
        files.push(entry.path.clone());
    }
    let id = format!("{}_c", clip.skeleton.trim_end_matches("_c"));
    fs::create_dir_all(folder.join(&id).parent().unwrap()).unwrap();
    fs::write(folder.join(&id), skeleton).unwrap();
    if !files.iter().any(|f| f.eq_ignore_ascii_case(&id)) {
        files.push(id);
    }
    let destination = dir.join("pak02_dir.vpk");
    deadlock_vpk::write_vpk(&folder, &files, &destination, None, 2, &mut || Ok(())).unwrap();
    Vpk::open(destination).unwrap()
}

/// A project folder with a fit file and a Blender file, and a packet that
/// turns the clip's first non-root bone a quarter turn over one second.
fn setup(dir: &Path, archive: &Vpk) -> (made_clips::MadeInputs, ResourceReplacement, String) {
    let original = archive.read(TARGET).unwrap();
    let skeleton: Skeleton = made_clips::clip_skeleton(archive, &original).unwrap();
    let clip = Clip::decode(&original).unwrap();
    let bone = skeleton.names.iter().enumerate().find(|(i, _)| skeleton.parents[*i] >= 0).unwrap().1.clone();
    let fit = dir.join("prepared-x").join(clip_making::FIT_FILE);
    fs::create_dir_all(fit.parent().unwrap()).unwrap();
    fs::write(&fit, br#"{"policy":"animation-fit-v1"}"#).unwrap();
    let blend = dir.join("moves.blend");
    fs::write(&blend, b"a Blender file").unwrap();
    let samples = (0..5)
        .map(|k| {
            let t = k as f64 / 4.;
            let q = turn(t * std::f64::consts::FRAC_PI_2);
            Sample { time: t, bones: BTreeMap::from([(bone.clone(), SampleBone { y: q, delta: q, position: None })]) }
        })
        .collect();
    let export = Export {
        policy: POLICY.into(),
        action: "Turn".into(),
        fps: 4.,
        length: 1.,
        bones: vec![BoneRow { hero: bone.clone(), source: "Bone".into(), anchor: None, keyed: true, bind_rotation: [0., 0., 0., 1.], shoulder_bridge: false }],
        samples,
        unmatched_hero_bones: vec![],
        mode: "as_it_looks".into(),
        keyed_source_bones: vec!["Bone".into()],
        feet: vec![],
    };
    let made = MadeClip::new("Turn");
    let cancel = CancellationToken::new();
    clip_making::store_packet(dir, &blend, &fit, &made, &export, &cancel).unwrap();
    let row = ResourceReplacement { enabled: Some(true), source: Some(blend.to_string_lossy().into()), target: Some(TARGET.into()), made: Some(made), ..Default::default() };
    assert!(clip.frames.len() >= 2);
    (made_clips::MadeInputs { project_root: dir.to_path_buf(), fit: Ok(fit) }, row, bone)
}

#[test]
fn a_made_row_is_authored_from_its_packet_and_checked_like_a_compiled_clip() {
    let dir = tempfile::tempdir().unwrap();
    let archive = with_skeleton(dir.path());
    let cancel = CancellationToken::new();
    let (inputs, row, bone) = setup(dir.path(), &archive);
    let (content, game) = (dir.path().join("content"), dir.path().join("game"));
    let receipt = motion::stage_with(&archive, MODEL, &[row.clone()], &content, &game, &cancel, false, Some(&inputs)).unwrap();
    assert_eq!(receipt.files, vec![TARGET.to_string()]);
    assert!(receipt.compile_sources.is_empty());
    let entry = &receipt.replacements[0];
    assert_eq!(entry.status, "verified");
    let made = entry.made.as_ref().unwrap();
    assert_eq!((made.made.action.as_str(), made.switches), ("Turn", SWITCHES));
    // The same checks as a compiled clip, and the staged bytes play the turn.
    assert_eq!(motion::verify_compiled(&archive, &receipt, &game, &cancel).unwrap().len(), 1);
    let staged = Clip::decode(&fs::read(game.join(TARGET)).unwrap()).unwrap();
    let skeleton = made_clips::clip_skeleton(&archive, &archive.read(TARGET).unwrap()).unwrap();
    let i = skeleton.names.iter().position(|n| *n == bone).unwrap();
    let last = staged.frames.last().unwrap()[i].rotation;
    let parent = skeleton.parents[i] as usize;
    let world_parent = deadlock_preparation::clip_author::worlds(&skeleton, staged.frames.last().unwrap()).unwrap()[parent].0;
    let world = deadlock_preparation::clip_decode::product(world_parent, last);
    assert!(deadlock_preparation::clip_author::degrees(world, turn(std::f64::consts::FRAC_PI_2)) < 0.01);
    // The receipt of a made row names its choices; every other row has no key.
    assert!(serde_json::to_string(&receipt).unwrap().contains("\"made\""));
    assert!(made_clips::same_rows(&[row.clone()], &receipt.replacements));
    let mut changed = row.clone();
    changed.made.as_mut().unwrap().in_place = false;
    assert!(!made_clips::same_rows(&[changed], &receipt.replacements));
    let plain = ResourceReplacement { source: Some("a.vnmclip_c".into()), target: Some(TARGET.into()), ..Default::default() };
    assert!(!made_clips::same_rows(&[plain], &receipt.replacements));
    // No taken key in its receipt, and no other hero's file was read (no
    // test in this file takes a clip from another hero).
    assert!(!serde_json::to_string(&receipt).unwrap().contains("\"taken\""));
    deadlock_components::hero_clips::verify_entries(&archive, &receipt.replacements, &cancel).unwrap();
    assert_eq!(deadlock_components::hero_clips::other_hero_reads(), 0);
}

#[test]
fn a_made_row_without_a_current_packet_stops_with_the_sentence() {
    let dir = tempfile::tempdir().unwrap();
    let archive = with_skeleton(dir.path());
    let cancel = CancellationToken::new();
    let (inputs, row, _) = setup(dir.path(), &archive);
    let (content, game) = (dir.path().join("content"), dir.path().join("game"));
    // No inputs (a caller that does not know the workspace).
    let error = motion::stage_with(&archive, MODEL, &[row.clone()], &content, &game, &cancel, true, None).unwrap_err().to_string();
    assert!(error.contains(clip_making::NOT_MADE), "{error}");
    // The Blender file changed since the packet was made.
    fs::write(row.source.as_deref().unwrap(), b"a changed Blender file").unwrap();
    let error = motion::stage_with(&archive, MODEL, &[row.clone()], &content, &game, &cancel, true, Some(&inputs)).unwrap_err().to_string();
    assert!(error.contains(clip_making::NOT_MADE), "{error}");
    // A preparation without a fit.
    let none = made_clips::MadeInputs { project_root: dir.path().into(), fit: Err(clip_making::NO_FIT.into()) };
    let error = motion::stage_with(&archive, MODEL, &[row], &content, &game, &cancel, true, Some(&none)).unwrap_err().to_string();
    assert!(error.contains(clip_making::NO_FIT), "{error}");
}

#[test]
fn switches_off_keep_still_tracks_still_and_ranges_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let archive = with_skeleton(dir.path());
    let cancel = CancellationToken::new();
    let (inputs, row, _) = setup(dir.path(), &archive);
    let off = clip_making::Switches { new_fixed_values: false, refit_ranges: false, still_to_moving: false, ..SWITCHES };
    let authored = made_clips::author_with(&archive, TARGET, &row, &inputs, off, &cancel).unwrap();
    let r = &authored.report;
    assert!(r.still_made_moving.is_empty() && r.still_new_fixed_value.is_empty() && r.ranges_refitted.is_empty(), "{r:?}");
    // The turned bone's rotation is a still track in the game's clip: held at its first frame.
    assert_eq!(r.still_held, vec!["bone_1".to_string()], "{r:?}");
    let held = Clip::decode(&authored.bytes).unwrap();
    assert!(held.frames.iter().all(|f| deadlock_preparation::clip_author::degrees(f[1].rotation, held.frames[0][1].rotation) < 1e-6));
    // On (the default): it moves.
    let on = made_clips::author_with(&archive, TARGET, &row, &inputs, SWITCHES, &cancel).unwrap();
    assert_eq!(on.report.switches, Some(SWITCHES));
    assert_eq!(on.report.still_made_moving, vec!["bone_1".to_string()], "{:?}", on.report);
    assert!(on.report.still_held.is_empty());
    // Only the moving switch off: still held, so no still track starts moving.
    let still = Switches { still_to_moving: false, ..SWITCHES };
    let only = made_clips::author_with(&archive, TARGET, &row, &inputs, still, &cancel).unwrap();
    assert!(only.report.still_made_moving.is_empty() && only.report.still_held == vec!["bone_1".to_string()], "{:?}", only.report);
}

/// The made clip's bytes with the app's switches, pinned: the digest was
/// taken with the code before the floor lift and the off hand existed. With
/// the floor lift on (the app's switches) and the off hand on as well, a clip
/// that has no feet and no gun hand is made exactly the same (nothing to
/// lift, the pass says why it did not run).
#[test]
fn the_new_switches_change_nothing_while_off_or_where_they_do_not_apply() {
    let dir = tempfile::tempdir().unwrap();
    let archive = with_skeleton(dir.path());
    let cancel = CancellationToken::new();
    let (inputs, row, _) = setup(dir.path(), &archive);
    let app = made_clips::author_with(&archive, TARGET, &row, &inputs, SWITCHES, &cancel).unwrap();
    assert!(SWITCHES.floor_lift && !SWITCHES.off_hand);
    assert_eq!(made_clips::digest(&app.bytes), "13254547f1a9660f8e128b9b3edb5a2ed72ddf3525ba2c7b80e3fa1a5163f026", "the made clip's bytes changed");
    assert_eq!(app.report.floor_lift_inches, None, "no feet: nothing lifted");
    let report = serde_json::to_value(&app.report).unwrap();
    assert_eq!(report["switches"]["floor_lift"], serde_json::json!(true));
    assert!(report["switches"].get("off_hand").is_none() && report.get("floor_lift_inches").is_none() && report.get("off_hand").is_none());
    let lift_off = made_clips::author_with(&archive, TARGET, &row, &inputs, Switches { floor_lift: false, ..SWITCHES }, &cancel).unwrap();
    assert_eq!(lift_off.bytes, app.bytes);
    let held = made_clips::author_with(&archive, TARGET, &row, &inputs, Switches { still_to_moving: false, ..SWITCHES }, &cancel).unwrap();
    assert_eq!(made_clips::digest(&held.bytes), "e518478c69138ff37fc72e6d2c6c4a4683b2eb500019864b3f95bddd76f94c19", "the made clip's bytes changed (still tracks held)");
    let on = Switches { floor_lift: true, off_hand: true, ..SWITCHES };
    let both = made_clips::author_with(&archive, TARGET, &row, &inputs, on, &cancel).unwrap();
    assert_eq!(both.bytes, app.bytes);
    assert_eq!(both.report.floor_lift_inches, None);
    assert_eq!((both.report.off_hand.as_ref(), both.report.off_hand_skipped.as_deref()), (None, Some(deadlock_preparation::clip_author::OFF_HAND_NO_GUN_HAND)));
}

#[test]
fn an_additive_clip_is_refused_before_and_while_making() {
    let dir = tempfile::tempdir().unwrap();
    let archive = with_clip(dir.path(), true);
    let cancel = CancellationToken::new();
    assert_eq!(made_clips::refusal(&archive, TARGET).unwrap(), Some(clip_making::ADDITIVE));
    let (inputs, row, _) = setup(dir.path(), &archive);
    let error = made_clips::author_row(&archive, TARGET, &row, &inputs, &cancel).err().unwrap().to_string();
    assert!(error.contains(clip_making::ADDITIVE), "{error}");
    // The ordinary clip can be made.
    let other = tempfile::tempdir().unwrap();
    assert_eq!(made_clips::refusal(&with_skeleton(other.path()), TARGET).unwrap(), None);
}
