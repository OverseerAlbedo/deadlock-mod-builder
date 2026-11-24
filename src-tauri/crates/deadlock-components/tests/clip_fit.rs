//! The Motion page's fit checks are a presentation of the build's own clip
//! comparison: a replacement fails a check exactly when `validate_clip` (and
//! so the build) refuses it, and the page's reason is the build's message.
use deadlock_components::motion::{self, FitReport};
use deadlock_core::CancellationToken;
use deadlock_resource::{binary, container, value::Kind, Value};
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
fn read(name: &str) -> Vec<u8> {
    fs::read(fixture(name)).unwrap()
}
/// `clip.bin` with its DATA changed by `edit`.
fn edited(edit: impl FnOnce(&mut Value)) -> Vec<u8> {
    let base = read("clip.bin");
    let layout = container::layout(&base).unwrap();
    let block = container::unique(&layout.blocks, "DATA").unwrap();
    let mut doc = binary::decode(container::payload(&base, block).unwrap()).unwrap();
    edit(&mut doc.root);
    let data = binary::encode(&doc.root, doc.format_guid).unwrap();
    container::rebuild(&base, &BTreeMap::from([("DATA".to_owned(), data)])).unwrap()
}
fn set(root: &mut Value, key: &str, value: Value) {
    let Kind::Object(rows) = &mut root.kind else { panic!("clip DATA is an object") };
    match rows.iter_mut().find(|(k, _)| k == key) {
        Some((_, v)) => *v = value,
        None => rows.push((key.to_owned(), value)),
    }
}
fn failed(report: &FitReport) -> Vec<&str> {
    report.checks.iter().filter(|c| c.status == "fail").map(|c| c.id.as_str()).collect()
}
/// The report against the build's verdict on the same bytes.
fn agree(original: &[u8], replacement: &[u8]) -> FitReport {
    let cancel = CancellationToken::new();
    let verdict = motion::validate_clip(original, replacement, &cancel);
    let report = motion::compiled_fit(original, replacement, &cancel).unwrap();
    assert_eq!(report.fits, verdict.is_ok());
    assert_eq!(!failed(&report).is_empty(), verdict.is_err());
    match verdict {
        Ok(_) => assert_eq!(report.reason, None),
        Err(error) => assert_eq!(report.reason.as_deref(), Some(motion::said(&error).as_str())),
    }
    report
}

#[test]
fn an_unchanged_or_pose_only_replacement_passes_every_check() {
    let original = read("clip.bin");
    for replacement in [original.clone(), read("clip-replaced.bin")] {
        let report = agree(&original, &replacement);
        assert_eq!(report.kind, "compiled");
        assert!(report.checks.iter().all(|c| c.status == "pass"), "{:?}", report.checks);
        assert_eq!(report.checks.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["skeleton", "length", "events", "unchanged"]);
    }
}

#[test]
fn each_changed_part_fails_its_own_check_with_the_builds_message() {
    let original = read("clip.bin");
    let number = |n: f64| Value::new(Kind::F32(n as f32));
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("length", edited(|v| set(v, "m_flDuration", number(2.0)))),
        ("skeleton", edited(|v| set(v, "m_skeleton", Value::new(Kind::String("animation/test/other.vnmskel".into()))))),
        ("events", edited(|v| set(v, "m_events", Value::new(Kind::Array(vec![Value::new(Kind::Object(vec![("_class".into(), Value::new(Kind::String("CNmFootEvent".into())))]))]))))),
        ("unchanged", edited(|v| set(v, "m_dmbExtra", Value::new(Kind::Bool(true))))),
    ];
    for (check, replacement) in cases {
        let report = agree(&original, &replacement);
        assert_eq!(failed(&report), [check], "{check}: {:?}", report.checks);
        assert_eq!(report.reason.as_deref(), Some(motion::METADATA_CHANGED));
    }
    // A root motion the original does not have.
    let report = agree(&original, &read("clip-bad-root.bin"));
    assert!(!report.fits);
    assert!(!failed(&report).is_empty());
}

#[test]
fn an_unreadable_replacement_is_a_report_with_the_builds_message() {
    let original = read("clip.bin");
    let cancel = CancellationToken::new();
    for replacement in [read("clip-bad-offset.bin"), b"not a clip".to_vec()] {
        let error = motion::validate_clip(&original, &replacement, &cancel).unwrap_err();
        let report = motion::compiled_fit(&original, &replacement, &cancel).unwrap();
        assert_eq!(report.kind, "unreadable");
        assert!(!report.fits);
        assert_eq!(report.reason, Some(motion::said(&error)));
        assert_eq!(failed(&report), ["readable"]);
    }
    let rotation = read("clip-rotation.bin");
    let report = motion::compiled_fit(&rotation, &read("clip-bad-rotation.bin"), &cancel).unwrap();
    assert_eq!(report.kind, "unreadable");
}

#[test]
fn fit_reads_the_game_clip_and_dispatches_on_the_file_kind() {
    let archive = Vpk::open(fixture("components_dir.vpk")).unwrap();
    let cancel = CancellationToken::new();
    let dir = tempfile::tempdir().unwrap();
    let compiled = dir.path().join("mine.vnmclip_c");
    fs::write(&compiled, read("clip-replaced.bin")).unwrap();
    let report = motion::fit(&archive, MODEL, TARGET, &compiled, &cancel).unwrap();
    assert!(report.fits && report.kind == "compiled");
    let other = dir.path().join("mine.fbx");
    fs::write(&other, b"fbx").unwrap();
    let report = motion::fit(&archive, MODEL, TARGET, &other, &cancel).unwrap();
    assert_eq!((report.kind.as_str(), report.fits), ("unsupported", false));
    assert_eq!(report.reason.as_deref(), Some(motion::UNSUPPORTED_SOURCE));
    let missing = motion::fit(&archive, MODEL, TARGET, &dir.path().join("gone.vnmclip_c"), &cancel).unwrap();
    assert_eq!(missing.kind, "unreadable");
    assert!(motion::fit(&archive, MODEL, "models/heroes_wip/test/none.vnmclip_c", &compiled, &cancel).is_err());
}

#[test]
fn an_authored_clip_runs_the_checks_the_build_runs_before_compiling() {
    let archive = Vpk::open(fixture("components_dir.vpk")).unwrap();
    let cancel = CancellationToken::new();
    let report = motion::fit(&archive, MODEL, TARGET, &fixture("action.vnmclip"), &cancel).unwrap();
    assert_eq!(report.kind, "authored");
    assert!(report.fits, "{:?}", report);
    let statuses: Vec<_> = report.checks.iter().map(|c| c.status.as_str()).collect();
    assert_eq!(statuses, ["pass", "pass", "pass", "pass", "pass", "after_compile", "after_compile", "after_compile", "after_compile"]);
    // Another skeleton: the build's own message, later steps not run.
    let dir = tempfile::tempdir().unwrap();
    fs::copy(fixture("animation.dmx"), dir.path().join("animation.dmx")).unwrap();
    let clip = dir.path().join("other.vnmclip");
    fs::write(&clip, "{ m_sourceFilename = \"animation.dmx\" m_animationSkeletonName = \"animation/test/other.vnmskel\" m_eventTracks = [] }").unwrap();
    let report = motion::fit(&archive, MODEL, TARGET, &clip, &cancel).unwrap();
    assert!(!report.fits);
    let statuses: Vec<_> = report.checks.iter().take(5).map(|c| (c.id.as_str(), c.status.as_str())).collect();
    assert_eq!(statuses, [("document", "pass"), ("animation", "pass"), ("skeleton_name", "fail"), ("additive_base", "not_checked"), ("skeletons_in_game", "not_checked")]);
    let staged = motion::stage(
        &archive,
        MODEL,
        &[deadlock_project::customization::ResourceReplacement { source: Some(clip.to_string_lossy().into()), target: Some(TARGET.into()), ..Default::default() }],
        &dir.path().join("content"),
        &dir.path().join("game"),
        &cancel,
        true,
    )
    .unwrap_err();
    assert_eq!(report.reason, Some(motion::said(&staged)));
    // A DMX that is missing fails the first check.
    let lone = dir.path().join("lone.vnmclip");
    fs::write(&lone, "{ m_sourceFilename = \"none.dmx\" m_animationSkeletonName = \"animation/test/hero.vnmskel\" }").unwrap();
    let report = motion::fit(&archive, MODEL, TARGET, &lone, &cancel).unwrap();
    assert_eq!(report.checks[0].status, "fail");
    assert!(report.checks[1..5].iter().all(|c| c.status == "not_checked"));
}

#[test]
fn event_marks_use_the_tag_word_and_the_clip_fraction() {
    let root = |events: Vec<Value>| Value::new(Kind::Object(vec![("m_events".into(), Value::new(Kind::Array(events)))]));
    let event = |class: &str, at: f64, extra: Vec<(&str, &str)>| {
        let mut rows = vec![
            ("_class".to_string(), Value::new(Kind::String(class.into()))),
            ("m_flStartTime".to_string(), Value::new(Kind::Object(vec![("m_flValue".into(), Value::new(Kind::F32(at as f32)))]))),
        ];
        rows.extend(extra.into_iter().map(|(k, v)| (k.to_string(), Value::new(Kind::String(v.into())))));
        Value::new(Kind::Object(rows))
    };
    let marks = motion::event_marks(&root(vec![
        event("CNmTaggedSoundEvent", 0.25, vec![("m_name", "footstep")]),
        event("CNmTaggedSoundEvent", 0.5, vec![("m_name", "mvmt_short")]),
        event("CNmLegacyEvent", 0.75, vec![("m_animEventClassName", "AE_MELEE_ATTACK")]),
        event("CNmFootEvent", 1.5, vec![]),
    ]));
    let words: Vec<_> = marks.iter().map(|m| (m.kind.as_str(), m.word.as_str())).collect();
    assert_eq!(words, [("sound", "Footstep"), ("sound", "Movement"), ("game", "Melee attack"), ("foot", "Foot")]);
    assert!((marks[0].at - 0.25).abs() < 1e-6);
    assert_eq!(marks[3].at, 1.0);
}
