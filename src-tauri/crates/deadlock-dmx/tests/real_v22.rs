//! Large fixture is supplied separately; never silently substitute a synthetic fixture.
use deadlock_core::CancellationToken;
use deadlock_dmx::{split, Dmx};
use std::{fs, path::PathBuf};
#[test]
#[ignore = "requires independently recovered 222 MB V22 fixture and Python oracle"]
fn recovered_v22_summary_and_full_selected_split_match_python() {
    let folder = PathBuf::from(std::env::var("DMB_V22_FIXTURE").expect("DMB_V22_FIXTURE required"));
    let source = folder.join("Collection.dmx");
    let d = Dmx::load(&source).unwrap();
    let oracle: serde_json::Value =
        serde_json::from_slice(&fs::read(folder.join("rust-oracle.json")).unwrap()).unwrap();
    assert_eq!(
        serde_json::to_value(d.summary().unwrap()).unwrap(),
        oracle["summary"]
    );
    assert_eq!(
        serde_json::to_value(d.geometry_counts().unwrap()).unwrap(),
        oracle["geometry"]
    );
    let out = tempfile::tempdir().unwrap();
    let report = split::split_file(
        &source,
        &["body".into()],
        out.path(),
        400000,
        &CancellationToken::new(),
    )
    .unwrap();
    let directory = report.directory.as_ref().unwrap();
    let mut actual = serde_json::to_value(&report).unwrap();
    actual.as_object_mut().unwrap().remove("source_file");
    actual.as_object_mut().unwrap().remove("directory");
    assert_eq!(actual, oracle["split_report"]);
    for p in &report.groups[0].pieces {
        assert_eq!(
            deadlock_core::safety::digest(&directory.join(&p.file)).unwrap(),
            oracle["piece_hashes"][&p.file].as_str().unwrap()
        );
    }
}
